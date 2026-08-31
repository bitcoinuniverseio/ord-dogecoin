[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $VolumeId,
    [string] $Region = 'eu-central-1',
    [string] $AvailabilityZone = 'eu-central-1a',
    [string] $SubnetId = 'subnet-0ff90da92caa8f00e',
    [string] $VpcId = 'vpc-0a689eb2541b99806',
    [Parameter(Mandatory)] [string] $AmiId,
    [string] $SourceCidr = '159.195.109.76/32',
    [string] $OperatorCidr = '65.108.197.168/32',
    [string] $CandidateTypes = 'm8i.8xlarge,m8a.8xlarge,c8i.8xlarge,c8a.8xlarge,m7i.8xlarge,m7a.8xlarge,c7i.8xlarge,c7a.8xlarge',
    [ValidateSet('true', 'false')] [string] $LaunchEnabled = 'false',
    [ValidateSet('ENABLED', 'DISABLED')] [string] $ControllerScheduleState = 'DISABLED',
    [string] $StackName = 'universe-doge-index-spot'
)

$ErrorActionPreference = 'Stop'
$template = Join-Path $PSScriptRoot 'template.yaml'
$packaged = Join-Path ([System.IO.Path]::GetTempPath()) "doge-index-packaged-$PID.yaml"

try {
    aws cloudformation validate-template --region $Region --template-body "file://$template" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'CloudFormation template validation failed' }

    $account = aws sts get-caller-identity --query Account --output text
    if ($LASTEXITCODE -ne 0 -or -not $account) { throw 'Unable to identify AWS account' }
    $bucket = "universe-doge-index-control-$account-$Region"

    aws s3api head-bucket --bucket $bucket 2>$null
    if ($LASTEXITCODE -ne 0) {
        aws s3api create-bucket --region $Region --bucket $bucket --create-bucket-configuration "LocationConstraint=$Region" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "Unable to create deployment bucket $bucket" }
    }
    aws s3api put-public-access-block --bucket $bucket --public-access-block-configuration BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true | Out-Null
    aws s3api put-bucket-encryption --bucket $bucket --server-side-encryption-configuration '{"Rules":[{"ApplyServerSideEncryptionByDefault":{"SSEAlgorithm":"AES256"},"BucketKeyEnabled":true}]}' | Out-Null
    aws s3api put-bucket-versioning --bucket $bucket --versioning-configuration Status=Enabled | Out-Null
    aws s3api put-bucket-tagging --bucket $bucket --tagging 'TagSet=[{Key=Project,Value=doge-index-catchup},{Key=DataClass,Value=control-artifacts}]' | Out-Null

    aws iam get-role --role-name AWSServiceRoleForEC2Spot 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) {
        aws iam create-service-linked-role --aws-service-name spot.amazonaws.com | Out-Null
    }

    aws cloudformation package --region $Region --template-file $template --s3-bucket $bucket --s3-prefix controller --output-template-file $packaged | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'CloudFormation package failed' }

    $parameters = @(
        "VolumeId=$VolumeId",
        "AvailabilityZone=$AvailabilityZone",
        "SubnetId=$SubnetId",
        "VpcId=$VpcId",
        "AmiId=$AmiId",
        "SourceCidr=$SourceCidr",
        "OperatorCidr=$OperatorCidr",
        "CandidateTypes=$CandidateTypes",
        "LaunchEnabled=$LaunchEnabled",
        "ControllerScheduleState=$ControllerScheduleState"
    )
    aws cloudformation deploy --region $Region --stack-name $StackName --template-file $packaged --capabilities CAPABILITY_NAMED_IAM --parameter-overrides $parameters --tags Project=doge-index-catchup Capacity=spot-only
    if ($LASTEXITCODE -ne 0) { throw 'CloudFormation deployment failed' }

    aws cloudformation describe-stacks --region $Region --stack-name $StackName --query 'Stacks[0].Outputs' --output table
}
finally {
    if (Test-Path -LiteralPath $packaged) {
        Remove-Item -LiteralPath $packaged -Force
    }
}
