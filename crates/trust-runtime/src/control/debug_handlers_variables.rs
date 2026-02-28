pub(super) fn handle_debug_scopes(
    id: u64,
    params: Option<serde_json::Value>,
    state: &ControlState,
) -> ControlResponse {
    let params: DebugScopesParams = match params {
        Some(value) => match serde_json::from_value(value) {
            Ok(parsed) => parsed,
            Err(err) => return ControlResponse::error(id, format!("invalid params: {err}")),
        },
        None => return ControlResponse::error(id, "missing params".into()),
    };
    debug!("control debug.scopes frame_id={}", params.frame_id);
    let snapshot = match state.debug.snapshot() {
        Some(snapshot) => snapshot,
        None => return ControlResponse::error(id, "no snapshot available".into()),
    };
    let requested_frame = crate::memory::FrameId(params.frame_id);
    let current_frame = snapshot.storage.current_frame().map(|frame| frame.id);
    let has_requested_frame = snapshot
        .storage
        .frames()
        .iter()
        .any(|frame| frame.id == requested_frame);
    let frame_id = if has_requested_frame {
        requested_frame
    } else {
        current_frame.unwrap_or(requested_frame)
    };
    let location = state
        .debug
        .frame_location(frame_id)
        .or_else(|| state.debug.last_stop().and_then(|stop| stop.location))
        .and_then(|loc| location_to_source(&loc, state));
    let has_frame = has_requested_frame || current_frame.is_some();
    let (has_globals, has_retain, has_instances) = (
        !snapshot.storage.globals().is_empty(),
        !snapshot.storage.retain().is_empty(),
        !snapshot.storage.instances().is_empty(),
    );
    debug!(
        "control debug.scopes has_frame={} current_frame={:?} requested_frame={} has_globals={} has_retain={} has_instances={}",
        has_frame,
        current_frame,
        params.frame_id,
        has_globals,
        has_retain,
        has_instances
    );
    let io_snapshot = state
        .io_snapshot
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    let has_io = crate::debug::dap::io_scope_available(io_snapshot.as_ref());

    let mut handles = match state.debug_variables.lock() {
        Ok(guard) => guard,
        Err(_) => return ControlResponse::error(id, "debug variables unavailable".into()),
    };
    handles.clear();

    let mut scopes = Vec::new();
    if has_frame {
        let locals_ref = handles.alloc(VariableHandle::Locals(frame_id));
        scopes.push(DebugScope {
            name: "Locals".to_string(),
            variables_reference: locals_ref,
            expensive: false,
            source: location.as_ref().map(|(source, _, _)| source.clone()),
            line: location.as_ref().map(|(_, line, _)| *line),
            column: location.as_ref().map(|(_, _, column)| *column),
            end_line: None,
            end_column: None,
        });
    }
    if has_globals {
        let globals_ref = handles.alloc(VariableHandle::Globals);
        scopes.push(DebugScope {
            name: "Globals".to_string(),
            variables_reference: globals_ref,
            expensive: false,
            source: None,
            line: None,
            column: None,
            end_line: None,
            end_column: None,
        });
    }
    if has_retain {
        let retain_ref = handles.alloc(VariableHandle::Retain);
        scopes.push(DebugScope {
            name: "Retain".to_string(),
            variables_reference: retain_ref,
            expensive: false,
            source: None,
            line: None,
            column: None,
            end_line: None,
            end_column: None,
        });
    }
    if has_io {
        let io_ref = handles.alloc(VariableHandle::IoRoot);
        scopes.push(DebugScope {
            name: "I/O".to_string(),
            variables_reference: io_ref,
            expensive: false,
            source: None,
            line: None,
            column: None,
            end_line: None,
            end_column: None,
        });
    }
    if has_instances {
        let instances_ref = handles.alloc(VariableHandle::Instances);
        scopes.push(DebugScope {
            name: "Instances".to_string(),
            variables_reference: instances_ref,
            expensive: false,
            source: None,
            line: None,
            column: None,
            end_line: None,
            end_column: None,
        });
    }

    debug!(
        "control debug.scopes result={:?}",
        scopes
            .iter()
            .map(|scope| scope.name.as_str())
            .collect::<Vec<_>>()
    );
    ControlResponse::ok(id, json!({ "scopes": scopes }))
}

pub(super) fn handle_debug_variables(
    id: u64,
    params: Option<serde_json::Value>,
    state: &ControlState,
) -> ControlResponse {
    let params: DebugVariablesParams = match params {
        Some(value) => match serde_json::from_value(value) {
            Ok(parsed) => parsed,
            Err(err) => return ControlResponse::error(id, format!("invalid params: {err}")),
        },
        None => return ControlResponse::error(id, "missing params".into()),
    };
    debug!(
        "control debug.variables reference={}",
        params.variables_reference
    );
    let snapshot = match state.debug.snapshot() {
        Some(snapshot) => snapshot,
        None => return ControlResponse::error(id, "no snapshot available".into()),
    };
    let io_snapshot = state
        .io_snapshot
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    let mut handles = match state.debug_variables.lock() {
        Ok(guard) => guard,
        Err(_) => return ControlResponse::error(id, "debug variables unavailable".into()),
    };
    let Some(handle) = handles.get(params.variables_reference).cloned() else {
        return ControlResponse::ok(id, json!({ "variables": [] }));
    };
    debug!("control debug.variables handle={:?}", handle);
    let variables = match handle {
        VariableHandle::Locals(frame_id) => {
            let entries = snapshot
                .storage
                .frames()
                .iter()
                .find(|frame| frame.id == frame_id)
                .map(|frame| {
                    let mut entries = Vec::new();
                    if let Some(instance_id) = frame.instance_id {
                        if let Some(instance) = snapshot.storage.get_instance(instance_id) {
                            entries.extend(
                                instance
                                    .variables
                                    .iter()
                                    .map(|(name, value)| (name.to_string(), value.clone())),
                            );
                        }
                    }
                    entries.extend(
                        frame
                            .variables
                            .iter()
                            .map(|(name, value)| (name.to_string(), value.clone())),
                    );
                    entries
                })
                .unwrap_or_default();
            crate::debug::dap::variables_from_entries(&mut handles, entries)
        }
        VariableHandle::Globals => {
            let entries = snapshot
                .storage
                .globals()
                .iter()
                .map(|(name, value)| (name.to_string(), value.clone()))
                .collect::<Vec<_>>();
            crate::debug::dap::variables_from_entries(&mut handles, entries)
        }
        VariableHandle::Retain => {
            let entries = snapshot
                .storage
                .retain()
                .iter()
                .map(|(name, value)| (name.to_string(), value.clone()))
                .collect::<Vec<_>>();
            crate::debug::dap::variables_from_entries(&mut handles, entries)
        }
        VariableHandle::Instances => {
            let instances = snapshot
                .storage
                .instances()
                .iter()
                .map(|(id, data)| (*id, format!("{}#{}", data.type_name, id.0)))
                .collect::<Vec<_>>();
            crate::debug::dap::variables_from_instances(&mut handles, instances)
        }
        VariableHandle::Instance(instance_id) => {
            let entries = snapshot
                .storage
                .get_instance(instance_id)
                .map(|instance| {
                    let mut entries = instance
                        .variables
                        .iter()
                        .map(|(name, value)| (name.to_string(), value.clone()))
                        .collect::<Vec<_>>();
                    if let Some(parent) = instance.parent {
                        entries.push(("parent".to_string(), Value::Instance(parent)));
                    }
                    entries
                })
                .unwrap_or_default();
            crate::debug::dap::variables_from_entries(&mut handles, entries)
        }
        VariableHandle::Struct(value) => {
            crate::debug::dap::variables_from_struct(&mut handles, value)
        }
        VariableHandle::Array(value) => {
            crate::debug::dap::variables_from_array(&mut handles, value)
        }
        VariableHandle::Reference(value_ref) => {
            let value = snapshot.storage.read_by_ref(value_ref).cloned();
            value
                .map(|value| {
                    vec![crate::debug::dap::variable_from_value(
                        &mut handles,
                        "*".to_string(),
                        value,
                        None,
                    )]
                })
                .unwrap_or_default()
        }
        VariableHandle::IoRoot => {
            let inputs_ref = handles.alloc(VariableHandle::IoInputs);
            let outputs_ref = handles.alloc(VariableHandle::IoOutputs);
            let memory_ref = handles.alloc(VariableHandle::IoMemory);
            let state = io_snapshot.unwrap_or_default();
            vec![
                DebugVariable {
                    name: "Inputs".to_string(),
                    value: format!("{} items", state.inputs.len()),
                    r#type: None,
                    variables_reference: inputs_ref,
                    evaluate_name: None,
                },
                DebugVariable {
                    name: "Outputs".to_string(),
                    value: format!("{} items", state.outputs.len()),
                    r#type: None,
                    variables_reference: outputs_ref,
                    evaluate_name: None,
                },
                DebugVariable {
                    name: "Memory".to_string(),
                    value: format!("{} items", state.memory.len()),
                    r#type: None,
                    variables_reference: memory_ref,
                    evaluate_name: None,
                },
            ]
        }
        VariableHandle::IoInputs => {
            let state = io_snapshot.unwrap_or_default();
            crate::debug::dap::variables_from_io_entries(&state.inputs)
        }
        VariableHandle::IoOutputs => {
            let state = io_snapshot.unwrap_or_default();
            crate::debug::dap::variables_from_io_entries(&state.outputs)
        }
        VariableHandle::IoMemory => {
            let state = io_snapshot.unwrap_or_default();
            crate::debug::dap::variables_from_io_entries(&state.memory)
        }
    };
    debug!("control debug.variables result_count={}", variables.len());
    ControlResponse::ok(id, json!({ "variables": variables }))
}

