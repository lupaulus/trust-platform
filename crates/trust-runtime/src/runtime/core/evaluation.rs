impl Runtime {
    /// Evaluate a debug expression within the current runtime context.
    pub fn evaluate_expression(
        &mut self,
        expr: &Expr,
        frame_id: Option<FrameId>,
    ) -> Result<Value, error::RuntimeError> {
        let profile = self.profile;
        let now = self.current_time;
        let registry = &self.registry;
        let functions = &self.functions;
        let stdlib = &self.stdlib;
        let function_blocks = &self.function_blocks;
        let classes = &self.classes;
        let access = &self.access;
        let execution_deadline = self.execution_deadline;
        let eval = |storage: &mut VariableStorage, instance_id: Option<InstanceId>| {
            let mut ctx = EvalContext {
                storage,
                registry,
                profile,
                now,
                debug: None,
                call_depth: 0,
                functions: Some(functions),
                stdlib: Some(stdlib),
                function_blocks: Some(function_blocks),
                classes: Some(classes),
                using: None,
                access: Some(access),
                current_instance: instance_id,
                return_name: None,
                loop_depth: 0,
                pause_requested: false,
                execution_deadline,
            };
            eval::eval_expr(&mut ctx, expr)
        };

        if let Some(frame_id) = frame_id {
            self.storage
                .with_frame(frame_id, |storage| {
                    let instance_id = storage.current_frame().and_then(|frame| frame.instance_id);
                    eval(storage, instance_id)
                })
                .ok_or(error::RuntimeError::InvalidFrame(frame_id.0))?
        } else {
            eval(&mut self.storage, None)
        }
    }

    /// Run a closure with an evaluation context, optionally scoped to a frame.
    pub fn with_eval_context<T>(
        &mut self,
        frame_id: Option<FrameId>,
        using: Option<&[SmolStr]>,
        f: impl FnOnce(&mut EvalContext<'_>) -> Result<T, error::RuntimeError>,
    ) -> Result<T, error::RuntimeError> {
        let profile = self.profile;
        let now = self.current_time;
        let registry = &self.registry;
        let functions = &self.functions;
        let stdlib = &self.stdlib;
        let function_blocks = &self.function_blocks;
        let classes = &self.classes;
        let access = &self.access;
        let execution_deadline = self.execution_deadline;
        let eval = |storage: &mut VariableStorage, instance_id: Option<InstanceId>| {
            let mut ctx = EvalContext {
                storage,
                registry,
                profile,
                now,
                debug: None,
                call_depth: 0,
                functions: Some(functions),
                stdlib: Some(stdlib),
                function_blocks: Some(function_blocks),
                classes: Some(classes),
                using,
                access: Some(access),
                current_instance: instance_id,
                return_name: None,
                loop_depth: 0,
                pause_requested: false,
                execution_deadline,
            };
            f(&mut ctx)
        };

        if let Some(frame_id) = frame_id {
            self.storage
                .with_frame(frame_id, |storage| {
                    let instance_id = storage.current_frame().and_then(|frame| frame.instance_id);
                    eval(storage, instance_id)
                })
                .ok_or(error::RuntimeError::InvalidFrame(frame_id.0))?
        } else {
            eval(&mut self.storage, None)
        }
    }

}
