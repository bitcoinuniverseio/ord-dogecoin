import base64
import hashlib
import json
import logging
import os
import time
from pathlib import Path

import boto3
from botocore.exceptions import ClientError


LOG = logging.getLogger()
LOG.setLevel(logging.INFO)

REGION = os.environ["REGION"]
AZ = os.environ["AVAILABILITY_ZONE"]
SUBNET_ID = os.environ["SUBNET_ID"]
VOLUME_ID = os.environ["VOLUME_ID"]
EIP_ALLOCATION_ID = os.environ["EIP_ALLOCATION_ID"]
LAUNCH_TEMPLATE_ID = os.environ["LAUNCH_TEMPLATE_ID"]
CANDIDATE_TYPES = [value.strip() for value in os.environ["CANDIDATE_TYPES"].split(",") if value.strip()]
PROJECT_TAG = os.environ.get("PROJECT_TAG", "doge-index-catchup")
LAUNCH_ENABLED = os.environ.get("LAUNCH_ENABLED", "false").lower() == "true"
LOCK_TABLE = os.environ["LOCK_TABLE"]

ec2 = boto3.client("ec2", region_name=REGION)
dynamodb = boto3.client("dynamodb", region_name=REGION)


def bootstrap_launch_template_version():
    source = Path(__file__).with_name("bootstrap.sh").read_text(encoding="utf-8")
    source = source.replace("__VOLUME_ID__", VOLUME_ID).replace("__REGION__", REGION)
    digest = hashlib.sha256(source.encode("utf-8")).hexdigest()[:16]
    description = f"doge-bootstrap-{digest}"
    versions = ec2.describe_launch_template_versions(
        LaunchTemplateId=LAUNCH_TEMPLATE_ID,
        Versions=["$Latest"],
    )["LaunchTemplateVersions"]
    latest = versions[0]
    if latest.get("VersionDescription") == description:
        return str(latest["VersionNumber"])

    created = ec2.create_launch_template_version(
        LaunchTemplateId=LAUNCH_TEMPLATE_ID,
        SourceVersion=str(latest["VersionNumber"]),
        VersionDescription=description,
        LaunchTemplateData={"UserData": base64.b64encode(source.encode("utf-8")).decode("ascii")},
    )["LaunchTemplateVersion"]
    version = str(created["VersionNumber"])
    ec2.modify_launch_template(LaunchTemplateId=LAUNCH_TEMPLATE_ID, DefaultVersion=version)
    LOG.info("Installed bootstrap launch template version %s", version)
    return version


def describe_volume():
    response = ec2.describe_volumes(VolumeIds=[VOLUME_ID])
    if len(response["Volumes"]) != 1:
        raise RuntimeError(f"Expected one persistent volume, found {len(response['Volumes'])}")
    volume = response["Volumes"][0]
    tags = {tag["Key"]: tag["Value"] for tag in volume.get("Tags", [])}
    if volume["AvailabilityZone"] != AZ:
        raise RuntimeError(f"Volume {VOLUME_ID} is in {volume['AvailabilityZone']}, expected {AZ}")
    if tags.get("Project") != PROJECT_TAG or tags.get("Persistence") != "retain":
        raise RuntimeError(f"Volume {VOLUME_ID} is missing required retention tags")
    return volume


def active_instances():
    response = ec2.describe_instances(
        Filters=[
            {"Name": "tag:Project", "Values": [PROJECT_TAG]},
            {"Name": "instance-state-name", "Values": ["pending", "running", "stopping", "stopped"]},
        ]
    )
    instances = [instance for reservation in response["Reservations"] for instance in reservation["Instances"]]
    instances.sort(key=lambda item: item["LaunchTime"], reverse=True)
    return instances


def require_spot(instance):
    if instance.get("InstanceLifecycle") != "spot":
        raise RuntimeError(
            f"Refusing non-Spot instance {instance['InstanceId']} for persistent index compute"
        )


def attach_and_protect(volume, instance):
    state = instance["State"]["Name"]
    if state not in {"running", "stopped"}:
        return
    attachments = volume.get("Attachments", [])
    if attachments:
        attachment = attachments[0]
        if attachment["InstanceId"] != instance["InstanceId"]:
            LOG.warning("Persistent volume is still attached to %s", attachment["InstanceId"])
            return
    else:
        ec2.attach_volume(VolumeId=VOLUME_ID, InstanceId=instance["InstanceId"], Device="/dev/sdf")
        LOG.info("Attached %s to %s", VOLUME_ID, instance["InstanceId"])
        return

    try:
        ec2.modify_instance_attribute(
            InstanceId=instance["InstanceId"],
            BlockDeviceMappings=[{"DeviceName": "/dev/sdf", "Ebs": {"DeleteOnTermination": False}}],
        )
    except ClientError as error:
        LOG.warning("DeleteOnTermination confirmation will be retried: %s", error)


def associate_eip(instance):
    if instance["State"]["Name"] != "running":
        return
    addresses = ec2.describe_addresses(AllocationIds=[EIP_ALLOCATION_ID])["Addresses"]
    address = addresses[0]
    if address.get("InstanceId") == instance["InstanceId"]:
        return
    ec2.associate_address(
        AllocationId=EIP_ALLOCATION_ID,
        InstanceId=instance["InstanceId"],
        AllowReassociation=True,
    )
    LOG.info("Associated persistent public address with %s", instance["InstanceId"])


def launch_spot_instance():
    if not LAUNCH_ENABLED:
        LOG.info("Spot launch is disabled")
        return None

    offerings = ec2.describe_instance_type_offerings(
        LocationType="availability-zone",
        Filters=[
            {"Name": "location", "Values": [AZ]},
            {"Name": "instance-type", "Values": CANDIDATE_TYPES},
        ],
    )["InstanceTypeOfferings"]
    available = {item["InstanceType"] for item in offerings}
    overrides = [
        {"InstanceType": instance_type, "SubnetId": SUBNET_ID, "Priority": float(position + 1)}
        for position, instance_type in enumerate(CANDIDATE_TYPES)
        if instance_type in available
    ]
    if not overrides:
        raise RuntimeError(f"No configured Spot candidate is offered in {AZ}")

    launch_template_version = bootstrap_launch_template_version()
    response = ec2.create_fleet(
        Type="instant",
        LaunchTemplateConfigs=[
            {
                "LaunchTemplateSpecification": {
                    "LaunchTemplateId": LAUNCH_TEMPLATE_ID,
                    "Version": launch_template_version,
                },
                "Overrides": overrides,
            }
        ],
        TargetCapacitySpecification={
            "TotalTargetCapacity": 1,
            "DefaultTargetCapacityType": "spot",
            "OnDemandTargetCapacity": 0,
            "SpotTargetCapacity": 1,
        },
        SpotOptions={
            "AllocationStrategy": "capacity-optimized-prioritized",
            "InstanceInterruptionBehavior": "terminate",
            "SingleInstanceType": False,
            "SingleAvailabilityZone": True,
        },
        TagSpecifications=[
            {
                "ResourceType": "fleet",
                "Tags": [
                    {"Key": "Name", "Value": "doge-index-spot-fleet"},
                    {"Key": "Project", "Value": PROJECT_TAG},
                    {"Key": "Capacity", "Value": "spot-only"},
                ],
            }
        ],
    )
    errors = response.get("Errors", [])
    instances = response.get("Instances", [])
    if errors:
        LOG.warning("Spot capacity request returned errors: %s", json.dumps(errors, default=str))
    if not instances:
        LOG.info("No compatible Spot capacity is currently available in %s", AZ)
        return None
    instance_ids = [item["InstanceIds"][0] for item in instances if item.get("InstanceIds")]
    LOG.info("Launched Spot instance candidates: %s", instance_ids)
    return instance_ids[0] if instance_ids else None


def reconcile(event):
    LOG.info("Reconcile event: %s", json.dumps(event, default=str))
    volume = describe_volume()
    instances = active_instances()
    if len(instances) > 1:
        LOG.warning("Multiple active Doge compute instances found: %s", [item["InstanceId"] for item in instances])

    if instances:
        instance = instances[0]
        require_spot(instance)
        associate_eip(instance)
        attach_and_protect(volume, instance)
        return {"status": "reconciled", "instanceId": instance["InstanceId"], "volumeId": VOLUME_ID}

    if volume["State"] != "available":
        LOG.info("Waiting for persistent volume state to become available: %s", volume["State"])
        return {"status": "waiting-for-volume", "volumeId": VOLUME_ID}

    instance_id = launch_spot_instance()
    return {"status": "launch-requested" if instance_id else "waiting-for-spot", "instanceId": instance_id}


def handler(event, context):
    owner = context.aws_request_id
    now = int(time.time())
    try:
        dynamodb.put_item(
            TableName=LOCK_TABLE,
            Item={
                "lockKey": {"S": PROJECT_TAG},
                "owner": {"S": owner},
                "expiresAt": {"N": str(now + 180)},
            },
            ConditionExpression="attribute_not_exists(lockKey) OR expiresAt < :now",
            ExpressionAttributeValues={":now": {"N": str(now)}},
        )
    except dynamodb.exceptions.ConditionalCheckFailedException:
        LOG.info("Another controller invocation holds the recovery lease")
        return {"status": "lease-held"}

    try:
        return reconcile(event)
    finally:
        try:
            dynamodb.delete_item(
                TableName=LOCK_TABLE,
                Key={"lockKey": {"S": PROJECT_TAG}},
                ConditionExpression="#owner = :owner",
                ExpressionAttributeNames={"#owner": "owner"},
                ExpressionAttributeValues={":owner": {"S": owner}},
            )
        except ClientError as error:
            LOG.warning("Unable to release controller lease: %s", error)
