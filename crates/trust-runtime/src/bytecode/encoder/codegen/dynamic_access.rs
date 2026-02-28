impl<'a> BytecodeEncoder<'a> {
    fn emit_assign(
        &mut self,
        ctx: &CodegenContext,
        target: &crate::eval::expr::LValue,
        value: &crate::eval::expr::Expr,
        code: &mut Vec<u8>,
    ) -> Result<bool, BytecodeError> {
        if let Some(emitted) = self.emit_dynamic_assign(ctx, target, value, code)? {
            return Ok(emitted);
        }
        let start_len = code.len();
        if !self.emit_expr(ctx, value, code)? {
            code.truncate(start_len);
            return Ok(false);
        }
        let reference = match self.resolve_lvalue_ref(ctx, target)? {
            Some(reference) => reference,
            None => {
                code.truncate(start_len);
                return Ok(false);
            }
        };
        let ref_idx = self.ref_index_for(&reference)?;
        code.push(0x21);
        code.extend_from_slice(&ref_idx.to_le_bytes());
        Ok(true)
    }

    fn emit_dynamic_assign(
        &mut self,
        ctx: &CodegenContext,
        target: &crate::eval::expr::LValue,
        value: &crate::eval::expr::Expr,
        code: &mut Vec<u8>,
    ) -> Result<Option<bool>, BytecodeError> {
        if !self.lvalue_is_self_field(ctx, target) {
            return Ok(None);
        }
        let start_len = code.len();
        if !self.emit_expr(ctx, value, code)? {
            code.truncate(start_len);
            return Ok(Some(false));
        }
        if !self.emit_dynamic_ref_for_lvalue(ctx, target, code)? {
            code.truncate(start_len);
            return Ok(Some(false));
        }
        code.push(0x13); // SWAP
        code.push(0x33); // STORE
        Ok(Some(true))
    }

    fn emit_dynamic_ref_for_lvalue(
        &mut self,
        ctx: &CodegenContext,
        target: &crate::eval::expr::LValue,
        code: &mut Vec<u8>,
    ) -> Result<bool, BytecodeError> {
        use crate::eval::expr::LValue;
        match target {
            LValue::Name(name) => self.emit_self_field_ref(ctx, name, code),
            LValue::Field { name, field } => {
                if !self.emit_self_field_ref(ctx, name, code)? {
                    return Ok(false);
                }
                let field_idx = self.strings.intern(field.clone());
                code.push(0x30);
                code.extend_from_slice(&field_idx.to_le_bytes());
                Ok(true)
            }
            LValue::Index { name, indices } => {
                if !self.emit_self_field_ref(ctx, name, code)? {
                    return Ok(false);
                }
                for index in indices {
                    if !self.emit_expr(ctx, index, code)? {
                        return Ok(false);
                    }
                    code.push(0x31);
                }
                Ok(true)
            }
            LValue::Deref(_) => Ok(false),
        }
    }

    fn lvalue_is_self_field(
        &self,
        ctx: &CodegenContext,
        target: &crate::eval::expr::LValue,
    ) -> bool {
        match target {
            crate::eval::expr::LValue::Name(name)
            | crate::eval::expr::LValue::Field { name, .. }
            | crate::eval::expr::LValue::Index { name, .. } => {
                ctx.self_field_name(name).is_some() && ctx.local_ref(name).is_none()
            }
            crate::eval::expr::LValue::Deref(_) => false,
        }
    }

    fn emit_self_field_ref(
        &mut self,
        ctx: &CodegenContext,
        name: &SmolStr,
        code: &mut Vec<u8>,
    ) -> Result<bool, BytecodeError> {
        let Some(field_name) = ctx.self_field_name(name) else {
            return Ok(false);
        };
        code.push(0x23);
        let name_idx = self.strings.intern(field_name.clone());
        code.push(0x30);
        code.extend_from_slice(&name_idx.to_le_bytes());
        Ok(true)
    }

    fn emit_load_access(
        &mut self,
        access: &AccessKind,
        code: &mut Vec<u8>,
    ) -> Result<(), BytecodeError> {
        match access {
            AccessKind::Static(reference) => self.emit_load_ref(reference, code),
            AccessKind::SelfField(field) => {
                code.push(0x23);
                let name_idx = self.strings.intern(field.clone());
                code.push(0x30);
                code.extend_from_slice(&name_idx.to_le_bytes());
                code.push(0x32);
                Ok(())
            }
        }
    }

    fn emit_store_access(
        &mut self,
        access: &AccessKind,
        code: &mut Vec<u8>,
    ) -> Result<(), BytecodeError> {
        match access {
            AccessKind::Static(reference) => self.emit_store_ref(reference, code),
            AccessKind::SelfField(field) => {
                code.push(0x23);
                let name_idx = self.strings.intern(field.clone());
                code.push(0x30);
                code.extend_from_slice(&name_idx.to_le_bytes());
                code.push(0x13);
                code.push(0x33);
                Ok(())
            }
        }
    }

    fn emit_dynamic_load_name(
        &mut self,
        ctx: &CodegenContext,
        name: &SmolStr,
        code: &mut Vec<u8>,
    ) -> Result<bool, BytecodeError> {
        if !self.emit_self_field_ref(ctx, name, code)? {
            return Ok(false);
        }
        code.push(0x32);
        Ok(true)
    }

    fn emit_dynamic_load_field(
        &mut self,
        ctx: &CodegenContext,
        base: &SmolStr,
        field: &SmolStr,
        code: &mut Vec<u8>,
    ) -> Result<bool, BytecodeError> {
        if !self.emit_self_field_ref(ctx, base, code)? {
            return Ok(false);
        }
        let field_idx = self.strings.intern(field.clone());
        code.push(0x30);
        code.extend_from_slice(&field_idx.to_le_bytes());
        code.push(0x32);
        Ok(true)
    }

    fn emit_dynamic_load_index(
        &mut self,
        ctx: &CodegenContext,
        base: &SmolStr,
        indices: &[crate::eval::expr::Expr],
        code: &mut Vec<u8>,
    ) -> Result<bool, BytecodeError> {
        if !self.emit_self_field_ref(ctx, base, code)? {
            return Ok(false);
        }
        for index in indices {
            if !self.emit_expr(ctx, index, code)? {
                return Ok(false);
            }
            code.push(0x31);
        }
        code.push(0x32);
        Ok(true)
    }
}
