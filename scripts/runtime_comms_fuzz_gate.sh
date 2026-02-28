#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

OUT_DIR="${OUT_DIR:-target/gate-artifacts/runtime-comms-fuzz}"
ITERS="${TRUST_COMMS_FUZZ_ITERS:-512}"
mkdir -p "${OUT_DIR}"

echo "[fuzz-gate] TRUST_COMMS_FUZZ_ITERS=${ITERS}"

echo "[fuzz-gate] mesh payload encode/decode fuzz smoke"
TRUST_COMMS_FUZZ_ITERS="${ITERS}" \
  cargo test -p trust-runtime --lib mesh::tests::mesh_payload_encode_decode_fuzz_smoke_budget -- --nocapture \
  | tee "${OUT_DIR}/mesh_payload_fuzz.log"

echo "[fuzz-gate] shm header fuzz smoke"
TRUST_COMMS_FUZZ_ITERS="${ITERS}" \
  cargo test -p trust-runtime --lib realtime::tests::t0_shm_header_fuzz_rejects_corruption_budget -- --nocapture \
  | tee "${OUT_DIR}/shm_header_fuzz.log"

echo "[fuzz-gate] runtime-cloud api payload fuzz smoke"
TRUST_COMMS_FUZZ_ITERS="${ITERS}" \
  cargo test -p trust-runtime --lib runtime_cloud::routing::tests::runtime_cloud_api_payload_fuzz_smoke_budget -- --nocapture \
  | tee "${OUT_DIR}/runtime_cloud_api_fuzz.log"

echo "[fuzz-gate] runtime-cloud wan allowlist parser fuzz smoke"
TRUST_COMMS_FUZZ_ITERS="${ITERS}" \
  cargo test -p trust-runtime --lib web::runtime_cloud_policy::tests::wan_allowlist_parser_fuzz_smoke_budget -- --nocapture \
  | tee "${OUT_DIR}/runtime_cloud_acl_fuzz.log"

cat > "${OUT_DIR}/summary.md" <<MD
# Runtime Comms Fuzz Gate

- iterations per target: ${ITERS}
- targets:
  - mesh payload encode/decode fuzz smoke
  - SHM header fuzz smoke
  - runtime-cloud API payload fuzz smoke
  - runtime-cloud WAN allowlist parser fuzz smoke

Result: PASS
MD

echo "[fuzz-gate] PASS"
