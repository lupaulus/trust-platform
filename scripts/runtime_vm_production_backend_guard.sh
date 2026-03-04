#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

fail() {
  echo "[vm-production-guard] FAIL: $1"
  exit 1
}

expect_match() {
  local pattern="$1"
  local file="$2"
  local description="$3"
  if ! rg -n --fixed-strings "${pattern}" "${file}" >/dev/null; then
    fail "${description} (missing: ${pattern} in ${file})"
  fi
}

expect_no_interpreter_usage() {
  local file="$1"
  local description="$2"
  if rg -n -e 'ExecutionBackend::Interpreter' -e '"interpreter"' "${file}" >/dev/null; then
    echo "[vm-production-guard] unexpected interpreter reference in ${file}:"
    rg -n -e 'ExecutionBackend::Interpreter' -e '"interpreter"' "${file}" || true
    fail "${description}"
  fi
}

expect_no_eval_expr_bridge() {
  local file="$1"
  local description="$2"
  if rg -n -e 'crate::eval::eval_expr' -e 'eval::eval_expr' -e '\beval_expr\s*\(' "${file}" >/dev/null; then
    echo "[vm-production-guard] unexpected eval_expr bridge in ${file}:"
    rg -n -e 'crate::eval::eval_expr' -e 'eval::eval_expr' -e '\beval_expr\s*\(' "${file}" || true
    fail "${description}"
  fi
}

if rg -n 'default\s*=\s*\[[^]]*legacy-interpreter' crates/trust-runtime/Cargo.toml >/dev/null; then
  fail "legacy-interpreter must not be part of default trust-runtime features"
fi

expect_match \
  "runtime.execution_backend='interpreter' is no longer supported for production runtimes; use 'vm'" \
  "crates/trust-runtime/src/config/parser/validation/runtime/entry.rs" \
  "runtime config parser must explicitly reject interpreter backend"

expect_no_interpreter_usage \
  "crates/trust-runtime/src/bin/trust-runtime/run/runtime/entry.rs" \
  "production run entry must not reference interpreter backend"

expect_no_interpreter_usage \
  "crates/trust-runtime/src/bundle_template.rs" \
  "runtime template defaults must remain VM-only"

expect_no_interpreter_usage \
  "crates/trust-runtime/src/settings.rs" \
  "runtime settings defaults must remain VM-only"

expect_no_interpreter_usage \
  "crates/trust-runtime/src/metrics.rs" \
  "runtime metrics defaults must remain VM-only"

expect_no_interpreter_usage \
  "crates/trust-runtime/src/runtime/core/lifecycle.rs" \
  "runtime lifecycle defaults must remain VM-only"

expect_no_interpreter_usage \
  "crates/trust-runtime/src/control/status_handlers.rs" \
  "control status fallback must remain VM-only"

expect_no_eval_expr_bridge \
  "crates/trust-runtime/src/runtime/vm/call.rs" \
  "VM CALL_NATIVE path must not bridge through eval_expr"

echo "[vm-production-guard] PASS"
