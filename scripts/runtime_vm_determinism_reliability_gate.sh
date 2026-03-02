#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

OUT_DIR="${OUT_DIR:-target/gate-artifacts/runtime-vm-determinism}"
ITERATIONS="${TRUST_VM_DETERMINISM_ITERATIONS:-3}"
TEST_THREADS="${TRUST_VM_DETERMINISM_TEST_THREADS:-1}"

mkdir -p "${OUT_DIR}"

if [[ "${ITERATIONS}" -lt 2 ]]; then
  echo "[vm-determinism-gate] FAIL: TRUST_VM_DETERMINISM_ITERATIONS must be >= 2"
  exit 1
fi

hash_file() {
  local file="$1"
  sha256sum "${file}" | awk '{print $1}'
}

echo "[vm-determinism-gate] repeat-run differential parity (${ITERATIONS} iterations)"
differential_hashes=()
for run in $(seq 1 "${ITERATIONS}"); do
  log_path="${OUT_DIR}/bytecode-vm-differential-run-${run}.log"
  signature_path="${OUT_DIR}/bytecode-vm-differential-run-${run}.signature"
  json_path="${OUT_DIR}/bytecode-vm-differential-run-${run}.json"

  started_ns="$(date +%s%N)"
  cargo test -p trust-runtime --test bytecode_vm_differential -- --test-threads="${TEST_THREADS}" \
    | tee "${log_path}"
  ended_ns="$(date +%s%N)"
  duration_ms="$(( (ended_ns - started_ns) / 1000000 ))"

  # Capture normalized test result lines (name + status) as repeat-run signature.
  grep '^test differential_' "${log_path}" \
    | sed -E 's/[[:space:]]+/ /g' \
    > "${signature_path}"

  if [[ ! -s "${signature_path}" ]]; then
    echo "[vm-determinism-gate] FAIL: no differential test signatures captured for run ${run}"
    exit 1
  fi

  tests_hash="$(hash_file "${signature_path}")"
  differential_hashes+=("${tests_hash}")

  jq -n \
    --arg run "${run}" \
    --arg duration_ms "${duration_ms}" \
    --arg tests_hash "${tests_hash}" \
    '{
      run: ($run | tonumber),
      duration_ms: ($duration_ms | tonumber),
      tests_hash: $tests_hash
    }' > "${json_path}"
done

reference_hash="${differential_hashes[0]}"
for current_hash in "${differential_hashes[@]}"; do
  if [[ "${current_hash}" != "${reference_hash}" ]]; then
    echo "[vm-determinism-gate] FAIL: differential test-set hash mismatch between runs"
    exit 1
  fi
done

echo "[vm-determinism-gate] reliability regression suite (fault/restart parity contracts)"
runtime_reliability_log="${OUT_DIR}/runtime-reliability.log"
hot_reload_log="${OUT_DIR}/hot-reload.log"

started_ns="$(date +%s%N)"
cargo test -p trust-runtime --test runtime_reliability -- --test-threads=1 | tee "${runtime_reliability_log}"
runtime_reliability_ms="$(( ($(date +%s%N) - started_ns) / 1000000 ))"

started_ns="$(date +%s%N)"
cargo test -p trust-runtime --test hot_reload -- --test-threads=1 | tee "${hot_reload_log}"
hot_reload_ms="$(( ($(date +%s%N) - started_ns) / 1000000 ))"

cat > "${OUT_DIR}/summary.md" <<MD
# MP-060 Determinism and Reliability Gate

- differential repeat-run iterations: ${ITERATIONS}
- differential test-set hash: ${reference_hash}
- runtime_reliability duration_ms: ${runtime_reliability_ms}
- hot_reload duration_ms: ${hot_reload_ms}

Checks:
- repeat-run differential parity: PASS
- fault/restart reliability suites: PASS

Result: PASS
MD

jq -n \
  --arg iterations "${ITERATIONS}" \
  --arg test_threads "${TEST_THREADS}" \
  --arg tests_hash "${reference_hash}" \
  --arg runtime_reliability_ms "${runtime_reliability_ms}" \
  --arg hot_reload_ms "${hot_reload_ms}" \
  '{
    differential_repeat_runs: {
      iterations: ($iterations | tonumber),
      test_threads: ($test_threads | tonumber),
      tests_hash: $tests_hash
    },
    reliability_suites: {
      runtime_reliability_duration_ms: ($runtime_reliability_ms | tonumber),
      hot_reload_duration_ms: ($hot_reload_ms | tonumber)
    },
    result: "pass"
  }' > "${OUT_DIR}/summary.json"

echo "[vm-determinism-gate] PASS"
