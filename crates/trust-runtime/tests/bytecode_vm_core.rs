use std::time::{Duration as StdDuration, Instant};

use trust_runtime::bytecode::{BytecodeModule, PouKind, SectionData, SectionId};
use trust_runtime::error::RuntimeError;
use trust_runtime::execution_backend::ExecutionBackend;
use trust_runtime::harness::{bytecode_module_from_source, TestHarness};
use trust_runtime::Runtime;

fn vm_harness(source: &str) -> TestHarness {
    let mut harness = TestHarness::from_source(source).expect("compile harness");
    let bytes = trust_runtime::harness::bytecode_bytes_from_source(source).expect("build bytecode");
    harness
        .runtime_mut()
        .apply_bytecode_bytes(&bytes, None)
        .expect("apply bytecode");
    harness
        .runtime_mut()
        .set_execution_backend(ExecutionBackend::BytecodeVm)
        .expect("select vm backend");
    harness
        .runtime_mut()
        .restart(trust_runtime::RestartMode::Cold)
        .expect("restart runtime");
    harness
}

fn main_pou_entry(module: &BytecodeModule) -> (u32, usize, usize) {
    let strings = match module.section(SectionId::StringTable) {
        Some(SectionData::StringTable(strings)) => strings,
        _ => panic!("missing string table"),
    };
    let index = match module.section(SectionId::PouIndex) {
        Some(SectionData::PouIndex(index)) => index,
        _ => panic!("missing pou index"),
    };
    let main = index
        .entries
        .iter()
        .find(|entry| {
            entry.kind == PouKind::Program
                && strings.entries[entry.name_idx as usize].eq_ignore_ascii_case("MAIN")
        })
        .expect("main entry");
    (
        main.id,
        main.code_offset as usize,
        (main.code_offset + main.code_length) as usize,
    )
}

fn main_body_bytes(module: &BytecodeModule) -> Vec<u8> {
    let (_, start, end) = main_pou_entry(module);
    let code = match module.section(SectionId::PouBodies) {
        Some(SectionData::PouBodies(code)) => code,
        _ => panic!("missing POU_BODIES"),
    };
    code[start..end].to_vec()
}

fn replace_main_body(module: &mut BytecodeModule, new_body: &[u8]) {
    let (main_id, _, _) = main_pou_entry(module);
    let new_offset =
        if let Some(SectionData::PouBodies(code)) = module.section_mut(SectionId::PouBodies) {
            let offset = code.len() as u32;
            code.extend_from_slice(new_body);
            offset
        } else {
            panic!("missing POU_BODIES");
        };

    if let Some(SectionData::PouIndex(index)) = module.section_mut(SectionId::PouIndex) {
        for entry in &mut index.entries {
            if entry.id == main_id {
                entry.code_offset = new_offset;
                entry.code_length = new_body.len() as u32;
            }
        }
    } else {
        panic!("missing POU_INDEX");
    }

    // Debug map offsets may no longer align after manual body patching.
    module.sections.retain(|section| {
        section.id != SectionId::DebugMap.as_raw()
            && section.id != SectionId::DebugStringTable.as_raw()
    });
}

fn vm_harness_from_module(source: &str, module: &BytecodeModule) -> TestHarness {
    let bytes = module.encode().expect("encode module");
    let mut harness = TestHarness::from_source(source).expect("compile runtime");
    harness
        .runtime_mut()
        .apply_bytecode_bytes(&bytes, None)
        .expect("apply bytecode");
    harness
        .runtime_mut()
        .set_execution_backend(ExecutionBackend::BytecodeVm)
        .expect("select vm backend");
    harness
        .runtime_mut()
        .restart(trust_runtime::RestartMode::Cold)
        .expect("restart runtime");
    harness
}

fn assert_invalid_bytecode_contains(errors: &[RuntimeError], needle: &str) {
    assert!(
        errors.iter().any(
            |err| matches!(err, RuntimeError::InvalidBytecode(message) if message.contains(needle))
        ),
        "expected InvalidBytecode containing '{needle}', got {errors:?}"
    );
}

fn assert_apply_invalid_bytecode_contains(module: &BytecodeModule, needle: &str) {
    let bytes = module.encode().expect("encode module");
    let mut runtime = Runtime::new();
    let err = runtime
        .apply_bytecode_bytes(&bytes, None)
        .expect_err("mutated module should fail during apply");
    match err {
        RuntimeError::InvalidBytecode(message) => {
            assert!(
                message.contains(needle),
                "expected InvalidBytecode containing '{needle}', got '{message}'"
            );
        }
        other => panic!("expected InvalidBytecode, got {other:?}"),
    }
}

#[test]
fn vm_executes_program_with_stack_and_pc_progression() {
    let source = r#"
        PROGRAM Main
        VAR
            count: DINT := 0;
        END_VAR
        count := count + 1;
        END_PROGRAM
    "#;
    let mut harness = vm_harness(source);
    harness.assert_eq("count", 0i32);
    let cycle = harness.cycle();
    assert!(
        cycle.errors.is_empty(),
        "unexpected VM cycle errors: {:?}",
        cycle.errors
    );
    harness.assert_eq("count", 1i32);
}

#[test]
fn vm_opcode_positive_path_covers_arith_logical_branch_jump_load_store_ref() {
    let source = r#"
        PROGRAM Main
        VAR
            i: DINT := 0;
            acc: DINT := 0;
            gate: BOOL := FALSE;
        END_VAR
        WHILE i < 4 DO
            gate := (i < 2) AND TRUE;
            IF gate THEN
                acc := acc + i;
            END_IF;
            i := i + 1;
        END_WHILE;
        END_PROGRAM
    "#;
    let module = bytecode_module_from_source(source).expect("compile bytecode module");
    let body = main_body_bytes(&module);
    assert!(body.contains(&0x02), "expected JUMP opcode in main body");
    assert!(
        body.contains(&0x03) || body.contains(&0x04),
        "expected JUMP_IF_TRUE/FALSE opcode in main body"
    );
    assert!(
        body.contains(&0x20),
        "expected LOAD_REF opcode in main body"
    );
    assert!(
        body.contains(&0x21),
        "expected STORE_REF opcode in main body"
    );
    assert!(body.contains(&0x40), "expected ADD opcode in main body");
    assert!(body.contains(&0x46), "expected AND opcode in main body");

    let mut harness = vm_harness(source);
    let cycle = harness.cycle();
    assert!(
        cycle.errors.is_empty(),
        "opcode positive-path execution failed: {:?}",
        cycle.errors
    );
    harness.assert_eq("i", 4i32);
    harness.assert_eq("acc", 1i32);
}

#[test]
fn vm_opcode_positive_path_covers_call_native_stdlib_dispatch() {
    let source = r#"
        PROGRAM Main
        VAR
            out_sel : INT := 0;
        END_VAR
        out_sel := SEL(G := TRUE, IN0 := INT#4, IN1 := INT#7);
        END_PROGRAM
    "#;
    let module = bytecode_module_from_source(source).expect("compile bytecode module");
    let body = main_body_bytes(&module);
    assert!(
        body.contains(&0x09),
        "expected CALL_NATIVE opcode in main body"
    );

    let mut harness = vm_harness(source);
    let cycle = harness.cycle();
    assert!(
        cycle.errors.is_empty(),
        "CALL_NATIVE stdlib dispatch failed: {:?}",
        cycle.errors
    );
    harness.assert_eq("out_sel", 7i16);
}

#[test]
fn vm_enforces_execution_deadline() {
    let source = r#"
        PROGRAM Main
        WHILE TRUE DO
        END_WHILE;
        END_PROGRAM
    "#;
    let mut harness = vm_harness(source);
    harness
        .runtime_mut()
        .set_execution_deadline(Instant::now().checked_sub(StdDuration::from_millis(1)));
    let cycle = harness.cycle();
    assert!(
        cycle
            .errors
            .iter()
            .any(|err| matches!(err, RuntimeError::ExecutionTimeout)),
        "expected ExecutionTimeout, got {:?}",
        cycle.errors
    );
}

#[test]
fn vm_rejects_invalid_call_native_symbol_index() {
    let source = r#"
        PROGRAM Main
        VAR
            keep : INT := 0;
        END_VAR
        keep := keep + INT#1;
        END_PROGRAM
    "#;
    let mut module = bytecode_module_from_source(source).expect("compile module");
    let mut body = Vec::new();
    body.push(0x09);
    body.extend_from_slice(&0_u32.to_le_bytes());
    body.extend_from_slice(&255_u32.to_le_bytes());
    body.extend_from_slice(&0_u32.to_le_bytes());
    body.push(0x06);
    replace_main_body(&mut module, &body);

    assert_apply_invalid_bytecode_contains(&module, "invalid index 255 for native symbol");
}

#[test]
fn vm_enforces_instruction_budget() {
    let source = r#"
        PROGRAM Main
        END_PROGRAM
    "#;
    let mut module = bytecode_module_from_source(source).expect("compile module");
    let mut body = Vec::new();
    body.push(0x02);
    body.extend_from_slice(&(-5_i32).to_le_bytes());
    replace_main_body(&mut module, &body);

    let mut harness = vm_harness_from_module(source, &module);
    harness.runtime_mut().set_execution_deadline(None);
    let cycle = harness.cycle();
    assert!(
        cycle
            .errors
            .iter()
            .any(|err| matches!(err, RuntimeError::ExecutionTimeout)),
        "expected ExecutionTimeout from instruction budget, got {:?}",
        cycle.errors
    );
}

#[test]
fn vm_validator_rejects_invalid_ref_index_operand() {
    let source = r#"
        PROGRAM Main
        VAR
            count: DINT := 0;
        END_VAR
        count := count + 1;
        END_PROGRAM
    "#;
    let mut module = bytecode_module_from_source(source).expect("compile bytecode module");
    let (main_offset, main_length) = {
        let strings = match module.section(SectionId::StringTable) {
            Some(SectionData::StringTable(strings)) => strings,
            _ => panic!("missing string table"),
        };
        let index = match module.section(SectionId::PouIndex) {
            Some(SectionData::PouIndex(index)) => index,
            _ => panic!("missing pou index"),
        };
        let main = index
            .entries
            .iter()
            .find(|entry| {
                entry.kind == PouKind::Program
                    && strings.entries[entry.name_idx as usize].eq_ignore_ascii_case("MAIN")
            })
            .expect("main entry");
        (main.code_offset as usize, main.code_length as usize)
    };
    if main_length < 5 {
        panic!("main body too short for patch");
    }
    if let Some(SectionData::PouBodies(code)) = module.section_mut(SectionId::PouBodies) {
        code[main_offset] = 0x20;
        code[main_offset + 1..main_offset + 5].copy_from_slice(&255_u32.to_le_bytes());
    } else {
        panic!("missing POU_BODIES");
    }

    assert_apply_invalid_bytecode_contains(&module, "invalid index 255 for ref");
}

#[test]
fn vm_validator_rejects_invalid_const_index_operand() {
    let source = r#"
        PROGRAM Main
        END_PROGRAM
    "#;
    let mut module = bytecode_module_from_source(source).expect("compile module");
    let mut body = Vec::new();
    body.push(0x10);
    body.extend_from_slice(&255_u32.to_le_bytes());
    body.push(0x06);
    replace_main_body(&mut module, &body);

    assert_apply_invalid_bytecode_contains(&module, "invalid index 255 for const");
}

#[test]
fn vm_rejects_invalid_opcode() {
    let source = r#"
        PROGRAM Main
        VAR
            count: DINT := 0;
        END_VAR
        count := count + 1;
        END_PROGRAM
    "#;
    let mut module = bytecode_module_from_source(source).expect("compile bytecode module");
    let (_, main_offset, _) = main_pou_entry(&module);
    if let Some(SectionData::PouBodies(code)) = module.section_mut(SectionId::PouBodies) {
        code[main_offset] = 0xFF;
    } else {
        panic!("missing POU_BODIES");
    }

    assert_apply_invalid_bytecode_contains(&module, "invalid opcode 0xFF");
}

#[test]
fn vm_rejects_malformed_operands() {
    let source = r#"
        PROGRAM Main
        END_PROGRAM
    "#;
    let mut module = bytecode_module_from_source(source).expect("compile module");
    replace_main_body(&mut module, &[0x20]);

    assert_apply_invalid_bytecode_contains(&module, "unexpected end of input");
}

#[test]
fn vm_rejects_invalid_jump_target() {
    let source = r#"
        PROGRAM Main
        END_PROGRAM
    "#;
    let mut module = bytecode_module_from_source(source).expect("compile module");
    let mut body = Vec::new();
    body.push(0x02);
    body.extend_from_slice(&(4_096_i32).to_le_bytes());
    body.push(0x06);
    replace_main_body(&mut module, &body);

    assert_apply_invalid_bytecode_contains(&module, "invalid jump target");
}

#[test]
fn vm_traps_unsupported_call_method_opcode() {
    let source = r#"
        PROGRAM Main
        END_PROGRAM
    "#;
    let mut module = bytecode_module_from_source(source).expect("compile module");
    let mut body = Vec::new();
    body.push(0x07);
    body.extend_from_slice(&0_u32.to_le_bytes());
    body.push(0x06);
    replace_main_body(&mut module, &body);

    let mut harness = vm_harness_from_module(source, &module);
    let cycle = harness.cycle();
    assert_invalid_bytecode_contains(&cycle.errors, "vm unsupported opcode CALL_METHOD");
}

#[test]
fn vm_rejects_stack_underflow() {
    let source = r#"
        PROGRAM Main
        END_PROGRAM
    "#;
    let mut module = bytecode_module_from_source(source).expect("compile module");
    replace_main_body(&mut module, &[0x12, 0x06]);

    let mut harness = vm_harness_from_module(source, &module);
    let cycle = harness.cycle();
    assert_invalid_bytecode_contains(&cycle.errors, "vm operand stack underflow");
}

#[test]
fn vm_rejects_stack_overflow() {
    let source = r#"
        PROGRAM Main
        VAR
            keep: DINT := 1;
        END_VAR
        keep := keep + 0;
        END_PROGRAM
    "#;
    let mut module = bytecode_module_from_source(source).expect("compile module");
    let const_idx = match module.section(SectionId::ConstPool) {
        Some(SectionData::ConstPool(pool)) if !pool.entries.is_empty() => 0_u32,
        Some(_) => panic!("expected const pool entries for overflow fixture"),
        _ => panic!("missing CONST_POOL"),
    };

    let mut body = Vec::new();
    body.push(0x10);
    body.extend_from_slice(&const_idx.to_le_bytes());
    body.push(0x11);
    body.push(0x02);
    body.extend_from_slice(&(-6_i32).to_le_bytes());
    replace_main_body(&mut module, &body);

    let mut harness = vm_harness_from_module(source, &module);
    let cycle = harness.cycle();
    assert_invalid_bytecode_contains(&cycle.errors, "vm operand stack overflow");
}

#[test]
fn vm_call_stack_handles_call_and_return() {
    let source = r#"
        FUNCTION Foo : DINT
        Foo := 1;
        END_FUNCTION

        PROGRAM Main
        VAR
            count: DINT := 0;
        END_VAR
        count := count + 1;
        END_PROGRAM
    "#;
    let mut module = bytecode_module_from_source(source).expect("compile module");
    let (main_id, foo_id) = {
        let strings = match module.section(SectionId::StringTable) {
            Some(SectionData::StringTable(strings)) => strings,
            _ => panic!("missing string table"),
        };
        let index = match module.section(SectionId::PouIndex) {
            Some(SectionData::PouIndex(index)) => index,
            _ => panic!("missing pou index"),
        };
        let mut main_id = None;
        let mut foo_id = None;
        for entry in &index.entries {
            let name = &strings.entries[entry.name_idx as usize];
            if name.eq_ignore_ascii_case("MAIN") {
                main_id = Some(entry.id);
            }
            if name.eq_ignore_ascii_case("FOO") {
                foo_id = Some(entry.id);
            }
        }
        (main_id.expect("Main POU id"), foo_id.expect("Foo POU id"))
    };

    let main_body = {
        let mut bytes = Vec::new();
        bytes.push(0x05);
        bytes.extend_from_slice(&foo_id.to_le_bytes());
        bytes.push(0x00);
        bytes.push(0x06);
        bytes
    };
    let foo_body = vec![0x06];

    let (main_offset, foo_offset) =
        if let Some(SectionData::PouBodies(code)) = module.section_mut(SectionId::PouBodies) {
            let main_offset = code.len() as u32;
            code.extend_from_slice(&main_body);
            let foo_offset = code.len() as u32;
            code.extend_from_slice(&foo_body);
            (main_offset, foo_offset)
        } else {
            panic!("missing POU_BODIES");
        };
    if let Some(SectionData::PouIndex(index)) = module.section_mut(SectionId::PouIndex) {
        for entry in &mut index.entries {
            if entry.id == main_id {
                entry.code_offset = main_offset;
                entry.code_length = main_body.len() as u32;
            } else if entry.id == foo_id {
                entry.code_offset = foo_offset;
                entry.code_length = foo_body.len() as u32;
            }
        }
    }
    module.sections.retain(|section| {
        section.id != SectionId::DebugMap.as_raw()
            && section.id != SectionId::DebugStringTable.as_raw()
    });

    let bytes = module.encode().expect("encode module");
    let mut harness = TestHarness::from_source(source).expect("compile runtime");
    harness
        .runtime_mut()
        .apply_bytecode_bytes(&bytes, None)
        .expect("apply bytecode");
    harness
        .runtime_mut()
        .set_execution_backend(ExecutionBackend::BytecodeVm)
        .expect("select vm backend");
    harness
        .runtime_mut()
        .restart(trust_runtime::RestartMode::Cold)
        .expect("restart runtime");

    let cycle = harness.cycle();
    assert!(
        cycle.errors.is_empty(),
        "call/ret VM execution should succeed, got {:?}",
        cycle.errors
    );
}
