use smol_str::SmolStr;

use crate::bytecode::{
    NATIVE_CALL_KIND_FUNCTION, NATIVE_CALL_KIND_FUNCTION_BLOCK, NATIVE_CALL_KIND_METHOD,
    NATIVE_CALL_KIND_STDLIB,
};
use crate::error::RuntimeError;
use crate::eval::expr::{self, Expr, LValue};
use crate::eval::{ArgValue, CallArg, EvalContext};
use crate::memory::{FrameId, InstanceId, MemoryLocation};
use crate::stdlib::{conversions, time, StdParams};
use crate::value::{Value, ValueRef};

use super::errors::VmTrap;
use super::frames::{FrameStack, VmFrame};
use super::stack::OperandStack;
use super::VmModule;

pub(super) const VM_LOCAL_SENTINEL_FRAME_ID: u32 = u32::MAX;

pub(super) fn push_call_frame(
    frame_stack: &mut FrameStack,
    module: &VmModule,
    pou_id: u32,
    return_pc: usize,
    runtime_instance: Option<InstanceId>,
) -> Result<usize, VmTrap> {
    let pou = module.pou(pou_id).ok_or(VmTrap::MissingPou(pou_id))?;
    let local_count = pou.local_ref_count as usize;
    let frame = VmFrame {
        pou_id,
        return_pc,
        code_start: pou.code_start,
        code_end: pou.code_end,
        local_ref_start: pou.local_ref_start,
        local_ref_count: pou.local_ref_count,
        locals: vec![Value::Null; local_count],
        runtime_instance,
        instance_owner: pou.primary_instance_owner,
    };
    let entry_pc = frame.code_start;
    frame_stack.push(frame)?;
    Ok(entry_pc)
}

#[derive(Debug, Clone)]
struct NativeArgSpec {
    name: Option<SmolStr>,
    is_target: bool,
}

pub(super) fn execute_native_call(
    runtime: &mut super::super::core::Runtime,
    module: &VmModule,
    frame: &mut VmFrame,
    operand_stack: &mut OperandStack,
    kind: u32,
    symbol_idx: u32,
    arg_count: u32,
) -> Result<Value, VmTrap> {
    let symbol = module
        .strings
        .get(symbol_idx as usize)
        .cloned()
        .ok_or(VmTrap::InvalidNativeSymbolIndex(symbol_idx))?;
    let (target_name, arg_specs) = parse_native_symbol(&symbol)?;
    let receiver_count = native_receiver_count(kind)?;
    let total = usize::try_from(arg_count)
        .map_err(|_| VmTrap::InvalidNativeCall("arg_count overflow".into()))?;
    if total < receiver_count {
        return Err(VmTrap::InvalidNativeCall(
            "arg_count smaller than native receiver arity".into(),
        ));
    }
    if arg_specs.len() + receiver_count != total {
        return Err(VmTrap::InvalidNativeCall(
            format!(
                "symbol arg metadata mismatch: expected {} payload(s), got {total}",
                arg_specs.len() + receiver_count
            )
            .into(),
        ));
    }

    let mut payload = Vec::with_capacity(total);
    for _ in 0..total {
        payload.push(operand_stack.pop()?);
    }
    payload.reverse();

    let receiver_value = if receiver_count == 1 {
        Some(payload.remove(0))
    } else {
        None
    };

    let mut temp_local_frame = None;
    let mut call_args = Vec::with_capacity(arg_specs.len());
    for (spec, value) in arg_specs.iter().zip(payload) {
        if spec.is_target {
            let Value::Reference(Some(value_ref)) = value else {
                return Err(VmTrap::InvalidNativeCall(
                    format!(
                        "target argument '{}' requires reference payload",
                        spec.name.as_deref().unwrap_or("<positional>")
                    )
                    .into(),
                ));
            };
            let value_ref =
                remap_local_reference(runtime, frame, value_ref, &mut temp_local_frame)?;
            call_args.push(CallArg {
                name: spec.name.clone(),
                value: ArgValue::Target(LValue::Deref(Box::new(Expr::Literal(Value::Reference(
                    Some(value_ref),
                ))))),
            });
        } else {
            call_args.push(CallArg {
                name: spec.name.clone(),
                value: ArgValue::Expr(Expr::Literal(value)),
            });
        }
    }

    match kind {
        NATIVE_CALL_KIND_FUNCTION | NATIVE_CALL_KIND_STDLIB => {
            if target_name.is_empty() {
                return Err(VmTrap::InvalidNativeCall(
                    "missing native function target".into(),
                ));
            }
        }
        NATIVE_CALL_KIND_FUNCTION_BLOCK => {
            receiver_value.as_ref().ok_or_else(|| {
                VmTrap::InvalidNativeCall("missing function-block receiver payload".into())
            })?;
        }
        NATIVE_CALL_KIND_METHOD => {
            if target_name.is_empty() {
                return Err(VmTrap::InvalidNativeCall("missing method name".into()));
            }
            receiver_value.as_ref().ok_or_else(|| {
                VmTrap::InvalidNativeCall("missing method receiver payload".into())
            })?;
        }
        _ => return Err(VmTrap::InvalidNativeCallKind(kind)),
    }

    let current_instance = frame.runtime_instance;
    let call_result = runtime.with_eval_context(temp_local_frame, None, move |ctx| {
        ctx.current_instance = current_instance;
        dispatch_native_call(ctx, kind, &target_name, receiver_value, &call_args)
    });

    if let Some(local_frame_id) = temp_local_frame {
        sync_vm_locals_from_temp(runtime, frame, local_frame_id)?;
        runtime
            .storage
            .remove_frame(local_frame_id)
            .ok_or_else(|| VmTrap::InvalidNativeCall("missing temporary local frame".into()))?;
    }

    call_result.map_err(VmTrap::from)
}

fn dispatch_native_call(
    ctx: &mut EvalContext<'_>,
    kind: u32,
    target_name: &SmolStr,
    receiver_value: Option<Value>,
    call_args: &[CallArg],
) -> Result<Value, RuntimeError> {
    match kind {
        NATIVE_CALL_KIND_FUNCTION => dispatch_native_function(ctx, target_name, call_args),
        NATIVE_CALL_KIND_STDLIB => dispatch_native_stdlib(ctx, target_name, call_args),
        NATIVE_CALL_KIND_FUNCTION_BLOCK => {
            dispatch_native_function_block(ctx, target_name, receiver_value, call_args)
        }
        NATIVE_CALL_KIND_METHOD => {
            dispatch_native_method(ctx, target_name, receiver_value, call_args)
        }
        _ => Err(RuntimeError::InvalidBytecode(
            format!("vm invalid CALL_NATIVE kind {kind}").into(),
        )),
    }
}

fn dispatch_native_function(
    ctx: &mut EvalContext<'_>,
    target_name: &SmolStr,
    call_args: &[CallArg],
) -> Result<Value, RuntimeError> {
    let key = SmolStr::new(target_name.to_ascii_uppercase());
    if let Some(functions) = ctx.functions {
        if let Some(function) = functions.get(&key) {
            return crate::eval::call_function(ctx, function, call_args);
        }
        if !target_name.contains('.') {
            if let Some(using) = ctx.using {
                if let Some(function) =
                    expr::resolve_using_function(functions, target_name.as_str(), using)
                {
                    return crate::eval::call_function(ctx, function, call_args);
                }
            }
        }
    }
    Err(RuntimeError::UndefinedFunction(target_name.clone()))
}

fn dispatch_native_stdlib(
    ctx: &mut EvalContext<'_>,
    target_name: &SmolStr,
    call_args: &[CallArg],
) -> Result<Value, RuntimeError> {
    let key = SmolStr::new(target_name.to_ascii_uppercase());
    if time::is_split_name(key.as_str()) {
        return expr::eval_split_call(ctx, key.as_str(), call_args);
    }
    let has_named_args = call_args.iter().any(|arg| arg.name.is_some());
    let stdlib = ctx.stdlib.ok_or(RuntimeError::TypeMismatch)?;
    if let Some(entry) = stdlib.get(key.as_str()) {
        let values = if has_named_args {
            expr::bind_stdlib_named_args(ctx, &entry.params, call_args)?
        } else {
            expr::eval_positional_args(ctx, call_args)?
        };
        return (entry.func)(&values);
    }
    if conversions::is_conversion_name(key.as_str()) {
        let params = StdParams::Fixed(vec![SmolStr::new("IN")]);
        let values = if has_named_args {
            expr::bind_stdlib_named_args(ctx, &params, call_args)?
        } else {
            expr::eval_positional_args(ctx, call_args)?
        };
        return stdlib.call(key.as_str(), &values);
    }
    Err(RuntimeError::UndefinedFunction(target_name.clone()))
}

fn dispatch_native_function_block(
    ctx: &mut EvalContext<'_>,
    _target_name: &SmolStr,
    receiver_value: Option<Value>,
    call_args: &[CallArg],
) -> Result<Value, RuntimeError> {
    let Some(Value::Instance(instance_id)) = receiver_value else {
        return Err(RuntimeError::TypeMismatch);
    };
    let function_blocks = ctx.function_blocks.ok_or(RuntimeError::TypeMismatch)?;
    let instance = ctx
        .storage
        .get_instance(instance_id)
        .ok_or(RuntimeError::NullReference)?;
    let key = SmolStr::new(instance.type_name.to_ascii_uppercase());
    let function_block = function_blocks
        .get(&key)
        .ok_or_else(|| RuntimeError::UndefinedFunctionBlock(instance.type_name.clone()))?;
    crate::eval::call_function_block(ctx, function_block, instance_id, call_args)?;
    Ok(Value::Null)
}

fn dispatch_native_method(
    ctx: &mut EvalContext<'_>,
    target_name: &SmolStr,
    receiver_value: Option<Value>,
    call_args: &[CallArg],
) -> Result<Value, RuntimeError> {
    let Some(Value::Instance(instance_id)) = receiver_value else {
        return Err(RuntimeError::TypeMismatch);
    };
    let method = expr::resolve_instance_method(ctx, instance_id, target_name)
        .ok_or_else(|| RuntimeError::UndefinedField(target_name.clone()))?;
    crate::eval::call_method(ctx, &method, instance_id, call_args)
}

fn native_receiver_count(kind: u32) -> Result<usize, VmTrap> {
    match kind {
        NATIVE_CALL_KIND_FUNCTION | NATIVE_CALL_KIND_STDLIB => Ok(0),
        NATIVE_CALL_KIND_FUNCTION_BLOCK | NATIVE_CALL_KIND_METHOD => Ok(1),
        _ => Err(VmTrap::InvalidNativeCallKind(kind)),
    }
}

fn parse_native_symbol(symbol: &SmolStr) -> Result<(SmolStr, Vec<NativeArgSpec>), VmTrap> {
    let mut parts = symbol.split('|');
    let target = SmolStr::new(parts.next().unwrap_or_default());
    let mut args = Vec::new();
    for raw in parts {
        if raw.is_empty() {
            return Err(VmTrap::InvalidNativeCall(
                "empty CALL_NATIVE arg token".into(),
            ));
        }
        let (is_target, suffix) = if let Some(rest) = raw.strip_prefix('E') {
            (false, rest)
        } else if let Some(rest) = raw.strip_prefix('T') {
            (true, rest)
        } else {
            return Err(VmTrap::InvalidNativeCall(
                "CALL_NATIVE arg token must start with E/T".into(),
            ));
        };
        let name = if suffix.is_empty() {
            None
        } else if let Some(named) = suffix.strip_prefix(':') {
            if named.is_empty() {
                return Err(VmTrap::InvalidNativeCall(
                    "CALL_NATIVE named token missing argument name".into(),
                ));
            }
            Some(SmolStr::new(named))
        } else {
            return Err(VmTrap::InvalidNativeCall(
                "CALL_NATIVE arg token suffix must be ':NAME'".into(),
            ));
        };
        args.push(NativeArgSpec { name, is_target });
    }
    Ok((target, args))
}

fn remap_local_reference(
    runtime: &mut super::super::core::Runtime,
    frame: &VmFrame,
    value_ref: ValueRef,
    temp_local_frame: &mut Option<FrameId>,
) -> Result<ValueRef, VmTrap> {
    if !matches!(
        value_ref.location,
        MemoryLocation::Local(FrameId(VM_LOCAL_SENTINEL_FRAME_ID))
    ) {
        return Ok(value_ref);
    }

    if value_ref.offset >= frame.locals.len() {
        return Err(VmTrap::InvalidNativeCall(
            format!(
                "local reference offset {} out of range for VM frame (locals={})",
                value_ref.offset,
                frame.locals.len()
            )
            .into(),
        ));
    }

    let frame_id = if let Some(existing) = *temp_local_frame {
        existing
    } else {
        let owner = SmolStr::new(format!("__vm_native_local_{}", frame.pou_id));
        let created = if let Some(instance_id) = frame.runtime_instance {
            runtime.storage.push_frame_with_instance(owner, instance_id)
        } else {
            runtime.storage.push_frame(owner)
        };
        for (slot, value) in frame.locals.iter().enumerate() {
            runtime
                .storage
                .set_local(SmolStr::new(format!("__vm_local_{slot}")), value.clone());
        }
        *temp_local_frame = Some(created);
        created
    };

    Ok(ValueRef {
        location: MemoryLocation::Local(frame_id),
        offset: value_ref.offset,
        path: value_ref.path,
    })
}

fn sync_vm_locals_from_temp(
    runtime: &super::super::core::Runtime,
    frame: &mut VmFrame,
    local_frame_id: FrameId,
) -> Result<(), VmTrap> {
    for slot in 0..frame.locals.len() {
        let value = runtime
            .storage
            .read_by_ref_parts(MemoryLocation::Local(local_frame_id), slot, &[])
            .cloned()
            .ok_or_else(|| {
                VmTrap::InvalidNativeCall(
                    format!("temporary local frame missing slot {slot}").into(),
                )
            })?;
        frame.locals[slot] = value;
    }
    Ok(())
}
