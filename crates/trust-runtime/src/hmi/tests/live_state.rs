fn synthetic_schema(min: Option<f64>, max: Option<f64>) -> HmiSchemaResult {
    synthetic_schema_with_deadband(min, max, None)
}

fn synthetic_schema_with_deadband(
    min: Option<f64>,
    max: Option<f64>,
    deadband: Option<f64>,
) -> HmiSchemaResult {
    HmiSchemaResult {
        version: HMI_SCHEMA_VERSION,
        schema_revision: 0,
        mode: "read_only",
        read_only: true,
        resource: "RESOURCE".to_string(),
        generated_at_ms: 0,
        descriptor_error: None,
        theme: resolve_theme(None),
        responsive: resolve_responsive(None),
        export: resolve_export(None),
        pages: vec![HmiPageSchema {
            id: DEFAULT_PAGE_ID.to_string(),
            title: "Overview".to_string(),
            order: 0,
            kind: "dashboard".to_string(),
            icon: None,
            duration_ms: None,
            svg: None,
            hidden: false,
            signals: Vec::new(),
            sections: Vec::new(),
            bindings: Vec::new(),
        }],
        widgets: vec![HmiWidgetSchema {
            id: "resource/RESOURCE/program/Main/field/speed".to_string(),
            path: "Main.speed".to_string(),
            label: "Speed".to_string(),
            data_type: "REAL".to_string(),
            access: "read",
            writable: false,
            widget: "value".to_string(),
            source: "program:Main".to_string(),
            page: DEFAULT_PAGE_ID.to_string(),
            group: DEFAULT_GROUP_NAME.to_string(),
            order: 0,
            zones: Vec::new(),
            on_color: None,
            off_color: None,
            section_title: None,
            widget_span: None,
            alarm_deadband: deadband,
            inferred_interface: false,
            detail_page: None,
            unit: Some("rpm".to_string()),
            min,
            max,
        }],
    }
}

fn synthetic_values(value: f64, ts_ms: u128) -> HmiValuesResult {
    let mut values = IndexMap::new();
    values.insert(
        "resource/RESOURCE/program/Main/field/speed".to_string(),
        HmiValueRecord {
            v: json!(value),
            q: "good",
            ts_ms,
        },
    );
    HmiValuesResult {
        connected: true,
        timestamp_ms: ts_ms,
        source_time_ns: None,
        freshness_ms: Some(0),
        values,
    }
}

#[test]
fn trend_downsample_preserves_bounds_and_window() {
    let schema = synthetic_schema(None, None);
    let mut live = HmiLiveState::default();
    for idx in 0..60 {
        update_live_state(
            &mut live,
            &schema,
            &synthetic_values(idx as f64, idx * 1_000),
        );
    }

    let trend = build_trends(&live, &schema, None, 60_000, 12);
    assert_eq!(trend.series.len(), 1);
    let points = &trend.series[0].points;
    assert!(points.len() <= 12);
    assert!(points.iter().all(|point| point.min <= point.value));
    assert!(points.iter().all(|point| point.max >= point.value));
    assert!(points.iter().all(|point| point.samples >= 1));

    let short_window = build_trends(&live, &schema, None, 10_000, 12);
    assert_eq!(short_window.series.len(), 1);
    let last_ts = short_window.series[0]
        .points
        .last()
        .map(|point| point.ts_ms)
        .unwrap_or_default();
    assert!(last_ts >= 50_000);
}

#[test]
fn alarm_state_machine_covers_raise_ack_clear_history() {
    let schema = synthetic_schema(Some(0.0), Some(100.0));
    let mut live = HmiLiveState::default();

    update_live_state(&mut live, &schema, &synthetic_values(80.0, 1_000));
    let baseline = build_alarm_view(&live, 10);
    assert!(baseline.active.is_empty());

    update_live_state(&mut live, &schema, &synthetic_values(120.0, 2_000));
    let raised = build_alarm_view(&live, 10);
    assert_eq!(raised.active.len(), 1);
    assert_eq!(raised.active[0].state, "raised");
    assert_eq!(
        raised.history.first().map(|event| event.event),
        Some("raised")
    );

    let alarm_id = raised.active[0].id.clone();
    acknowledge_alarm(&mut live, alarm_id.as_str(), 2_500).expect("acknowledge alarm");
    let acknowledged = build_alarm_view(&live, 10);
    assert_eq!(acknowledged.active[0].state, "acknowledged");
    assert_eq!(
        acknowledged.history.first().map(|event| event.event),
        Some("acknowledged")
    );

    update_live_state(&mut live, &schema, &synthetic_values(95.0, 3_000));
    let cleared = build_alarm_view(&live, 10);
    assert!(cleared.active.is_empty());
    let history_events = cleared
        .history
        .iter()
        .map(|event| event.event)
        .collect::<Vec<_>>();
    assert!(history_events.contains(&"raised"));
    assert!(history_events.contains(&"acknowledged"));
    assert!(history_events.contains(&"cleared"));
}

#[test]
fn alarm_deadband_requires_reentry_window_before_clear() {
    let schema = synthetic_schema_with_deadband(None, Some(100.0), Some(2.0));
    let mut live = HmiLiveState::default();

    update_live_state(&mut live, &schema, &synthetic_values(101.0, 1_000));
    let raised = build_alarm_view(&live, 10);
    assert_eq!(raised.active.len(), 1);

    // Still active because value is inside threshold but not outside deadband clear window.
    update_live_state(&mut live, &schema, &synthetic_values(99.0, 2_000));
    let still_active = build_alarm_view(&live, 10);
    assert_eq!(still_active.active.len(), 1);

    // Clears once value re-enters clear window (<= max-deadband).
    update_live_state(&mut live, &schema, &synthetic_values(97.5, 3_000));
    let cleared = build_alarm_view(&live, 10);
    assert!(cleared.active.is_empty());
}
