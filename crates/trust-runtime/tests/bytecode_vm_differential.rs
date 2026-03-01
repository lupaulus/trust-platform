use trust_runtime::execution_backend::ExecutionBackend;
use trust_runtime::harness::TestHarness;
use trust_runtime::value::Value;

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

fn assert_backend_parity(source: &str, vars: &[&str], cycles: usize) {
    let mut interpreter = TestHarness::from_source(source).expect("compile interpreter harness");
    let mut vm = vm_harness(source);

    for _ in 0..cycles {
        let interp_cycle = interpreter.cycle();
        let vm_cycle = vm.cycle();
        assert_eq!(
            interp_cycle.errors, vm_cycle.errors,
            "backend errors diverged: interp={:?} vm={:?}",
            interp_cycle.errors, vm_cycle.errors
        );
    }

    for name in vars {
        assert_eq!(
            interpreter.get_output(name),
            vm.get_output(name),
            "output diverged for variable '{name}'"
        );
    }
}

#[test]
fn differential_c1_function_named_default_and_inout_calls() {
    let source = r#"
        FUNCTION Add : INT
        VAR_INPUT
            a : INT;
            b : INT := INT#2;
        END_VAR
        Add := a + b;
        END_FUNCTION

        FUNCTION Bump : INT
        VAR_IN_OUT
            x : INT;
        END_VAR
        VAR_INPUT
            inc : INT := INT#1;
        END_VAR
        x := x + inc;
        Bump := x;
        END_FUNCTION

        PROGRAM Main
        VAR
            v : INT := INT#10;
            out_named : INT := INT#0;
            out_default : INT := INT#0;
            out_inout : INT := INT#0;
        END_VAR

        out_named := Add(b := INT#4, a := INT#3);
        out_default := Add(a := INT#3);
        out_inout := Bump(v, INT#5);
        END_PROGRAM
    "#;

    assert_backend_parity(source, &["v", "out_named", "out_default", "out_inout"], 1);
}

#[test]
fn differential_c1_function_block_and_builtin_dispatch() {
    let source = r#"
        FUNCTION_BLOCK Counter
        VAR_INPUT
            inc : BOOL;
        END_VAR
        VAR_OUTPUT
            value : INT;
        END_VAR
        VAR
            count : INT := INT#0;
        END_VAR
        IF inc THEN
            count := count + INT#1;
        END_IF;
        value := count;
        END_FUNCTION_BLOCK

        PROGRAM Main
        VAR
            fb : Counter;
            up : R_TRIG;
            clk : BOOL := FALSE;
            out_count : INT := INT#0;
            out_edge : BOOL := FALSE;
        END_VAR

        fb(inc := TRUE, value => out_count);
        up(CLK := clk, Q => out_edge);
        END_PROGRAM
    "#;

    let mut interpreter = TestHarness::from_source(source).expect("compile interpreter harness");
    let mut vm = vm_harness(source);

    for clk in [false, false, true, true] {
        interpreter.set_input("clk", Value::Bool(clk));
        vm.set_input("clk", Value::Bool(clk));
        let interp_cycle = interpreter.cycle();
        let vm_cycle = vm.cycle();
        assert_eq!(
            interp_cycle.errors, vm_cycle.errors,
            "backend errors diverged: interp={:?} vm={:?}",
            interp_cycle.errors, vm_cycle.errors
        );
    }

    for name in ["out_count", "out_edge"] {
        assert_eq!(
            interpreter.get_output(name),
            vm.get_output(name),
            "output diverged for variable '{name}'"
        );
    }
}

#[test]
fn differential_c1_method_call_dispatch() {
    let source = r#"
        CLASS Counter
        VAR PUBLIC
            value : INT := INT#0;
        END_VAR
        METHOD PUBLIC Inc : INT
        VAR_INPUT
            inc_step : INT := INT#1;
        END_VAR
        value := value + inc_step;
        Inc := value;
        END_METHOD
        END_CLASS

        PROGRAM Main
        VAR
            c : Counter;
            out_c1 : INT := INT#0;
            out_c2 : INT := INT#0;
        END_VAR
        out_c1 := c.Inc(INT#1);
        out_c2 := c.Inc(inc_step := INT#2);
        END_PROGRAM
    "#;

    assert_backend_parity(source, &["out_c1", "out_c2"], 1);
}

#[test]
fn differential_c1_stdlib_named_dispatch() {
    let source = r#"
        PROGRAM Main
        VAR
            out_sel : INT := INT#0;
            out_max : INT := INT#0;
        END_VAR
        out_sel := SEL(G := TRUE, IN0 := INT#4, IN1 := INT#7);
        out_max := MAX(IN1 := INT#3, IN2 := INT#9);
        END_PROGRAM
    "#;

    assert_backend_parity(source, &["out_sel", "out_max"], 1);
}

#[test]
fn differential_c2_string_and_wstring_literals_and_comparisons() {
    let source = r#"
        PROGRAM Main
        VAR
            s_value : STRING := '';
            ws_value : WSTRING := "";
            out_str_eq : BOOL := FALSE;
            out_str_lt : BOOL := FALSE;
            out_wstr_eq : BOOL := FALSE;
            out_wstr_lt : BOOL := FALSE;
        END_VAR

        s_value := 'WORLD';
        ws_value := "LUNA";
        out_str_eq := s_value = 'WORLD';
        out_str_lt := 'AA' < 'AB';
        out_wstr_eq := ws_value = "LUNA";
        out_wstr_lt := "AA" < "AB";
        END_PROGRAM
    "#;

    assert_backend_parity(
        source,
        &[
            "s_value",
            "ws_value",
            "out_str_eq",
            "out_str_lt",
            "out_wstr_eq",
            "out_wstr_lt",
        ],
        1,
    );
}

#[test]
fn differential_c2_string_stdlib_dispatch_with_literals() {
    let source = r#"
        PROGRAM Main
        VAR
            out_left : STRING := '';
            out_mid : STRING := '';
            out_find : INT := INT#0;
            out_w_replace : WSTRING := "";
            out_w_insert : WSTRING := "";
        END_VAR

        out_left := LEFT(IN := 'ABCDE', L := INT#3);
        out_mid := MID(IN := 'ABCDE', L := INT#2, P := INT#2);
        out_find := FIND(IN1 := 'BC', IN2 := 'ABCDE');
        out_w_replace := REPLACE(IN1 := "ABCDE", IN2 := "Z", L := INT#2, P := INT#3);
        out_w_insert := INSERT(IN1 := "ABE", IN2 := "CD", P := INT#3);
        END_PROGRAM
    "#;

    assert_backend_parity(
        source,
        &[
            "out_left",
            "out_mid",
            "out_find",
            "out_w_replace",
            "out_w_insert",
        ],
        1,
    );
}

#[test]
fn differential_c3_this_super_and_method_dispatch() {
    let source = r#"
        FUNCTION_BLOCK ThisCounter
        VAR
            count : INT := INT#5;
        END_VAR
        VAR_OUTPUT
            value : INT;
        END_VAR
        value := THIS.count;
        END_FUNCTION_BLOCK

        FUNCTION_BLOCK BaseFb
        VAR PUBLIC
            count : INT := INT#10;
        END_VAR
        METHOD PUBLIC GetCount : INT
        GetCount := count;
        END_METHOD
        END_FUNCTION_BLOCK

        FUNCTION_BLOCK DerivedFb EXTENDS BaseFb
        VAR PUBLIC
            extra : INT := INT#3;
        END_VAR
        METHOD PUBLIC GetCount : INT
        GetCount := count + extra;
        END_METHOD
        METHOD PUBLIC GetSuper : INT
        GetSuper := SUPER.GetCount();
        END_METHOD
        END_FUNCTION_BLOCK

        PROGRAM Main
        VAR
            fb_this : ThisCounter;
            fb_derived : DerivedFb;
            out_this : INT := INT#0;
            out_override : INT := INT#0;
            out_super : INT := INT#0;
        END_VAR
        fb_this(value => out_this);
        out_override := fb_derived.GetCount();
        out_super := fb_derived.GetSuper();
        END_PROGRAM
    "#;

    assert_backend_parity(source, &["out_this", "out_override", "out_super"], 1);
}

#[test]
fn differential_c3_interface_method_dispatch() {
    let source = r#"
        INTERFACE ICounter
        METHOD Next : INT
        END_METHOD
        END_INTERFACE

        CLASS Counter IMPLEMENTS ICounter
        VAR PUBLIC
            value : INT := INT#0;
        END_VAR
        METHOD PUBLIC Next : INT
        value := value + INT#1;
        Next := value;
        END_METHOD
        END_CLASS

        PROGRAM Main
        VAR
            c : Counter;
            i : ICounter;
            out_iface_1 : INT := INT#0;
            out_direct : INT := INT#0;
            out_iface_2 : INT := INT#0;
        END_VAR
        i := c;
        out_iface_1 := i.Next();
        out_direct := c.Next();
        out_iface_2 := i.Next();
        END_PROGRAM
    "#;

    assert_backend_parity(source, &["out_iface_1", "out_direct", "out_iface_2"], 1);
}
