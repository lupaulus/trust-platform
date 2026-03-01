use std::sync::{Arc, Mutex};
use trust_runtime::execution_backend::ExecutionBackend;
use trust_runtime::harness::CompileSession;
use trust_runtime::metrics::RuntimeMetrics;
use trust_runtime::value::Value;
use trust_runtime::Runtime;

#[test]
fn loads_runtime() {
    let runtime = Runtime::new();
    let _profile = runtime.profile();
}

#[test]
fn runtime_execution_backend_defaults_and_validation() {
    let mut runtime = Runtime::new();
    assert_eq!(runtime.execution_backend(), ExecutionBackend::Interpreter);

    let err = runtime
        .set_execution_backend(ExecutionBackend::BytecodeVm)
        .expect_err("vm backend should require loaded bytecode module");
    assert!(err.to_string().contains("runtime.execution_backend='vm'"));
}

#[test]
fn runtime_metrics_snapshot_tracks_vm_backend_selection() {
    let source = r#"
        PROGRAM Main
        VAR
            count : DINT := DINT#0;
        END_VAR
        count := count + DINT#1;
        END_PROGRAM
    "#;
    let session = CompileSession::from_source(source);
    let mut runtime = session.build_runtime().expect("build runtime");
    let bytes = session.build_bytecode_bytes().expect("build bytecode");
    runtime
        .apply_bytecode_bytes(&bytes, None)
        .expect("apply bytecode");

    let metrics = Arc::new(Mutex::new(RuntimeMetrics::default()));
    runtime.set_metrics_sink(metrics.clone());
    runtime
        .set_execution_backend(ExecutionBackend::BytecodeVm)
        .expect("select vm backend");
    runtime.execute_cycle().expect("execute cycle");

    let snapshot = metrics.lock().expect("metrics lock").snapshot();
    assert_eq!(snapshot.execution_backend, ExecutionBackend::BytecodeVm);
}

#[test]
fn runtime_rolls_back_to_interpreter_with_loaded_bytecode() {
    let source = r#"
        PROGRAM Main
        VAR
            count : DINT := DINT#0;
        END_VAR
        count := count + DINT#1;
        END_PROGRAM
    "#;

    let session = CompileSession::from_source(source);
    let mut runtime = session.build_runtime().expect("build runtime");
    let bytes = session.build_bytecode_bytes().expect("build bytecode");
    runtime
        .apply_bytecode_bytes(&bytes, None)
        .expect("apply bytecode");

    runtime
        .set_execution_backend(ExecutionBackend::Interpreter)
        .expect("select interpreter backend");
    runtime.execute_cycle().expect("execute cycle");

    assert_eq!(runtime.execution_backend(), ExecutionBackend::Interpreter);
    let main_id = match runtime.storage().get_global("Main") {
        Some(Value::Instance(id)) => *id,
        other => panic!("expected Main instance global, got {other:?}"),
    };
    assert_eq!(
        runtime.storage().get_instance_var(main_id, "count"),
        Some(&Value::DInt(1))
    );
}
