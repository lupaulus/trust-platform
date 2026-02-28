/// Drives a runtime with a scheduling clock.
#[derive(Debug)]
pub struct ResourceRunner<C: Clock + Clone> {
    runtime: Runtime,
    clock: C,
    cycle_interval: Duration,
    time_scale: u32,
    restart_signal: Option<Arc<Mutex<Option<crate::RestartMode>>>>,
    start_gate: Option<Arc<StartGate>>,
    command_rx: Option<std::sync::mpsc::Receiver<ResourceCommand>>,
    simulation: Option<crate::simulation::SimulationController>,
}

impl<C: Clock + Clone> ResourceRunner<C> {
    #[must_use]
    pub fn new(runtime: Runtime, clock: C, cycle_interval: Duration) -> Self {
        Self {
            runtime,
            clock,
            cycle_interval,
            time_scale: 1,
            restart_signal: None,
            start_gate: None,
            command_rx: None,
            simulation: None,
        }
    }

    /// Attach a restart signal for external control.
    #[must_use]
    pub fn with_restart_signal(mut self, signal: Arc<Mutex<Option<crate::RestartMode>>>) -> Self {
        self.restart_signal = Some(signal);
        self
    }

    /// Attach a start gate that must be opened before the scheduler runs.
    #[must_use]
    pub fn with_start_gate(mut self, gate: Arc<StartGate>) -> Self {
        self.start_gate = Some(gate);
        self
    }

    /// Apply simulation time acceleration (`>= 1`).
    #[must_use]
    pub fn with_time_scale(mut self, scale: u32) -> Self {
        self.time_scale = scale.max(1);
        self
    }

    /// Attach a simulation controller for coupling/disturbance hooks.
    #[must_use]
    pub fn with_simulation(mut self, simulation: crate::simulation::SimulationController) -> Self {
        self.simulation = Some(simulation);
        self
    }

    /// Access the underlying runtime.
    #[must_use]
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// Mutate the underlying runtime.
    pub fn runtime_mut(&mut self) -> &mut Runtime {
        &mut self.runtime
    }

    /// Execute one cycle using the current clock time.
    pub fn tick(&mut self) -> Result<(), RuntimeError> {
        let now = self.clock.now();
        self.runtime.set_current_time(now);
        self.runtime.execute_cycle()
    }

    /// Execute one cycle with shared global synchronization.
    pub fn tick_with_shared(&mut self, shared: &SharedGlobals) -> Result<(), RuntimeError> {
        let now = self.clock.now();
        self.runtime.set_current_time(now);
        shared.with_lock(|globals| {
            shared.sync_into_locked(globals, &mut self.runtime)?;
            let result = self.runtime.execute_cycle();
            shared.sync_from_locked(globals, &self.runtime)?;
            result
        })
    }

    /// Spawn the runner in a dedicated OS thread.
    pub fn spawn(self, name: impl Into<String>) -> Result<ResourceHandle<C>, RuntimeError> {
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(ResourceState::Boot));
        let last_error = Arc::new(Mutex::new(None));
        let clock = self.clock.clone();
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let mut runner = self;
        runner.command_rx = Some(cmd_rx);

        let stop_thread = stop.clone();
        let state_thread = state.clone();
        let last_error_thread = last_error.clone();

        let (id_tx, id_rx) = std::sync::mpsc::channel();
        let builder = thread::Builder::new().name(name.into());
        let join = builder
            .spawn(move || {
                let _ = id_tx.send(thread::current().id());
                run_resource_loop(runner, stop_thread, state_thread, last_error_thread);
            })
            .map_err(|err| RuntimeError::ThreadSpawn(err.to_string().into()))?;

        let thread_id = id_rx.recv().unwrap_or_else(|_| join.thread().id());

        Ok(ResourceHandle {
            stop,
            state,
            last_error,
            thread_id,
            clock,
            join: Some(join),
            cmd_tx: cmd_tx.clone(),
        })
    }

    /// Spawn the runner with shared global synchronization.
    pub fn spawn_with_shared(
        self,
        name: impl Into<String>,
        shared: SharedGlobals,
    ) -> Result<ResourceHandle<C>, RuntimeError> {
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(ResourceState::Boot));
        let last_error = Arc::new(Mutex::new(None));
        let clock = self.clock.clone();
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let mut runner = self;
        runner.command_rx = Some(cmd_rx);

        let stop_thread = stop.clone();
        let state_thread = state.clone();
        let last_error_thread = last_error.clone();
        let shared_thread = shared.clone();

        let (id_tx, id_rx) = std::sync::mpsc::channel();
        let builder = thread::Builder::new().name(name.into());
        let join = builder
            .spawn(move || {
                let _ = id_tx.send(thread::current().id());
                run_resource_loop_with_shared(
                    runner,
                    stop_thread,
                    state_thread,
                    last_error_thread,
                    shared_thread,
                );
            })
            .map_err(|err| RuntimeError::ThreadSpawn(err.to_string().into()))?;

        let thread_id = id_rx.recv().unwrap_or_else(|_| join.thread().id());

        Ok(ResourceHandle {
            stop,
            state,
            last_error,
            thread_id,
            clock,
            join: Some(join),
            cmd_tx: cmd_tx.clone(),
        })
    }
}
