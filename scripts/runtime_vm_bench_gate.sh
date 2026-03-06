#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

BENCH_OUT_DIR="${TRUST_VM_BENCH_ARTIFACT_DIR:-${OUT_DIR:-target/gate-artifacts/runtime-vm-bench}}"
PROFILE="${TRUST_VM_BENCH_PROFILE:-quick}"
ENFORCE_THRESHOLDS="${TRUST_VM_BENCH_ENFORCE_THRESHOLDS:-0}"
CARGO_PROFILE="${TRUST_VM_BENCH_CARGO_PROFILE:-debug}"

MEDIAN_RATIO_MAX="${TRUST_VM_MEDIAN_RATIO_MAX:-0.50}"
P99_RATIO_MAX="${TRUST_VM_P99_RATIO_MAX:-0.70}"
THROUGHPUT_RATIO_MIN="${TRUST_VM_THROUGHPUT_RATIO_MIN:-2.00}"

case "${PROFILE}" in
  quick)
    SAMPLES="${TRUST_VM_BENCH_SAMPLES:-400}"
    WARMUP_CYCLES="${TRUST_VM_BENCH_WARMUP_CYCLES:-40}"
    ;;
  full)
    SAMPLES="${TRUST_VM_BENCH_SAMPLES:-2000}"
    WARMUP_CYCLES="${TRUST_VM_BENCH_WARMUP_CYCLES:-200}"
    ;;
  *)
    echo "[vm-bench-gate] FAIL: unsupported profile '${PROFILE}' (expected quick|full)"
    exit 1
    ;;
esac

mkdir -p "${BENCH_OUT_DIR}"

# Avoid leaking benchmark artifact OUT_DIR into cargo/rustc build script env.
unset OUT_DIR

case "${CARGO_PROFILE}" in
  debug|release)
    ;;
  *)
    echo "[vm-bench-gate] FAIL: unsupported TRUST_VM_BENCH_CARGO_PROFILE='${CARGO_PROFILE}' (expected debug|release)"
    exit 1
    ;;
esac

cargo_args=(run -p trust-runtime --bin trust-runtime --features legacy-interpreter --)
if [[ "${CARGO_PROFILE}" == "release" ]]; then
  cargo_args=(run --release -p trust-runtime --bin trust-runtime --features legacy-interpreter --)
fi

echo "[vm-bench-gate] running trust-runtime bench execution-backend (profile=${PROFILE}, cargo_profile=${CARGO_PROFILE})"
started_ns="$(date +%s%N)"
cargo "${cargo_args[@]}" \
  bench execution-backend \
  --samples "${SAMPLES}" \
  --warmup-cycles "${WARMUP_CYCLES}" \
  --output json \
  > "${BENCH_OUT_DIR}/execution-backend.json"
duration_ms="$(( ($(date +%s%N) - started_ns) / 1000000 ))"

read_json_number() {
  local file="$1"
  local query="$2"
  jq -r "${query}" "${file}"
}

float_le() {
  local left="$1"
  local right="$2"
  awk -v a="${left}" -v b="${right}" 'BEGIN { exit (a <= b) ? 0 : 1 }'
}

float_ge() {
  local left="$1"
  local right="$2"
  awk -v a="${left}" -v b="${right}" 'BEGIN { exit (a >= b) ? 0 : 1 }'
}

MEDIAN_RATIO="$(read_json_number "${BENCH_OUT_DIR}/execution-backend.json" '.report.aggregate.median_latency_ratio')"
P99_RATIO="$(read_json_number "${BENCH_OUT_DIR}/execution-backend.json" '.report.aggregate.p99_latency_ratio')"
THROUGHPUT_RATIO="$(read_json_number "${BENCH_OUT_DIR}/execution-backend.json" '.report.aggregate.throughput_ratio')"

RESULT="recorded"
if [[ "${ENFORCE_THRESHOLDS}" == "1" ]]; then
  if ! float_le "${MEDIAN_RATIO}" "${MEDIAN_RATIO_MAX}"; then
    echo "[vm-bench-gate] FAIL: median latency ratio ${MEDIAN_RATIO} exceeds ${MEDIAN_RATIO_MAX}"
    exit 1
  fi
  if ! float_le "${P99_RATIO}" "${P99_RATIO_MAX}"; then
    echo "[vm-bench-gate] FAIL: p99 latency ratio ${P99_RATIO} exceeds ${P99_RATIO_MAX}"
    exit 1
  fi
  if ! float_ge "${THROUGHPUT_RATIO}" "${THROUGHPUT_RATIO_MIN}"; then
    echo "[vm-bench-gate] FAIL: throughput ratio ${THROUGHPUT_RATIO} below ${THROUGHPUT_RATIO_MIN}"
    exit 1
  fi
  RESULT="pass"
fi

cat > "${BENCH_OUT_DIR}/summary.md" <<MD
# MP-060 Runtime Execution Backend Benchmark

- profile: ${PROFILE}
- samples per fixture: ${SAMPLES}
- warmup cycles: ${WARMUP_CYCLES}
- aggregate median latency ratio (vm/interpreter): ${MEDIAN_RATIO} (target <= ${MEDIAN_RATIO_MAX})
- aggregate p99 latency ratio (vm/interpreter): ${P99_RATIO} (target <= ${P99_RATIO_MAX})
- aggregate throughput ratio (vm/interpreter): ${THROUGHPUT_RATIO} (target >= ${THROUGHPUT_RATIO_MIN})
- thresholds enforced: ${ENFORCE_THRESHOLDS}
- cargo profile: ${CARGO_PROFILE}
- benchmark duration_ms: ${duration_ms}
- result: ${RESULT}
MD

jq -n \
  --arg profile "${PROFILE}" \
  --argjson samples "${SAMPLES}" \
  --argjson warmup_cycles "${WARMUP_CYCLES}" \
  --arg median_ratio "${MEDIAN_RATIO}" \
  --arg p99_ratio "${P99_RATIO}" \
  --arg throughput_ratio "${THROUGHPUT_RATIO}" \
  --arg median_ratio_max "${MEDIAN_RATIO_MAX}" \
  --arg p99_ratio_max "${P99_RATIO_MAX}" \
  --arg throughput_ratio_min "${THROUGHPUT_RATIO_MIN}" \
  --arg enforced "${ENFORCE_THRESHOLDS}" \
  --arg cargo_profile "${CARGO_PROFILE}" \
  --arg duration_ms "${duration_ms}" \
  --arg result "${RESULT}" \
  '{
    profile: $profile,
    cargo_profile: $cargo_profile,
    samples_per_fixture: $samples,
    warmup_cycles: $warmup_cycles,
    duration_ms: ($duration_ms | tonumber),
    aggregate_ratios: {
      median_latency_vm_over_interpreter: ($median_ratio | tonumber),
      p99_latency_vm_over_interpreter: ($p99_ratio | tonumber),
      throughput_vm_over_interpreter: ($throughput_ratio | tonumber)
    },
    thresholds: {
      median_latency_max: ($median_ratio_max | tonumber),
      p99_latency_max: ($p99_ratio_max | tonumber),
      throughput_min: ($throughput_ratio_min | tonumber)
    },
    thresholds_enforced: ($enforced == "1"),
    result: $result
  }' > "${BENCH_OUT_DIR}/summary.json"

echo "[vm-bench-gate] ${RESULT^^}"
