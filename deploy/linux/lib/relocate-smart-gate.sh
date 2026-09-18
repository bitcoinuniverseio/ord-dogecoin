# shellcheck shell=bash
# Structured SMART acceptance for the relocation destination disk.
#
# The gate this replaced keyed on the absence of the string "Self-test routine
# in progress", so a disk that had never been tested passed immediately, and it
# accepted a single zero row among attributes 5/197/198 as proof that all three
# were zero. Both are decided here from parsed smartctl JSON instead, and every
# missing or unreadable field is a rejection.
#
# Usage: smart_gate_evaluate <expected-serial> < smartctl-json
# Prints one verdict line. Returns 0 to accept, 1 to reject.

# How recently the extended self-test must have completed, measured in drive
# power-on hours because smartctl reports self-test age that way.
SMART_GATE_MAX_TEST_AGE_HOURS=${SMART_GATE_MAX_TEST_AGE_HOURS:-168}
# Reallocated sectors, current pending sectors, offline uncorrectable sectors.
SMART_GATE_REQUIRED_ATTRIBUTES=${SMART_GATE_REQUIRED_ATTRIBUTES:-5,197,198}

smart_gate_evaluate() {
  local expected_serial=${1:?expected serial required}
  local here
  here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
  if ! command -v python3 >/dev/null 2>&1; then
    # No interpreter is a rejection, never a pass.
    echo "reject smart-gate-interpreter-missing"
    return 1
  fi
  python3 "$here/relocate-smart-gate.py" \
    "$expected_serial" \
    "$SMART_GATE_MAX_TEST_AGE_HOURS" \
    "$SMART_GATE_REQUIRED_ATTRIBUTES"
}
