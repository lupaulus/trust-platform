pub(super) fn handle_hmi_schema_get(id: u64, state: &ControlState) -> ControlResponse {
    let metadata = match state.metadata.lock() {
        Ok(guard) => guard,
        Err(_) => return ControlResponse::error(id, "metadata unavailable".into()),
    };
    let snapshot = load_runtime_snapshot(state);
    let descriptor = hmi_descriptor_snapshot(state);
    let mut result = crate::hmi::build_schema(
        state.resource_name.as_str(),
        &metadata,
        snapshot.as_ref(),
        true,
        Some(&descriptor.customization),
    );
    result.schema_revision = descriptor.schema_revision;
    result.descriptor_error = descriptor.last_error.clone();
    ControlResponse::ok(
        id,
        serde_json::to_value(result).expect("serialize hmi.schema.get"),
    )
}

pub(super) fn handle_hmi_values_get(
    id: u64,
    params: Option<serde_json::Value>,
    state: &ControlState,
) -> ControlResponse {
    let params = match params {
        Some(value) => match serde_json::from_value::<HmiValuesParams>(value) {
            Ok(parsed) => parsed,
            Err(err) => return ControlResponse::error(id, format!("invalid params: {err}")),
        },
        None => HmiValuesParams { ids: None },
    };
    let metadata = match state.metadata.lock() {
        Ok(guard) => guard,
        Err(_) => return ControlResponse::error(id, "metadata unavailable".into()),
    };
    let snapshot = load_runtime_snapshot(state);
    let descriptor = hmi_descriptor_snapshot(state);
    let schema = crate::hmi::build_schema(
        state.resource_name.as_str(),
        &metadata,
        snapshot.as_ref(),
        true,
        Some(&descriptor.customization),
    );
    let result = crate::hmi::build_values(
        state.resource_name.as_str(),
        &metadata,
        snapshot.as_ref(),
        true,
        params.ids.as_deref(),
    );
    if let Ok(mut live) = state.hmi_live.lock() {
        crate::hmi::update_live_state(&mut live, &schema, &result);
    }
    ControlResponse::ok(
        id,
        serde_json::to_value(result).expect("serialize hmi.values.get"),
    )
}

pub(super) fn handle_hmi_trends_get(
    id: u64,
    params: Option<serde_json::Value>,
    state: &ControlState,
) -> ControlResponse {
    let params = match params {
        Some(value) => match serde_json::from_value::<HmiTrendsParams>(value) {
            Ok(parsed) => parsed,
            Err(err) => return ControlResponse::error(id, format!("invalid params: {err}")),
        },
        None => HmiTrendsParams::default(),
    };
    let metadata = match state.metadata.lock() {
        Ok(guard) => guard,
        Err(_) => return ControlResponse::error(id, "metadata unavailable".into()),
    };
    let snapshot = load_runtime_snapshot(state);
    let descriptor = hmi_descriptor_snapshot(state);
    let schema = crate::hmi::build_schema(
        state.resource_name.as_str(),
        &metadata,
        snapshot.as_ref(),
        true,
        Some(&descriptor.customization),
    );
    let values = crate::hmi::build_values(
        state.resource_name.as_str(),
        &metadata,
        snapshot.as_ref(),
        true,
        params.ids.as_deref(),
    );
    let result = match state.hmi_live.lock() {
        Ok(mut live) => {
            crate::hmi::update_live_state(&mut live, &schema, &values);
            crate::hmi::build_trends(
                &live,
                &schema,
                params.ids.as_deref(),
                params.duration_ms.unwrap_or(10 * 60 * 1_000),
                params.buckets.unwrap_or(120),
            )
        }
        Err(_) => return ControlResponse::error(id, "hmi state unavailable".into()),
    };
    ControlResponse::ok(
        id,
        serde_json::to_value(result).expect("serialize hmi.trends.get"),
    )
}

pub(super) fn handle_hmi_alarms_get(
    id: u64,
    params: Option<serde_json::Value>,
    state: &ControlState,
) -> ControlResponse {
    let params = match params {
        Some(value) => match serde_json::from_value::<HmiAlarmsParams>(value) {
            Ok(parsed) => parsed,
            Err(err) => return ControlResponse::error(id, format!("invalid params: {err}")),
        },
        None => HmiAlarmsParams::default(),
    };
    let metadata = match state.metadata.lock() {
        Ok(guard) => guard,
        Err(_) => return ControlResponse::error(id, "metadata unavailable".into()),
    };
    let snapshot = load_runtime_snapshot(state);
    let descriptor = hmi_descriptor_snapshot(state);
    let schema = crate::hmi::build_schema(
        state.resource_name.as_str(),
        &metadata,
        snapshot.as_ref(),
        true,
        Some(&descriptor.customization),
    );
    let values = crate::hmi::build_values(
        state.resource_name.as_str(),
        &metadata,
        snapshot.as_ref(),
        true,
        None,
    );
    let result = match state.hmi_live.lock() {
        Ok(mut live) => {
            crate::hmi::update_live_state(&mut live, &schema, &values);
            crate::hmi::build_alarm_view(&live, params.limit.unwrap_or(100))
        }
        Err(_) => return ControlResponse::error(id, "hmi state unavailable".into()),
    };
    ControlResponse::ok(
        id,
        serde_json::to_value(result).expect("serialize hmi.alarms.get"),
    )
}

