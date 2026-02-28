//! HMI websocket handshake/session helper functions.

#![allow(missing_docs)]

use super::*;

pub(super) fn websocket_accept_key(request: &tiny_http::Request) -> Result<String, &'static str> {
    let upgrade = header_value(request, "Upgrade").ok_or("missing Upgrade header")?;
    if !upgrade.eq_ignore_ascii_case("websocket") {
        return Err("invalid websocket upgrade");
    }
    let connection = header_value(request, "Connection").ok_or("missing Connection header")?;
    if !connection.to_ascii_lowercase().contains("upgrade") {
        return Err("invalid Connection upgrade");
    }
    let key = header_value(request, "Sec-WebSocket-Key").ok_or("missing Sec-WebSocket-Key")?;
    Ok(tungstenite::handshake::derive_accept_key(key.as_bytes()))
}

pub(super) fn spawn_hmi_websocket_session(
    stream: Box<dyn tiny_http::ReadWrite + Send>,
    control_state: Arc<ControlState>,
    request_token: Option<String>,
) {
    thread::spawn(move || {
        if let Err(err) = run_hmi_websocket_session(stream, control_state, request_token) {
            tracing::debug!("hmi websocket session closed: {err}");
        }
    });
}

fn run_hmi_websocket_session(
    stream: Box<dyn tiny_http::ReadWrite + Send>,
    control_state: Arc<ControlState>,
    request_token: Option<String>,
) -> Result<(), String> {
    use tungstenite::protocol::Role;

    let mut socket = tungstenite::protocol::WebSocket::from_raw_socket(stream, Role::Server, None);
    let mut request_id = 10_000_u64;
    let mut last_schema_revision = 0_u64;
    let mut widget_ids = Vec::new();
    let mut last_values = serde_json::Map::new();
    let mut last_alarm_payload: Option<serde_json::Value> = None;
    let mut next_schema_poll = Instant::now();
    let mut next_alarm_poll = Instant::now();

    if let Some(schema_result) = hmi_control_result(
        control_state.as_ref(),
        &mut request_id,
        "hmi.schema.get",
        None,
        request_token.as_deref(),
    ) {
        last_schema_revision = schema_result
            .get("schema_revision")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        widget_ids = hmi_widget_ids(&schema_result);
    }

    loop {
        let values_params = if widget_ids.is_empty() {
            None
        } else {
            Some(json!({ "ids": widget_ids }))
        };
        let values_result = hmi_control_result(
            control_state.as_ref(),
            &mut request_id,
            "hmi.values.get",
            values_params,
            request_token.as_deref(),
        )
        .ok_or_else(|| "hmi.values.get failed".to_string())?;

        if let Some(delta) = hmi_values_delta(&values_result, &mut last_values) {
            hmi_ws_send_json(
                &mut socket,
                &json!({
                    "type": "hmi.values.delta",
                    "result": delta,
                }),
            )?;
        }

        let now = Instant::now();
        if now >= next_schema_poll {
            next_schema_poll = now + HMI_WS_SCHEMA_POLL_INTERVAL;
            if let Some(schema_result) = hmi_control_result(
                control_state.as_ref(),
                &mut request_id,
                "hmi.schema.get",
                None,
                request_token.as_deref(),
            ) {
                let revision = schema_result
                    .get("schema_revision")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(last_schema_revision);
                if revision != last_schema_revision {
                    last_schema_revision = revision;
                    widget_ids = hmi_widget_ids(&schema_result);
                    hmi_ws_send_json(
                        &mut socket,
                        &json!({
                            "type": "hmi.schema.revision",
                            "result": { "schema_revision": revision }
                        }),
                    )?;
                }
            }
        }

        if now >= next_alarm_poll {
            next_alarm_poll = now + HMI_WS_ALARMS_POLL_INTERVAL;
            if let Some(alarms_result) = hmi_control_result(
                control_state.as_ref(),
                &mut request_id,
                "hmi.alarms.get",
                Some(json!({ "limit": 50_u64 })),
                request_token.as_deref(),
            ) {
                if last_alarm_payload.as_ref() != Some(&alarms_result) {
                    last_alarm_payload = Some(alarms_result.clone());
                    hmi_ws_send_json(
                        &mut socket,
                        &json!({
                            "type": "hmi.alarms.event",
                            "result": alarms_result
                        }),
                    )?;
                }
            }
        }

        std::thread::sleep(HMI_WS_VALUES_POLL_INTERVAL);
    }
}

fn hmi_control_result(
    control_state: &ControlState,
    request_id: &mut u64,
    request_type: &str,
    params: Option<serde_json::Value>,
    request_token: Option<&str>,
) -> Option<serde_json::Value> {
    *request_id = request_id.saturating_add(1);
    let mut payload = json!({
        "id": *request_id,
        "type": request_type,
    });
    if let Some(params) = params {
        payload["params"] = params;
    }
    let response = dispatch_control_request(payload, control_state, Some("web/ws"), request_token);
    let response = serde_json::to_value(response).ok()?;
    if !response
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    response.get("result").cloned()
}

fn hmi_widget_ids(schema: &serde_json::Value) -> Vec<String> {
    schema
        .get("widgets")
        .and_then(serde_json::Value::as_array)
        .map(|widgets| {
            widgets
                .iter()
                .filter_map(|widget| widget.get("id").and_then(serde_json::Value::as_str))
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn hmi_values_delta(
    values_result: &serde_json::Value,
    last_values: &mut serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let values = values_result.get("values")?.as_object()?;
    let mut delta = serde_json::Map::new();
    for (id, entry) in values {
        if last_values.get(id) != Some(entry) {
            delta.insert(id.clone(), entry.clone());
        }
    }
    last_values.retain(|id, _| values.contains_key(id));
    for (id, entry) in values {
        last_values.insert(id.clone(), entry.clone());
    }
    if delta.is_empty() {
        return None;
    }
    Some(json!({
        "connected": values_result
            .get("connected")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        "timestamp_ms": values_result.get("timestamp_ms").cloned().unwrap_or(serde_json::Value::Null),
        "values": delta,
    }))
}

fn hmi_ws_send_json<S>(
    socket: &mut tungstenite::protocol::WebSocket<S>,
    payload: &serde_json::Value,
) -> Result<(), String>
where
    S: std::io::Read + std::io::Write,
{
    socket
        .send(tungstenite::Message::Text(payload.to_string().into()))
        .map_err(|err| err.to_string())
}
