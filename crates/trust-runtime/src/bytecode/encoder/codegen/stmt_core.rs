impl<'a> BytecodeEncoder<'a> {
    pub(super) fn emit_pou_body(
        &mut self,
        ctx: &mut CodegenContext,
        pou_id: u32,
        body: &[crate::eval::stmt::Stmt],
    ) -> Result<(Vec<u8>, Vec<DebugEntry>), BytecodeError> {
        let mut code = Vec::new();
        let mut debug_entries = Vec::new();
        for stmt in body {
            self.emit_stmt(ctx, pou_id, stmt, &mut code, &mut debug_entries)?;
        }
        Ok((code, debug_entries))
    }

    fn emit_stmt(
        &mut self,
        ctx: &mut CodegenContext,
        pou_id: u32,
        stmt: &crate::eval::stmt::Stmt,
        code: &mut Vec<u8>,
        debug_entries: &mut Vec<DebugEntry>,
    ) -> Result<(), BytecodeError> {
        let offset = to_u32(code.len(), "debug code offset")?;
        if let (Some(location), Some(sources)) = (stmt.location(), self.sources) {
            let source = sources
                .get(location.file_id as usize)
                .ok_or_else(|| BytecodeError::InvalidSection("debug source missing".into()))?;
            let (line, column) = crate::debug::location_to_line_col(source, location);
            let line = line.saturating_add(1);
            let column = column.saturating_add(1);
            let file_idx = self.file_path_index(location.file_id)?;
            debug_entries.push(DebugEntry {
                pou_id,
                code_offset: offset,
                file_idx,
                line,
                column,
                kind: 0,
            });
        }
        let emitted = match stmt {
            crate::eval::stmt::Stmt::Assign { target, value, .. } => {
                self.emit_assign(ctx, target, value, code)?
            }
            crate::eval::stmt::Stmt::Expr { expr, .. } => {
                if !self.emit_expr(ctx, expr, code)? {
                    false
                } else {
                    code.push(0x12); // POP (statement value is discarded)
                    true
                }
            }
            crate::eval::stmt::Stmt::If {
                condition,
                then_block,
                else_if,
                else_block,
                ..
            } => self.emit_if_stmt(
                ctx,
                pou_id,
                condition,
                then_block,
                else_if,
                else_block,
                code,
                debug_entries,
            )?,
            crate::eval::stmt::Stmt::Case {
                selector,
                branches,
                else_block,
                ..
            } => self.emit_case_stmt(
                ctx,
                pou_id,
                selector,
                branches,
                else_block,
                code,
                debug_entries,
            )?,
            crate::eval::stmt::Stmt::While {
                condition, body, ..
            } => self.emit_while_stmt(ctx, pou_id, condition, body, code, debug_entries)?,
            crate::eval::stmt::Stmt::Repeat { body, until, .. } => {
                self.emit_repeat_stmt(ctx, pou_id, body, until, code, debug_entries)?
            }
            crate::eval::stmt::Stmt::For {
                control,
                start,
                end,
                step,
                body,
                ..
            } => self.emit_for_stmt(
                ctx,
                pou_id,
                control,
                start,
                end,
                step,
                body,
                code,
                debug_entries,
            )?,
            crate::eval::stmt::Stmt::Label { stmt, .. } => {
                if let Some(stmt) = stmt.as_deref() {
                    self.emit_stmt(ctx, pou_id, stmt, code, debug_entries)?;
                    true
                } else {
                    false
                }
            }
            _ => false,
        };

        if !emitted {
            if stmt_contains_c1_required_call(stmt) {
                return Err(BytecodeError::InvalidSection(
                    "unsupported C1 CALL_NATIVE lowering path".into(),
                ));
            }
            code.push(0x00);
        }
        Ok(())
    }

    fn emit_block(
        &mut self,
        ctx: &mut CodegenContext,
        pou_id: u32,
        block: &[crate::eval::stmt::Stmt],
        code: &mut Vec<u8>,
        debug_entries: &mut Vec<DebugEntry>,
    ) -> Result<(), BytecodeError> {
        for stmt in block {
            self.emit_stmt(ctx, pou_id, stmt, code, debug_entries)?;
        }
        Ok(())
    }

}

fn stmt_contains_c1_required_call(stmt: &crate::eval::stmt::Stmt) -> bool {
    use crate::eval::stmt::Stmt;
    match stmt {
        Stmt::Assign { value, .. } => expr_contains_call(value),
        Stmt::Expr { expr, .. } => expr_contains_call(expr),
        Stmt::If {
            condition,
            then_block,
            else_if,
            else_block,
            ..
        } => {
            expr_contains_call(condition)
                || then_block.iter().any(stmt_contains_c1_required_call)
                || else_if
                    .iter()
                    .any(|(expr, block)| {
                        expr_contains_call(expr)
                            || block.iter().any(stmt_contains_c1_required_call)
                    })
                || else_block.iter().any(stmt_contains_c1_required_call)
        }
        Stmt::Case {
            selector,
            branches,
            else_block,
            ..
        } => {
            expr_contains_call(selector)
                || branches.iter().any(|(_, block)| {
                    block.iter().any(stmt_contains_c1_required_call)
                })
                || else_block.iter().any(stmt_contains_c1_required_call)
        }
        Stmt::While {
            condition, body, ..
        } => {
            expr_contains_call(condition) || body.iter().any(stmt_contains_c1_required_call)
        }
        Stmt::Repeat { body, until, .. } => {
            body.iter().any(stmt_contains_c1_required_call) || expr_contains_call(until)
        }
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            expr_contains_call(start)
                || expr_contains_call(end)
                || expr_contains_call(step)
                || body.iter().any(stmt_contains_c1_required_call)
        }
        Stmt::Label { stmt, .. } => stmt
            .as_deref()
            .map(stmt_contains_c1_required_call)
            .unwrap_or(false),
        Stmt::AssignAttempt { value, .. } | Stmt::Return { expr: Some(value), .. } => {
            expr_contains_call(value)
        }
        Stmt::Return { expr: None, .. }
        | Stmt::Jmp { .. }
        | Stmt::Exit { .. }
        | Stmt::Continue { .. } => false,
    }
}

fn expr_contains_call(expr: &crate::eval::expr::Expr) -> bool {
    use crate::eval::expr::Expr;
    match expr {
        Expr::Call { .. } => true,
        Expr::Unary { expr, .. } => expr_contains_call(expr),
        Expr::Binary { left, right, .. } => expr_contains_call(left) || expr_contains_call(right),
        Expr::Index { target, indices } => {
            expr_contains_call(target) || indices.iter().any(expr_contains_call)
        }
        Expr::Field { target, .. } => expr_contains_call(target),
        Expr::Ref(lvalue) => lvalue_contains_call(lvalue),
        Expr::Deref(expr) | Expr::SizeOf(crate::eval::expr::SizeOfTarget::Expr(expr)) => {
            expr_contains_call(expr)
        }
        Expr::SizeOf(crate::eval::expr::SizeOfTarget::Type(_))
        | Expr::Literal(_)
        | Expr::This
        | Expr::Super
        | Expr::Name(_) => false,
    }
}

fn lvalue_contains_call(lvalue: &crate::eval::expr::LValue) -> bool {
    use crate::eval::expr::LValue;
    match lvalue {
        LValue::Name(_) => false,
        LValue::Field { .. } => false,
        LValue::Index { indices, .. } => indices.iter().any(expr_contains_call),
        LValue::Deref(expr) => expr_contains_call(expr),
    }
}
