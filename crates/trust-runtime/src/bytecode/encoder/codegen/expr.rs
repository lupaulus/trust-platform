impl<'a> BytecodeEncoder<'a> {
    fn emit_expr(
        &mut self,
        ctx: &CodegenContext,
        expr: &crate::eval::expr::Expr,
        code: &mut Vec<u8>,
    ) -> Result<bool, BytecodeError> {
        let start_len = code.len();
        let result = match expr {
            crate::eval::expr::Expr::Literal(value) => {
                let const_idx = match self.const_index_for(value) {
                    Ok(idx) => idx,
                    Err(_) => {
                        code.truncate(start_len);
                        return Ok(false);
                    }
                };
                code.push(0x10);
                code.extend_from_slice(&const_idx.to_le_bytes());
                Ok(true)
            }
            crate::eval::expr::Expr::Name(name) => {
                if let Some(reference) = ctx.local_ref(name) {
                    let ref_idx = self.ref_index_for(reference)?;
                    code.push(0x20);
                    code.extend_from_slice(&ref_idx.to_le_bytes());
                    return Ok(true);
                }
                if self.emit_dynamic_load_name(ctx, name, code)? {
                    return Ok(true);
                }
                let reference = match self.resolve_name_ref(ctx, name)? {
                    Some(reference) => reference,
                    None => {
                        code.truncate(start_len);
                        return Ok(false);
                    }
                };
                let ref_idx = self.ref_index_for(&reference)?;
                code.push(0x20);
                code.extend_from_slice(&ref_idx.to_le_bytes());
                Ok(true)
            }
            crate::eval::expr::Expr::Field { target, field } => {
                if let crate::eval::expr::Expr::Name(base) = target.as_ref() {
                    if self.emit_dynamic_load_field(ctx, base, field, code)? {
                        return Ok(true);
                    }
                    code.truncate(start_len);
                    let reference = match self.resolve_lvalue_ref(
                        ctx,
                        &crate::eval::expr::LValue::Field {
                            name: base.clone(),
                            field: field.clone(),
                        },
                    )? {
                        Some(reference) => reference,
                        None => {
                            code.truncate(start_len);
                            return Ok(false);
                        }
                    };
                    let ref_idx = self.ref_index_for(&reference)?;
                    code.push(0x20);
                    code.extend_from_slice(&ref_idx.to_le_bytes());
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            crate::eval::expr::Expr::Index { target, indices } => {
                if let crate::eval::expr::Expr::Name(base) = target.as_ref() {
                    if self.emit_dynamic_load_index(ctx, base, indices, code)? {
                        return Ok(true);
                    }
                    code.truncate(start_len);
                    let reference = match self.resolve_lvalue_ref(
                        ctx,
                        &crate::eval::expr::LValue::Index {
                            name: base.clone(),
                            indices: indices.clone(),
                        },
                    )? {
                        Some(reference) => reference,
                        None => {
                            code.truncate(start_len);
                            return Ok(false);
                        }
                    };
                    let ref_idx = self.ref_index_for(&reference)?;
                    code.push(0x20);
                    code.extend_from_slice(&ref_idx.to_le_bytes());
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            crate::eval::expr::Expr::Unary { op, expr } => {
                use crate::eval::ops::UnaryOp;
                if !self.emit_expr(ctx, expr, code)? {
                    code.truncate(start_len);
                    return Ok(false);
                }
                match op {
                    UnaryOp::Neg => code.push(0x45),
                    UnaryOp::Not => code.push(0x49),
                    UnaryOp::Pos => {}
                }
                Ok(true)
            }
            crate::eval::expr::Expr::Binary { op, left, right } => {
                use crate::eval::ops::BinaryOp;
                let opcode = match op {
                    BinaryOp::Add => 0x40,
                    BinaryOp::Sub => 0x41,
                    BinaryOp::Mul => 0x42,
                    BinaryOp::Div => 0x43,
                    BinaryOp::Mod => 0x44,
                    BinaryOp::Pow => 0x4C,
                    BinaryOp::And => 0x46,
                    BinaryOp::Or => 0x47,
                    BinaryOp::Xor => 0x48,
                    BinaryOp::Eq => 0x50,
                    BinaryOp::Ne => 0x51,
                    BinaryOp::Lt => 0x52,
                    BinaryOp::Le => 0x53,
                    BinaryOp::Gt => 0x54,
                    BinaryOp::Ge => 0x55,
                };
                if !self.emit_expr(ctx, left, code)? {
                    code.truncate(start_len);
                    return Ok(false);
                }
                if !self.emit_expr(ctx, right, code)? {
                    code.truncate(start_len);
                    return Ok(false);
                }
                code.push(opcode);
                Ok(true)
            }
            _ => Ok(false),
        };
        match result {
            Ok(true) => Ok(true),
            Ok(false) => {
                code.truncate(start_len);
                Ok(false)
            }
            Err(err) => {
                code.truncate(start_len);
                Err(err)
            }
        }
    }
}
