#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

OUT_DIR="${OUT_DIR:-target/gate-artifacts/runtime-cloud-security-profiles}"
mkdir -p "${OUT_DIR}"

run_case() {
  local case_id="$1"
  local test_filter="$2"
  local log_path="${OUT_DIR}/${case_id}.log"
  echo "[security-gate] running ${case_id}"
  cargo test -p trust-runtime --test web_io_config_integration "${test_filter}" -- --nocapture \
    | tee "${log_path}"
}

run_case "dev-profile" "runtime_cloud_state_endpoint_exposes_context_and_topology_contract"
run_case "plant-profile" "runtime_cloud_state_requires_secure_profile_transport_in_plant_mode"
run_case "wan-profile" "runtime_cloud_preflight_wan_requires_secure_profile_preconditions"

cat > "${OUT_DIR}/summary.md" <<'MD'
# Runtime Cloud Security Profile Gate

Executed profile evidence tests:

- dev:
  - `runtime_cloud_state_endpoint_exposes_context_and_topology_contract`
- plant:
  - `runtime_cloud_state_requires_secure_profile_transport_in_plant_mode`
- wan:
  - `runtime_cloud_preflight_wan_requires_secure_profile_preconditions`

Result: PASS
MD

echo "[security-gate] PASS"
