use smol_str::SmolStr;

use crate::bytecode::{
    NATIVE_CALL_KIND_FUNCTION, NATIVE_CALL_KIND_FUNCTION_BLOCK, NATIVE_CALL_KIND_METHOD,
    NATIVE_CALL_KIND_STDLIB,
};
use crate::eval::expr::{Expr, LValue};
use crate::eval::{ArgValue, CallArg};
use crate::memory::{FrameId, InstanceId, MemoryLocation};
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

    let call_target = match kind {
        NATIVE_CALL_KIND_FUNCTION | NATIVE_CALL_KIND_STDLIB => {
            if target_name.is_empty() {
                return Err(VmTrap::InvalidNativeCall(
                    "missing native function target".into(),
                ));
            }
            Expr::Name(target_name)
        }
        NATIVE_CALL_KIND_FUNCTION_BLOCK => {
            let receiver = receiver_value.ok_or_else(|| {
                VmTrap::InvalidNativeCall("missing function-block receiver payload".into())
            })?;
            Expr::Literal(receiver)
        }
        NATIVE_CALL_KIND_METHOD => {
            if target_name.is_empty() {
                return Err(VmTrap::InvalidNativeCall("missing method name".into()));
            }
            let receiver = receiver_value.ok_or_else(|| {
                VmTrap::InvalidNativeCall("missing method receiver payload".into())
            })?;
            Expr::Field {
                target: Box::new(Expr::Literal(receiver)),
                field: target_name,
            }
        }
        _ => return Err(VmTrap::InvalidNativeCallKind(kind)),
    };
    let call_expr = Expr::Call {
        target: Box::new(call_target),
        args: call_args,
    };

    let call_result = runtime.with_eval_context(temp_local_frame, None, |ctx| {
        ctx.current_instance = frame.runtime_instance;
        crate::eval::eval_expr(ctx, &call_expr)
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
