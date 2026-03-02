#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

OUT_DIR="${OUT_DIR:-target/gate-artifacts/runtime-vm-differential}"
PROFILE="${TRUST_VM_DIFFERENTIAL_PROFILE:-quick}"
TEST_THREADS="${TRUST_VM_DIFFERENTIAL_TEST_THREADS:-1}"
MAX_SECONDS="${TRUST_VM_DIFFERENTIAL_MAX_SECONDS:-0}"

mkdir -p "${OUT_DIR}"
: > "${OUT_DIR}/timings.csv"

run_and_log() {
  local name="$1"
  shift
  local log_path="${OUT_DIR}/${name}.log"
  started_ns="$(date +%s%N)"
  "$@" 2>&1 | tee "${log_path}" >&2
  ended_ns="$(date +%s%N)"
  echo "$(( (ended_ns - started_ns) / 1000000 ))"
}

duration_ms_total=0
echo "name,duration_ms" >> "${OUT_DIR}/timings.csv"

case "${PROFILE}" in
  quick)
    tests=(
      differential_c1_function_named_default_and_inout_calls
      differential_c2_string_stdlib_dispatch_with_literals
      differential_c3_interface_method_dispatch
      differential_c4_reference_deref_and_nested_field_index_chains
      differential_c5_sizeof_type_and_expression_edge_cases
    )
    for test_name in "${tests[@]}"; do
      echo "[vm-differential-gate] quick subset: ${test_name}"
      run_ms="$(run_and_log \
        "bytecode_vm_differential_${test_name}" \
        cargo test -p trust-runtime --test bytecode_vm_differential "${test_name}" -- --exact --test-threads="${TEST_THREADS}")"
      duration_ms_total="$((duration_ms_total + run_ms))"
      echo "${test_name},${run_ms}" >> "${OUT_DIR}/timings.csv"
    done
    ;;
  full)
    echo "[vm-differential-gate] full differential suite"
    run_ms="$(run_and_log \
      "bytecode_vm_differential_full" \
      cargo test -p trust-runtime --test bytecode_vm_differential -- --test-threads="${TEST_THREADS}")"
    duration_ms_total="$((duration_ms_total + run_ms))"
    echo "full_suite,${run_ms}" >> "${OUT_DIR}/timings.csv"
    ;;
  *)
    echo "[vm-differential-gate] FAIL: unsupported profile '${PROFILE}' (expected quick|full)"
    exit 1
    ;;
esac

duration_seconds="$(awk -v ms="${duration_ms_total}" 'BEGIN { printf "%.3f", ms / 1000.0 }')"

alert_triggered=false
result="pass"
if [[ "${MAX_SECONDS}" != "0" ]]; then
  if ! awk -v elapsed="${duration_seconds}" -v limit="${MAX_SECONDS}" 'BEGIN { exit (elapsed <= limit) ? 0 : 1 }'; then
    alert_triggered=true
    result="fail"
    echo "[vm-differential-gate] ALERT: runtime ${duration_seconds}s exceeds threshold ${MAX_SECONDS}s"
  fi
fi

cat > "${OUT_DIR}/summary.md" <<MD
# MP-060 Differential Gate

- profile: ${PROFILE}
- test_threads: ${TEST_THREADS}
- duration_ms: ${duration_ms_total}
- duration_seconds: ${duration_seconds}
- runtime_alert_threshold_seconds: ${MAX_SECONDS}
- alert_triggered: ${alert_triggered}
- timings_csv: timings.csv

Result: ${result^^}
MD

jq -n \
  --arg profile "${PROFILE}" \
  --arg test_threads "${TEST_THREADS}" \
  --arg duration_ms "${duration_ms_total}" \
  --arg duration_seconds "${duration_seconds}" \
  --arg max_seconds "${MAX_SECONDS}" \
  --argjson alert_triggered "${alert_triggered}" \
  --arg result "${result}" \
  '{
    profile: $profile,
    test_threads: ($test_threads | tonumber),
    duration_ms: ($duration_ms | tonumber),
    duration_seconds: ($duration_seconds | tonumber),
    runtime_alert_threshold_seconds: (
      if $max_seconds == "0" then null else ($max_seconds | tonumber) end
    ),
    alert_triggered: $alert_triggered,
    result: $result
  }' > "${OUT_DIR}/summary.json"

if [[ "${alert_triggered}" == "true" ]]; then
  echo "[vm-differential-gate] FAIL: sustained runtime alert threshold exceeded"
  exit 1
fi

echo "[vm-differential-gate] PASS"
