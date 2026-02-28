pub(crate) fn init_locals(
    ctx: &mut EvalContext<'_>,
    locals: &[VarDef],
) -> Result<(), RuntimeError> {
    for local in locals {
        if local.external {
            continue;
        }
        if let Some(fb_name) = function_block_type_name(local.type_id, ctx.registry) {
            let function_blocks = ctx.function_blocks.ok_or(RuntimeError::TypeMismatch)?;
            let functions = ctx.functions.ok_or(RuntimeError::TypeMismatch)?;
            let stdlib = ctx.stdlib.ok_or(RuntimeError::TypeMismatch)?;
            let classes = ctx.classes.ok_or(RuntimeError::TypeMismatch)?;
            let key = SmolStr::new(fb_name.to_ascii_uppercase());
            let fb = function_blocks
                .get(&key)
                .ok_or(RuntimeError::UndefinedFunctionBlock(fb_name))?;
            let instance_id = create_fb_instance(
                ctx.storage,
                ctx.registry,
                &ctx.profile,
                classes,
                function_blocks,
                functions,
                stdlib,
                fb,
            )?;
            ctx.storage
                .set_local(local.name.clone(), Value::Instance(instance_id));
            continue;
        }
        if let Some(class_name) = class_type_name(local.type_id, ctx.registry) {
            let function_blocks = ctx.function_blocks.ok_or(RuntimeError::TypeMismatch)?;
            let functions = ctx.functions.ok_or(RuntimeError::TypeMismatch)?;
            let stdlib = ctx.stdlib.ok_or(RuntimeError::TypeMismatch)?;
            let classes = ctx.classes.ok_or(RuntimeError::TypeMismatch)?;
            let key = SmolStr::new(class_name.to_ascii_uppercase());
            let class_def = classes.get(&key).ok_or(RuntimeError::TypeMismatch)?;
            let instance_id = create_class_instance(
                ctx.storage,
                ctx.registry,
                &ctx.profile,
                classes,
                function_blocks,
                functions,
                stdlib,
                class_def,
            )?;
            ctx.storage
                .set_local(local.name.clone(), Value::Instance(instance_id));
            continue;
        }
        let value = if let Some(expr) = &local.initializer {
            eval_expr(ctx, expr)?
        } else {
            default_value_for_type_id(local.type_id, ctx.registry, &ctx.profile)
                .unwrap_or(Value::Null)
        };
        ctx.storage.set_local(local.name.clone(), value);
    }
    Ok(())
}

pub(crate) fn init_locals_in_frame(
    ctx: &mut EvalContext<'_>,
    locals: &[VarDef],
) -> Result<(), RuntimeError> {
    for local in locals {
        if local.external {
            continue;
        }
        if let Some(fb_name) = function_block_type_name(local.type_id, ctx.registry) {
            let function_blocks = ctx.function_blocks.ok_or(RuntimeError::TypeMismatch)?;
            let functions = ctx.functions.ok_or(RuntimeError::TypeMismatch)?;
            let stdlib = ctx.stdlib.ok_or(RuntimeError::TypeMismatch)?;
            let classes = ctx.classes.ok_or(RuntimeError::TypeMismatch)?;
            let key = SmolStr::new(fb_name.to_ascii_uppercase());
            let fb = function_blocks
                .get(&key)
                .ok_or(RuntimeError::UndefinedFunctionBlock(fb_name))?;
            let instance_id = create_fb_instance(
                ctx.storage,
                ctx.registry,
                &ctx.profile,
                classes,
                function_blocks,
                functions,
                stdlib,
                fb,
            )?;
            ctx.storage
                .set_local(local.name.clone(), Value::Instance(instance_id));
            continue;
        }
        if let Some(class_name) = class_type_name(local.type_id, ctx.registry) {
            let function_blocks = ctx.function_blocks.ok_or(RuntimeError::TypeMismatch)?;
            let functions = ctx.functions.ok_or(RuntimeError::TypeMismatch)?;
            let stdlib = ctx.stdlib.ok_or(RuntimeError::TypeMismatch)?;
            let classes = ctx.classes.ok_or(RuntimeError::TypeMismatch)?;
            let key = SmolStr::new(class_name.to_ascii_uppercase());
            let class_def = classes.get(&key).ok_or(RuntimeError::TypeMismatch)?;
            let instance_id = create_class_instance(
                ctx.storage,
                ctx.registry,
                &ctx.profile,
                classes,
                function_blocks,
                functions,
                stdlib,
                class_def,
            )?;
            ctx.storage
                .set_local(local.name.clone(), Value::Instance(instance_id));
            continue;
        }
        let value = if let Some(expr) = &local.initializer {
            eval_expr(ctx, expr)?
        } else {
            default_value_for_type_id(local.type_id, ctx.registry, &ctx.profile)
                .unwrap_or(Value::Null)
        };
        ctx.storage.set_local(local.name.clone(), value);
    }
    Ok(())
}

fn function_block_type_name(type_id: TypeId, registry: &TypeRegistry) -> Option<SmolStr> {
    let ty = registry.get(type_id)?;
    match ty {
        trust_hir::Type::FunctionBlock { name } => Some(name.clone()),
        trust_hir::Type::Alias { target, .. } => function_block_type_name(*target, registry),
        _ => None,
    }
}

fn class_type_name(type_id: TypeId, registry: &TypeRegistry) -> Option<SmolStr> {
    let ty = registry.get(type_id)?;
    match ty {
        trust_hir::Type::Class { name } => Some(name.clone()),
        trust_hir::Type::Alias { target, .. } => class_type_name(*target, registry),
        _ => None,
    }
}

