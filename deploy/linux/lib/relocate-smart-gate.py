#!/usr/bin/env python3
# Structured SMART verdict for the relocation destination disk.
# Reads smartctl JSON on stdin; see relocate-smart-gate.sh for the contract.
import json
import sys

expected_serial, max_age_raw, required_raw = sys.argv[1:4]


def reject(reason):
    print("reject %s" % reason)
    raise SystemExit(1)


def accept(reason):
    print("accept %s" % reason)
    raise SystemExit(0)


try:
    max_age = int(max_age_raw)
    required = [int(part) for part in required_raw.split(",") if part.strip()]
except ValueError:
    reject("gate-configuration-invalid")
if not required:
    reject("gate-configuration-invalid")

raw = sys.stdin.read()
try:
    doc = json.loads(raw)
except Exception:
    reject("smart-json-unparseable")
if not isinstance(doc, dict):
    reject("smart-json-unparseable")

# The verdict is bound to the disk it was read from, so an old capture or a
# reading of the wrong device cannot stand in for the destination.
serial = doc.get("serial_number")
if not isinstance(serial, str) or not serial:
    reject("serial-missing")
if serial != expected_serial:
    reject("serial-mismatch")

# Only ATA attribute semantics are implemented. Anything else fails closed
# rather than being waved through by rules that do not describe it.
protocol = (doc.get("device") or {}).get("protocol")
if protocol != "ATA":
    reject("unsupported-device-protocol")

status = doc.get("smart_status")
if not isinstance(status, dict) or not isinstance(status.get("passed"), bool):
    reject("smart-status-missing")
if status["passed"] is not True:
    reject("smart-status-failed")

log = (doc.get("ata_smart_self_test_log") or {}).get("standard")
table = (log or {}).get("table")
if not isinstance(table, list):
    reject("self-test-log-missing")

# smartctl lists the newest self-test first.
extended = None
for entry in table:
    if not isinstance(entry, dict):
        continue
    kind = (entry.get("type") or {}).get("string")
    if isinstance(kind, str) and "extended" in kind.lower():
        extended = entry
        break
if extended is None:
    reject("extended-self-test-absent")

entry_status = extended.get("status")
if not isinstance(entry_status, dict):
    reject("extended-self-test-status-missing")
descriptor = entry_status.get("string")
if not isinstance(descriptor, str):
    reject("extended-self-test-status-missing")
if "remaining_percent" in entry_status or "in progress" in descriptor.lower():
    reject("extended-self-test-in-progress")
if not isinstance(entry_status.get("passed"), bool):
    reject("extended-self-test-status-missing")
if entry_status["passed"] is not True:
    reject("extended-self-test-failed")
if "completed without error" not in descriptor.lower():
    reject("extended-self-test-failed")

# A test that completed thousands of power-on hours ago says nothing about the
# disk now, so completion is tied to the drive's current power-on time.
lifetime = extended.get("lifetime_hours")
power_on = (doc.get("power_on_time") or {}).get("hours")
if not isinstance(lifetime, int) or isinstance(lifetime, bool):
    reject("extended-self-test-time-missing")
if not isinstance(power_on, int) or isinstance(power_on, bool):
    reject("power-on-time-missing")
age = power_on - lifetime
if age < 0:
    reject("extended-self-test-time-invalid")
if age > max_age:
    reject("extended-self-test-stale")

attributes = (doc.get("ata_smart_attributes") or {}).get("table")
if not isinstance(attributes, list):
    reject("attribute-table-missing")
by_id = {}
for entry in attributes:
    if isinstance(entry, dict) and isinstance(entry.get("id"), int):
        by_id[entry["id"]] = entry

# Every selected counter must be present and zero. One zero row is not proof
# about the others.
for attribute_id in required:
    entry = by_id.get(attribute_id)
    if entry is None:
        reject("attribute-%d-missing" % attribute_id)
    raw_value = entry.get("raw")
    if not isinstance(raw_value, dict):
        reject("attribute-%d-unreadable" % attribute_id)
    value = raw_value.get("value")
    if isinstance(value, bool) or not isinstance(value, int):
        text = raw_value.get("string")
        if not isinstance(text, str) or not text.strip():
            reject("attribute-%d-unreadable" % attribute_id)
        try:
            value = int(text.split()[0])
        except ValueError:
            reject("attribute-%d-unreadable" % attribute_id)
    if value != 0:
        reject("attribute-%d-nonzero" % attribute_id)

accept(
    "serial=%s extended-self-test-completed-without-error age-hours=%d counters-zero=%s"
    % (serial, age, ",".join(str(i) for i in required))
)
