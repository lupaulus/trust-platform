fn scaled_time(now: Duration, scale: u32) -> Duration {
    if scale <= 1 {
        return now;
    }
    let scaled = now.as_nanos().saturating_mul(i64::from(scale));
    Duration::from_nanos(scaled)
}

fn scaled_sleep_interval(interval: Duration, scale: u32) -> Duration {
    if scale <= 1 {
        return interval;
    }
    let nanos = interval.as_nanos();
    if nanos <= 0 {
        return Duration::ZERO;
    }
    let scaled = (nanos / i64::from(scale)).max(1);
    Duration::from_nanos(scaled)
}

fn run_resource_loop<C: Clock + Clone>(
    mut runner: ResourceRunner<C>,
    stop: Arc<AtomicBool>,
    state: Arc<Mutex<ResourceState>>,
    last_error: Arc<Mutex<Option<RuntimeError>>>,
) {
    let mut paused = false;
    if let Some(gate) = runner.start_gate.as_ref() {
        *state.lock().expect("resource state poisoned") = ResourceState::Ready;
        if !gate.wait_open(&stop) {
            *state.lock().expect("resource state poisoned") = ResourceState::Stopped;
            return;
        }
    }
    *state.lock().expect("resource state poisoned") = ResourceState::Running;
    loop {
        if stop.load(Ordering::SeqCst) {
            let _ = runner.runtime.save_retain_store();
            *state.lock().expect("resource state poisoned") = ResourceState::Stopped;
            break;
        }

        if let Some(commands) = runner.command_rx.as_ref() {
            while let Ok(command) = commands.try_recv() {
                match command {
                    ResourceCommand::Pause => {
                        paused = true;
                        *state.lock().expect("resource state poisoned") = ResourceState::Paused;
                    }
                    ResourceCommand::Resume => {
                        paused = false;
                        *state.lock().expect("resource state poisoned") = ResourceState::Running;
                    }
                    other => apply_resource_command(&mut runner.runtime, other),
                }
            }
        }

        if let Some(signal) = runner.restart_signal.as_ref() {
            if let Ok(mut guard) = signal.lock() {
                if let Some(mode) = guard.take() {
                    if let Err(err) = runner.runtime.restart(mode) {
                        *last_error.lock().expect("resource error poisoned") = Some(err);
                        *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
                        break;
                    }
                    if let Err(err) = runner.runtime.load_retain_store() {
                        *last_error.lock().expect("resource error poisoned") = Some(err);
                        *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
                        break;
                    }
                }
            }
        }

        if paused {
            let now_raw = runner.clock.now();
            let interval = runner.cycle_interval.as_nanos();
            if interval <= 0 {
                thread::yield_now();
            } else {
                let sleep_interval =
                    scaled_sleep_interval(runner.cycle_interval, runner.time_scale);
                let deadline = Duration::from_nanos(
                    now_raw.as_nanos().saturating_add(sleep_interval.as_nanos()),
                );
                runner.clock.sleep_until(deadline);
            }
            continue;
        }

        let now_raw = runner.clock.now();
        let now = scaled_time(now_raw, runner.time_scale);
        runner.runtime.set_current_time(now);
        let wall_start = std::time::Instant::now();
        if let Some(simulation) = runner.simulation.as_mut() {
            if let Err(err) = simulation.apply_pre_cycle(now, &mut runner.runtime) {
                if matches!(
                    runner.runtime.fault_policy(),
                    crate::watchdog::FaultPolicy::Restart
                ) {
                    if let Err(restart_err) = runner.runtime.restart(crate::RestartMode::Warm) {
                        *last_error.lock().expect("resource error poisoned") = Some(restart_err);
                        *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
                        break;
                    }
                    continue;
                }
                *last_error.lock().expect("resource error poisoned") = Some(err);
                *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
                break;
            }
        }
        let mut result = runner.runtime.execute_cycle();
        if result.is_ok() {
            if let Some(simulation) = runner.simulation.as_mut() {
                result = simulation.apply_post_cycle(now, &runner.runtime);
            }
        }
        if let Err(err) = result {
            if matches!(
                runner.runtime.fault_policy(),
                crate::watchdog::FaultPolicy::Restart
            ) {
                if let Err(restart_err) = runner.runtime.restart(crate::RestartMode::Warm) {
                    *last_error.lock().expect("resource error poisoned") = Some(restart_err);
                    *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
                    break;
                }
                continue;
            }
            *last_error.lock().expect("resource error poisoned") = Some(err);
            *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
            break;
        }

        let watchdog = runner.runtime.watchdog_policy();
        if watchdog.enabled {
            let elapsed = i64::try_from(wall_start.elapsed().as_nanos()).unwrap_or(i64::MAX);
            if elapsed > watchdog.timeout.as_nanos() {
                if matches!(watchdog.action, crate::watchdog::WatchdogAction::Restart) {
                    if let Err(restart_err) = runner.runtime.restart(crate::RestartMode::Warm) {
                        *last_error.lock().expect("resource error poisoned") = Some(restart_err);
                        *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
                        break;
                    }
                } else {
                    let err = runner.runtime.watchdog_timeout();
                    *last_error.lock().expect("resource error poisoned") = Some(err);
                    *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
                    break;
                }
            }
        }

        let interval = runner.cycle_interval.as_nanos();
        if interval <= 0 {
            thread::yield_now();
            continue;
        }
        let sleep_interval = scaled_sleep_interval(runner.cycle_interval, runner.time_scale);
        let deadline =
            Duration::from_nanos(now_raw.as_nanos().saturating_add(sleep_interval.as_nanos()));
        runner.clock.sleep_until(deadline);
    }
}

fn run_resource_loop_with_shared<C: Clock + Clone>(
    mut runner: ResourceRunner<C>,
    stop: Arc<AtomicBool>,
    state: Arc<Mutex<ResourceState>>,
    last_error: Arc<Mutex<Option<RuntimeError>>>,
    shared: SharedGlobals,
) {
    let mut paused = false;
    if let Some(gate) = runner.start_gate.as_ref() {
        *state.lock().expect("resource state poisoned") = ResourceState::Ready;
        if !gate.wait_open(&stop) {
            *state.lock().expect("resource state poisoned") = ResourceState::Stopped;
            return;
        }
    }
    *state.lock().expect("resource state poisoned") = ResourceState::Running;
    loop {
        if stop.load(Ordering::SeqCst) {
            let _ = runner.runtime.save_retain_store();
            *state.lock().expect("resource state poisoned") = ResourceState::Stopped;
            break;
        }

        if let Some(commands) = runner.command_rx.as_ref() {
            while let Ok(command) = commands.try_recv() {
                match command {
                    ResourceCommand::Pause => {
                        paused = true;
                        *state.lock().expect("resource state poisoned") = ResourceState::Paused;
                    }
                    ResourceCommand::Resume => {
                        paused = false;
                        *state.lock().expect("resource state poisoned") = ResourceState::Running;
                    }
                    other => apply_resource_command(&mut runner.runtime, other),
                }
            }
        }

        if let Some(signal) = runner.restart_signal.as_ref() {
            if let Ok(mut guard) = signal.lock() {
                if let Some(mode) = guard.take() {
                    if let Err(err) = runner.runtime.restart(mode) {
                        *last_error.lock().expect("resource error poisoned") = Some(err);
                        *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
                        break;
                    }
                    if let Err(err) = runner.runtime.load_retain_store() {
                        *last_error.lock().expect("resource error poisoned") = Some(err);
                        *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
                        break;
                    }
                }
            }
        }

        if paused {
            let now_raw = runner.clock.now();
            let interval = runner.cycle_interval.as_nanos();
            if interval <= 0 {
                thread::yield_now();
            } else {
                let sleep_interval =
                    scaled_sleep_interval(runner.cycle_interval, runner.time_scale);
                let deadline = Duration::from_nanos(
                    now_raw.as_nanos().saturating_add(sleep_interval.as_nanos()),
                );
                runner.clock.sleep_until(deadline);
            }
            continue;
        }

        let now_raw = runner.clock.now();
        let now = scaled_time(now_raw, runner.time_scale);
        runner.runtime.set_current_time(now);
        let wall_start = std::time::Instant::now();
        if let Some(simulation) = runner.simulation.as_mut() {
            if let Err(err) = simulation.apply_pre_cycle(now, &mut runner.runtime) {
                if matches!(
                    runner.runtime.fault_policy(),
                    crate::watchdog::FaultPolicy::Restart
                ) {
                    if let Err(restart_err) = runner.runtime.restart(crate::RestartMode::Warm) {
                        *last_error.lock().expect("resource error poisoned") = Some(restart_err);
                        *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
                        break;
                    }
                    continue;
                }
                *last_error.lock().expect("resource error poisoned") = Some(err);
                *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
                break;
            }
        }
        let mut result = shared.with_lock(|globals| {
            shared.sync_into_locked(globals, &mut runner.runtime)?;
            let result = runner.runtime.execute_cycle();
            shared.sync_from_locked(globals, &runner.runtime)?;
            result
        });
        if result.is_ok() {
            if let Some(simulation) = runner.simulation.as_mut() {
                result = simulation.apply_post_cycle(now, &runner.runtime);
            }
        }
        if let Err(err) = result {
            if matches!(
                runner.runtime.fault_policy(),
                crate::watchdog::FaultPolicy::Restart
            ) {
                if let Err(restart_err) = runner.runtime.restart(crate::RestartMode::Warm) {
                    *last_error.lock().expect("resource error poisoned") = Some(restart_err);
                    *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
                    break;
                }
                continue;
            }
            *last_error.lock().expect("resource error poisoned") = Some(err);
            *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
            break;
        }
        let watchdog = runner.runtime.watchdog_policy();
        if watchdog.enabled {
            let elapsed = i64::try_from(wall_start.elapsed().as_nanos()).unwrap_or(i64::MAX);
            if elapsed > watchdog.timeout.as_nanos() {
                if matches!(watchdog.action, crate::watchdog::WatchdogAction::Restart) {
                    if let Err(restart_err) = runner.runtime.restart(crate::RestartMode::Warm) {
                        *last_error.lock().expect("resource error poisoned") = Some(restart_err);
                        *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
                        break;
                    }
                } else {
                    let err = runner.runtime.watchdog_timeout();
                    *last_error.lock().expect("resource error poisoned") = Some(err);
                    *state.lock().expect("resource state poisoned") = ResourceState::Faulted;
                    break;
                }
            }
        }

        let interval = runner.cycle_interval.as_nanos();
        if interval <= 0 {
            thread::yield_now();
            continue;
        }
        let sleep_interval = scaled_sleep_interval(runner.cycle_interval, runner.time_scale);
        let deadline =
            Duration::from_nanos(now_raw.as_nanos().saturating_add(sleep_interval.as_nanos()));
        runner.clock.sleep_until(deadline);
    }
}

fn apply_resource_command(runtime: &mut Runtime, command: ResourceCommand) {
    match command {
        ResourceCommand::Pause | ResourceCommand::Resume => {}
        ResourceCommand::UpdateWatchdog(policy) => runtime.set_watchdog_policy(policy),
        ResourceCommand::UpdateFaultPolicy(policy) => runtime.set_fault_policy(policy),
        ResourceCommand::UpdateRetainSaveInterval(interval) => {
            runtime.set_retain_save_interval(interval)
        }
        ResourceCommand::UpdateIoSafeState(state) => runtime.set_io_safe_state(state),
        ResourceCommand::ReloadBytecode { bytes, respond_to } => {
            let result = runtime
                .apply_bytecode_bytes(&bytes, None)
                .and_then(|_| runtime.restart(crate::RestartMode::Warm))
                .and_then(|_| runtime.load_retain_store())
                .map(|_| runtime.metadata_snapshot());
            let _ = respond_to.send(result);
        }
        ResourceCommand::MeshSnapshot { names, respond_to } => {
            let snapshot = runtime.snapshot_globals(&names);
            let _ = respond_to.send(snapshot);
        }
        ResourceCommand::MeshApply {
            updates,
            source: _,
            sequence: _,
        } => runtime.apply_mesh_updates(&updates),
        ResourceCommand::Snapshot { respond_to } => {
            let snapshot = crate::debug::DebugSnapshot {
                storage: runtime.storage().clone(),
                now: runtime.current_time(),
            };
            let _ = respond_to.send(snapshot);
        }
    }
}
