#!/bin/bash
# Fixture tests for the relocation SMART gate. Reads sanitized smartctl JSON
# only; it starts no copy, touches no service and needs no destination host.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=../../lib/relocate-smart-gate.sh
. "$here/../../lib/relocate-smart-gate.sh"

SERIAL=WD-SANITIZED-0001
pass=0
fail=0

check() { # <fixture> <accept|reject> <expected-reason-substring>
  local fixture=$1 expect=$2 reason=$3 out status
  set +e
  out=$(smart_gate_evaluate "$SERIAL" < "$here/fixtures/$fixture" 2>&1)
  status=$?
  set -e
  local got=reject
  [ "$status" -eq 0 ] && got=accept
  if [ "$got" = "$expect" ] && [[ "$out" == *"$reason"* ]]; then
    printf 'ok   %-28s %s\n' "$fixture" "$out"
    pass=$((pass + 1))
  else
    printf 'FAIL %-28s expected %s/%s, got %s/%s\n' \
      "$fixture" "$expect" "$reason" "$got" "$out"
    fail=$((fail + 1))
  fi
}

check clean-completed.json      accept extended-self-test-completed-without-error
check failed-completed.json     reject extended-self-test-failed
check absent-extended.json      reject extended-self-test-absent
check in-progress.json          reject extended-self-test-in-progress
check nonzero-counter.json      reject attribute-197-nonzero
check missing-counter.json      reject attribute-198-missing
check missing-field.json        reject smart-status-missing
check health-failed.json        reject smart-status-failed
check serial-mismatch.json      reject serial-mismatch
check stale-extended.json       reject extended-self-test-stale
check unsupported-protocol.json reject unsupported-device-protocol
check unparseable.json          reject smart-json-unparseable

# The relocation script waits on some rejections and aborts on the rest. If a
# reason it waits on is no longer a reason the evaluator emits, the migration
# would abort on a merely pending test, so the vocabulary is checked here.
script=$here/../../universe-ord-dogecoin-full-relocate
for reason in extended-self-test-absent extended-self-test-in-progress; do
  if ! grep -q -- "$reason" "$script"; then
    printf "FAIL %-28s relocation script no longer waits on %s
" drift-guard "$reason"
    fail=$((fail + 1))
  elif ! grep -q -- "$reason" "$here/../../lib/relocate-smart-gate.py"; then
    printf "FAIL %-28s evaluator no longer emits %s
" drift-guard "$reason"
    fail=$((fail + 1))
  else
    printf "ok   %-28s %s is emitted and waited on
" drift-guard "$reason"
    pass=$((pass + 1))
  fi
done

# The elapsed-time shortcut must stay gone.
if grep -q "10% of test remaining|10pct-since" "$script"; then
  printf "FAIL %-28s the elapsed-time SMART shortcut is back
" no-shortcut
  fail=$((fail + 1))
else
  printf "ok   %-28s no elapsed-time shortcut
" no-shortcut
  pass=$((pass + 1))
fi

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
