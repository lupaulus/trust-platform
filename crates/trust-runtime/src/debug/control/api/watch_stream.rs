impl DebugControl {
    pub fn register_watch_expression(&self, expr: Expr) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("debug state poisoned");
        state.watches.push(WatchEntry { expr, last: None });
    }

    /// Clear watch expressions.
    pub fn clear_watch_expressions(&self) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("debug state poisoned");
        state.watches.clear();
        state.watch_changed = false;
    }

    /// Returns whether watch values changed since the last stop, and resets the flag.
    #[must_use]
    pub fn take_watch_changed(&self) -> bool {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("debug state poisoned");
        let changed = state.watch_changed;
        state.watch_changed = false;
        changed
    }

    /// Stream log output to a sender instead of buffering.
    pub fn set_log_sender(&self, sender: Sender<DebugLog>) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("debug state poisoned");
        state.log_tx = Some(sender);
    }

    /// Stop streaming log output; new logs will buffer.
    pub fn clear_log_sender(&self) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("debug state poisoned");
        state.log_tx = None;
    }

    /// Stream I/O snapshots to a sender.
    pub fn set_io_sender(&self, sender: Sender<IoSnapshot>) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("debug state poisoned");
        state.io_tx = Some(sender);
    }

    /// Stop streaming I/O snapshots.
    pub fn clear_io_sender(&self) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("debug state poisoned");
        state.io_tx = None;
    }

    /// Stream runtime events to a sender.
    pub fn set_runtime_sender(&self, sender: Sender<RuntimeEvent>) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("debug state poisoned");
        state.runtime_tx = Some(sender);
    }

    /// Stop streaming runtime events.
    pub fn clear_runtime_sender(&self) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("debug state poisoned");
        state.runtime_tx = None;
    }

    /// Stream stop events to a sender.
    pub fn set_stop_sender(&self, sender: Sender<DebugStop>) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("debug state poisoned");
        state.stop_tx = Some(sender);
    }

    /// Stop streaming stop events.
    pub fn clear_stop_sender(&self) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("debug state poisoned");
        state.stop_tx = None;
    }

    /// Emit an I/O snapshot to listeners, if configured.
    pub fn push_io_snapshot(&self, snapshot: IoSnapshot) {
        let (lock, _) = &*self.state;
        let state = lock.lock().expect("debug state poisoned");
        if let Some(sender) = &state.io_tx {
            let _ = sender.send(snapshot);
        }
    }

    /// Emit a runtime event to listeners, if configured.
    pub fn push_runtime_event(&self, event: RuntimeEvent) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("debug state poisoned");
        if let Some(sender) = &state.runtime_tx {
            let _ = sender.send(event.clone());
        } else {
            state.runtime_events.push(event);
        }
    }

    /// Refresh the stored snapshot using the provided evaluation context.
    pub fn refresh_snapshot(&self, ctx: &mut EvalContext<'_>) {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().expect("debug state poisoned");
        update_watch_snapshot(&mut state, ctx);
        update_snapshot(&mut state, ctx);
    }
}
