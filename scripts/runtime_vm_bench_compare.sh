#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 3 || $# -gt 4 ]]; then
  echo "usage: $0 <run1-execution-backend.json> <run2-execution-backend.json> <run3-execution-backend.json> [out-dir]"
  exit 1
fi

RUN1_JSON="$1"
RUN2_JSON="$2"
RUN3_JSON="$3"
OUT_DIR="${4:-$(dirname "${RUN1_JSON}")}"

for file in "${RUN1_JSON}" "${RUN2_JSON}" "${RUN3_JSON}"; do
  if [[ ! -f "${file}" ]]; then
    echo "[vm-bench-compare] FAIL: missing input ${file}"
    exit 1
  fi
done

mkdir -p "${OUT_DIR}"
OUT_JSON="${OUT_DIR}/compare-3run.json"
OUT_MD="${OUT_DIR}/compare-3run.md"

MEDIAN_RATIO_MAX="${TRUST_VM_MEDIAN_RATIO_MAX:-1.10}"
P99_RATIO_MAX="${TRUST_VM_P99_RATIO_MAX:-1.00}"
THROUGHPUT_RATIO_MIN="${TRUST_VM_THROUGHPUT_RATIO_MIN:-1.00}"
ENFORCE_THRESHOLDS="${TRUST_VM_BENCH_ENFORCE_THRESHOLDS:-0}"

jq -s '
  def mean: if length == 0 then 0 else (add / length) end;
  def median:
    if length == 0 then 0
    else (sort | .[(length / 2 | floor)])
    end;
  def outlier_runs($values):
    ($values | median) as $m
    | if $m <= 0 then []
      else [
        range(0; ($values | length)) as $idx
        | ($values[$idx]) as $v
        | if ($v > ($m * 2)) or ($v < ($m / 2))
          then {
            run: ($idx + 1),
            value: $v,
            median: $m,
            ratio_to_median: ($v / $m)
          }
          else empty
          end
      ]
      end;
  {
    corpus: .[0].report.corpus,
    runs: length,
    runs_files: {
      run1: $run1,
      run2: $run2,
      run3: $run3
    },
    runs_raw_metrics: {
      aggregate: {
        median_latency_ratio: (
          map(
            [.report.fixtures[].median_latency_ratio]
            | sort
            | .[(length / 2 | floor)]
          )
        ),
        p99_latency_ratio: (map(.report.aggregate.p99_latency_ratio)),
        throughput_ratio: (map(.report.aggregate.throughput_ratio))
      },
      fixtures: (
        (.[0].report.fixtures | map(.fixture)) as $fixture_names
        | [
            $fixture_names[] as $name
            | {
                fixture: $name,
                median_latency_ratio: ([.[].report.fixtures[] | select(.fixture == $name) | .median_latency_ratio]),
                p99_latency_ratio: ([.[].report.fixtures[] | select(.fixture == $name) | .p99_latency_ratio]),
                throughput_ratio: ([.[].report.fixtures[] | select(.fixture == $name) | .throughput_ratio])
              }
          ]
      )
    },
    decision_metrics: {
      aggregate: {
        median_latency_ratio: (
          map(
            [.report.fixtures[].median_latency_ratio]
            | sort
            | .[(length / 2 | floor)]
          )
          | median
        ),
        p99_latency_ratio: (map(.report.aggregate.p99_latency_ratio) | median),
        throughput_ratio: (map(.report.aggregate.throughput_ratio) | median)
      },
      fixtures: (
        (.[0].report.fixtures | map(.fixture)) as $fixture_names
        | [
            $fixture_names[] as $name
            | {
                fixture: $name,
                median_latency_ratio: ([.[].report.fixtures[] | select(.fixture == $name) | .median_latency_ratio] | median),
                p99_latency_ratio: ([.[].report.fixtures[] | select(.fixture == $name) | .p99_latency_ratio] | median),
                throughput_ratio: ([.[].report.fixtures[] | select(.fixture == $name) | .throughput_ratio] | median),
                max_fallbacks: ([.[].report.fixtures[] | select(.fixture == $name) | (.vm_profile.register_program_fallbacks // 0)] | max),
                min_register_executed: ([.[].report.fixtures[] | select(.fixture == $name) | (.vm_profile.register_programs_executed // 0)] | min)
              }
          ]
      )
    },
    mean_metrics: {
      aggregate: {
        median_latency_ratio: (
          map(
            [.report.fixtures[].median_latency_ratio]
            | sort
            | .[(length / 2 | floor)]
          )
          | mean
        ),
        p99_latency_ratio: (map(.report.aggregate.p99_latency_ratio) | mean),
        throughput_ratio: (map(.report.aggregate.throughput_ratio) | mean)
      }
    },
    outlier_notes: {
      aggregate: {
        median_latency_ratio: (
          outlier_runs(
            map(
              [.report.fixtures[].median_latency_ratio]
              | sort
              | .[(length / 2 | floor)]
            )
          )
        ),
        p99_latency_ratio: (outlier_runs(map(.report.aggregate.p99_latency_ratio))),
        throughput_ratio: (outlier_runs(map(.report.aggregate.throughput_ratio)))
      },
      fixtures: (
        (.[0].report.fixtures | map(.fixture)) as $fixture_names
        | [
            $fixture_names[] as $name
            | {
                fixture: $name,
                median_latency_ratio: (outlier_runs([.[].report.fixtures[] | select(.fixture == $name) | .median_latency_ratio])),
                p99_latency_ratio: (outlier_runs([.[].report.fixtures[] | select(.fixture == $name) | .p99_latency_ratio])),
                throughput_ratio: (outlier_runs([.[].report.fixtures[] | select(.fixture == $name) | .throughput_ratio]))
              }
          ]
      )
    }
  }
' --arg run1 "${RUN1_JSON}" --arg run2 "${RUN2_JSON}" --arg run3 "${RUN3_JSON}" "${RUN1_JSON}" "${RUN2_JSON}" "${RUN3_JSON}" > "${OUT_JSON}"

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

DECISION_MEDIAN_RATIO="$(jq -r '.decision_metrics.aggregate.median_latency_ratio' "${OUT_JSON}")"
DECISION_P99_RATIO="$(jq -r '.decision_metrics.aggregate.p99_latency_ratio' "${OUT_JSON}")"
DECISION_THROUGHPUT_RATIO="$(jq -r '.decision_metrics.aggregate.throughput_ratio' "${OUT_JSON}")"

RESULT="recorded"
if [[ "${ENFORCE_THRESHOLDS}" == "1" ]]; then
  if ! float_le "${DECISION_MEDIAN_RATIO}" "${MEDIAN_RATIO_MAX}"; then
    echo "[vm-bench-compare] FAIL: decision median ratio ${DECISION_MEDIAN_RATIO} exceeds ${MEDIAN_RATIO_MAX}"
    exit 1
  fi
  if ! float_le "${DECISION_P99_RATIO}" "${P99_RATIO_MAX}"; then
    echo "[vm-bench-compare] FAIL: decision p99 ratio ${DECISION_P99_RATIO} exceeds ${P99_RATIO_MAX}"
    exit 1
  fi
  if ! float_ge "${DECISION_THROUGHPUT_RATIO}" "${THROUGHPUT_RATIO_MIN}"; then
    echo "[vm-bench-compare] FAIL: decision throughput ratio ${DECISION_THROUGHPUT_RATIO} below ${THROUGHPUT_RATIO_MIN}"
    exit 1
  fi
  RESULT="pass"
fi

jq \
  --argjson median_ratio_max "${MEDIAN_RATIO_MAX}" \
  --argjson p99_ratio_max "${P99_RATIO_MAX}" \
  --argjson throughput_ratio_min "${THROUGHPUT_RATIO_MIN}" \
  --arg enforced "${ENFORCE_THRESHOLDS}" \
  --arg result "${RESULT}" \
  '. + {
    thresholds: {
      median_latency_max: $median_ratio_max,
      p99_latency_max: $p99_ratio_max,
      throughput_min: $throughput_ratio_min
    },
    thresholds_enforced: ($enforced == "1"),
    result: $result
  }' "${OUT_JSON}" > "${OUT_JSON}.tmp"
mv "${OUT_JSON}.tmp" "${OUT_JSON}"

{
  echo "# MP-060 Runtime VM 3-Run Comparison"
  echo
  jq -r '"- corpus: \(.corpus)\n- runs: \(.runs)\n- decision aggregate median ratio (median of per-fixture medians): \(.decision_metrics.aggregate.median_latency_ratio)\n- decision aggregate p99 ratio: \(.decision_metrics.aggregate.p99_latency_ratio)\n- decision aggregate throughput ratio: \(.decision_metrics.aggregate.throughput_ratio)\n- mean aggregate median ratio (informational): \(.mean_metrics.aggregate.median_latency_ratio)\n- mean aggregate p99 ratio (informational): \(.mean_metrics.aggregate.p99_latency_ratio)\n- mean aggregate throughput ratio (informational): \(.mean_metrics.aggregate.throughput_ratio)\n- thresholds enforced: \(.thresholds_enforced)\n- result: \(.result)"' "${OUT_JSON}"
  echo
  echo "## Decision Per Fixture"
  jq -r '.decision_metrics.fixtures[] | "- \(.fixture): median=\(.median_latency_ratio), p99=\(.p99_latency_ratio), throughput=\(.throughput_ratio), min_register_executed=\(.min_register_executed), max_fallbacks=\(.max_fallbacks)"' "${OUT_JSON}"
  echo
  echo "## Outlier Notes (>2x or <0.5x run-median)"
  jq -r '
    [
      (if (.outlier_notes.aggregate.median_latency_ratio | length) > 0 then "- aggregate median_latency_ratio: \(.outlier_notes.aggregate.median_latency_ratio)" else empty end),
      (if (.outlier_notes.aggregate.p99_latency_ratio | length) > 0 then "- aggregate p99_latency_ratio: \(.outlier_notes.aggregate.p99_latency_ratio)" else empty end),
      (if (.outlier_notes.aggregate.throughput_ratio | length) > 0 then "- aggregate throughput_ratio: \(.outlier_notes.aggregate.throughput_ratio)" else empty end),
      (.outlier_notes.fixtures[]
        | select((.median_latency_ratio | length) > 0 or (.p99_latency_ratio | length) > 0 or (.throughput_ratio | length) > 0)
        | "- \(.fixture): median=\(.median_latency_ratio), p99=\(.p99_latency_ratio), throughput=\(.throughput_ratio)")
    ]
    | if length == 0 then "- none" else .[] end
  ' "${OUT_JSON}"
} > "${OUT_MD}"

echo "[vm-bench-compare] ${RESULT^^}"
