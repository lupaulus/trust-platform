impl Runtime {
    /// Get the current debug control handle, if set.
    #[must_use]
    pub fn debug_control(&self) -> Option<crate::debug::DebugControl> {
        self.debug.clone()
    }

    /// Register statement locations for a file id.
    pub fn register_statement_locations(
        &mut self,
        file_id: u32,
        locations: Vec<crate::debug::SourceLocation>,
    ) {
        self.statement_index.insert(file_id, locations);
    }

    /// Get the statement locations for a file id.
    #[must_use]
    pub fn statement_locations(&self, file_id: u32) -> Option<&[crate::debug::SourceLocation]> {
        self.statement_index.get(&file_id).map(Vec::as_slice)
    }

    /// Resolve a breakpoint to a statement location for the given file and source.
    #[must_use]
    pub fn resolve_breakpoint_location(
        &self,
        source: &str,
        file_id: u32,
        line: u32,
        column: u32,
    ) -> Option<crate::debug::SourceLocation> {
        let locations = self.statement_index.get(&file_id)?;
        crate::debug::resolve_breakpoint_location(source, file_id, locations, line, column)
    }

    /// Resolve a breakpoint and return its adjusted line/column.
    #[must_use]
    pub fn resolve_breakpoint_position(
        &self,
        source: &str,
        file_id: u32,
        line: u32,
        column: u32,
    ) -> Option<(crate::debug::SourceLocation, u32, u32)> {
        let location = self.resolve_breakpoint_location(source, file_id, line, column)?;
        let (resolved_line, resolved_col) = crate::debug::location_to_line_col(source, &location);
        Some((location, resolved_line, resolved_col))
    }

}
