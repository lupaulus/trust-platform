pub(super) fn handle_hmi_alarm_ack(
    id: u64,
    params: Option<serde_json::Value>,
    state: &ControlState,
) -> ControlResponse {
    let params = match params {
        Some(value) => match serde_json::from_value::<HmiAlarmAckParams>(value) {
            Ok(parsed) => parsed,
            Err(err) => return ControlResponse::error(id, format!("invalid params: {err}")),
        },
        None => return ControlResponse::error(id, "missing params".into()),
    };
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let result = match state.hmi_live.lock() {
        Ok(mut live) => {
            match crate::hmi::acknowledge_alarm(&mut live, params.id.as_str(), timestamp_ms) {
                Ok(()) => crate::hmi::build_alarm_view(&live, 100),
                Err(err) => return ControlResponse::error(id, err),
            }
        }
        Err(_) => return ControlResponse::error(id, "hmi state unavailable".into()),
    };
    ControlResponse::ok(
        id,
        serde_json::to_value(result).expect("serialize hmi.alarm.ack"),
    )
}

pub(super) fn handle_hmi_write(
    id: u64,
    params: Option<serde_json::Value>,
    state: &ControlState,
) -> ControlResponse {
    let params = match params {
        Some(value) => match serde_json::from_value::<HmiWriteParams>(value) {
            Ok(parsed) => parsed,
            Err(err) => return ControlResponse::error(id, format!("invalid params: {err}")),
        },
        None => return ControlResponse::error(id, "missing params".into()),
    };
    let target = params.id.trim();
    if target.is_empty() {
        return ControlResponse::error(id, "missing params.id".into());
    }

    let descriptor = hmi_descriptor_snapshot(state);
    let customization = descriptor.customization;
    if !customization.write_enabled() {
        return ControlResponse::error(id, "hmi.write disabled in read-only mode".into());
    }
    if customization.write_allowlist().is_empty() {
        return ControlResponse::error(id, "hmi.write allowlist is empty".into());
    }

    let metadata = match state.metadata.lock() {
        Ok(guard) => guard,
        Err(_) => return ControlResponse::error(id, "metadata unavailable".into()),
    };
    let snapshot = match load_runtime_snapshot(state) {
        Some(snapshot) => snapshot,
        None => return ControlResponse::error(id, "runtime snapshot unavailable".into()),
    };
    let point = match crate::hmi::resolve_write_point(
        state.resource_name.as_str(),
        &metadata,
        Some(&snapshot),
        target,
    ) {
        Some(point) => point,
        None => return ControlResponse::error(id, format!("unknown hmi target '{target}'")),
    };
    let allowed = customization.write_target_allowed(point.id.as_str())
        || customization.write_target_allowed(point.path.as_str());
    if !allowed {
        return ControlResponse::error(id, "hmi.write target is not in allowlist".into());
    }
    let template = match crate::hmi::resolve_write_value_template(&point, &snapshot) {
        Some(value) => value,
        None => {
            return ControlResponse::error(
                id,
                format!("hmi.write target '{}' is currently unavailable", point.id),
            )
        }
    };
    let value = match parse_hmi_write_value(&params.value, &template) {
        Some(value) => value,
        None => {
            return ControlResponse::error(
                id,
                format!("invalid hmi.write value for target '{}'", point.id),
            )
        }
    };

    match &point.binding {
        crate::hmi::HmiWriteBinding::ProgramVar { program, variable } => {
            let instance_id = match snapshot.storage.get_global(program.as_str()) {
                Some(Value::Instance(instance_id)) => *instance_id,
                _ => {
                    return ControlResponse::error(
                        id,
                        format!("hmi.write target '{}' is currently unavailable", point.id),
                    )
                }
            };
            state
                .debug
                .enqueue_instance_write(instance_id, variable.clone(), value);
        }
        crate::hmi::HmiWriteBinding::Global { name } => {
            state.debug.enqueue_global_write(name.clone(), value);
        }
    }

    ControlResponse::ok(
        id,
        json!({
            "status": "queued",
            "id": point.id,
            "path": point.path,
        }),
    )
}
