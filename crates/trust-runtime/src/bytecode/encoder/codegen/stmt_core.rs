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
