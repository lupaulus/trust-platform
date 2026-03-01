# Runtime Execution Backend Migration and Compatibility Policy

This note defines the MP-060 rollout contract for selecting runtime execution backends and for keeping interpreter rollback support available during VM-default migration.

## Backend controls

- Startup configuration:
  - `runtime.execution_backend = "interpreter"|"vm"` in `runtime.toml`.
- CLI override:
  - `--execution-backend=interpreter|vm` on `trust-runtime run` and `trust-runtime play`.
- Runtime control plane:
  - Live `config.set` writes to `runtime.execution_backend` are rejected.
  - Backend changes require explicit startup-time operator action (CLI/config + restart).

## Compatibility mode policy (MP-060 rollout)

- Before VM default switch:
  - Interpreter remains the default backend.
  - VM is opt-in via CLI/config.
- At VM default switch milestone:
  - Interpreter remains an explicit opt-in compatibility mode.
  - Compatibility mode must remain supported for two release cycles after the switch.
- During that two-release window:
  - `--execution-backend=interpreter` must keep taking precedence over bundle config.
  - `runtime.execution_backend = "interpreter"` must remain accepted by config validation.
  - No automatic mid-cycle backend switching is permitted.

## Enforcement and evidence

- Runtime/tests enforcing compatibility controls:
  - `crates/trust-runtime/src/bin/trust-runtime/run/tests.rs`:
    - `execution_backend_selection_cli_interpreter_overrides_vm_bundle`
    - `execution_backend_selection_cli_overrides_bundle`
  - `crates/trust-runtime/tests/api_smoke.rs`:
    - `runtime_rolls_back_to_interpreter_with_loaded_bytecode`
  - `crates/trust-runtime/src/control/tests/core.rs`:
    - `config_set_rejects_runtime_backend_switch_during_live_control`
- CI release-evidence gates:
  - `.github/workflows/ci.yml` job `version-release-guard`.
  - `scripts/check_release_version_alignment.py` (workspace + VS Code version sync).
  - `scripts/check_version_release_evidence.py` (tag/workflow/release evidence on main/master version bumps).
  - `release-gate-report` artifact must include `gate-version-release-guard`.

## Operator migration expectations

1. Keep production runtime config explicit (`runtime.execution_backend`) during rollout.
2. Use CLI override for emergency rollback without rebuild:
   - `trust-runtime run --execution-backend=interpreter ...`
3. Treat backend changes as restart-boundary operations, never in-flight cycle edits.
4. Track release notes for the VM default-switch release and the two subsequent compatibility-window releases.
