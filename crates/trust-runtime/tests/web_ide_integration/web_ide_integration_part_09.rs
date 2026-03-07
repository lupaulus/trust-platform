use super::*;

#[test]
fn unified_shell_entry_routes_redirect_to_ide() {
    let project = make_project("root-redirect");
    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    for path in ["", "/setup"] {
        let response = ureq::get(&format!("{base}{path}"))
            .config()
            .http_status_as_error(false)
            .max_redirects(0)
            .build()
            .call()
            .unwrap_or_else(|err| panic!("fetch {path} without redirect failed: {err}"));
        assert_eq!(
            response.status().as_u16(),
            302,
            "{path} must issue 302 redirect"
        );
        let location = response
            .headers()
            .get("location")
            .expect("redirect must have Location header");
        assert_eq!(location, "/ide", "{path} must redirect to /ide");
    }

    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn unified_shell_tab_deep_links_serve_ide_html() {
    let project = make_project("tab-deep-links");
    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    for path in [
        "/ide",
        "/ide/code",
        "/ide/hardware",
        "/ide/settings",
        "/ide/logs",
    ] {
        let shell = ureq::get(&format!("{base}{path}"))
            .call()
            .unwrap_or_else(|_| panic!("fetch {path} failed"))
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|_| panic!("read {path} failed"));
        assert!(
            shell.contains("id=\"ideTabNav\""),
            "{path} must contain tab navigation"
        );
        assert!(
            shell.contains("id=\"editorMount\""),
            "{path} must contain editor mount"
        );
    }

    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn unified_shell_header_uses_compact_toolbar_with_overflow_menu() {
    let project = make_project("compact-toolbar");
    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    let mut response = ureq::get(&format!("{base}/ide"))
        .call()
        .expect("fetch /ide");
    let body = response.body_mut().read_to_string().expect("read /ide");

    for id in [
        "id=\"openProjectBtn\"",
        "id=\"saveBtn\"",
        "id=\"buildBtn\"",
        "id=\"deployBtn\"",
        "id=\"moreActionsBtn\"",
        "id=\"moreActionsMenu\"",
        "id=\"quickOpenBtn\"",
        "id=\"cmdPaletteBtn\"",
        "id=\"saveAllBtn\"",
        "id=\"validateBtn\"",
        "id=\"testBtn\"",
    ] {
        assert!(body.contains(id), "toolbar html must contain {id}");
    }

    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn unified_shell_serves_all_ide_tab_modules() {
    let project = make_project("tab-modules");
    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    for module in [
        "ide-tabs.js",
        "ide-hardware.js",
        "ide-online.js",
        "ide-debug.js",
        "ide-settings.js",
        "ide-logs.js",
    ] {
        let mut response = ureq::get(&format!("{base}/ide/modules/{module}"))
            .call()
            .unwrap_or_else(|err| panic!("fetch {module} failed: {err}"));
        let body = response
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|_| panic!("read {module} failed"));
        assert!(
            body.len() > 100,
            "{module} must contain substantial content (got {} bytes)",
            body.len()
        );
    }

    let cytoscape = ureq::get(&format!("{base}/ide/modules/cytoscape.min.js"))
        .call()
        .expect("fetch cytoscape under IDE namespace")
        .body_mut()
        .read_to_string()
        .expect("read cytoscape");
    assert!(
        cytoscape.len() > 1000,
        "cytoscape.min.js must be a large library"
    );

    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn unified_shell_online_module_defaults_connection_to_same_origin_and_auto_connects() {
    let project = make_project("online-default-connect");
    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    let mut response = ureq::get(&format!("{base}/ide/modules/ide-online.js"))
        .call()
        .expect("fetch ide-online.js");
    let body = response
        .body_mut()
        .read_to_string()
        .expect("read ide-online.js");

    assert!(
        body.contains("function onlineDefaultConnectPort()"),
        "online module must expose derived default port helper"
    );
    assert!(
        body.contains("window.location.port"),
        "online module must derive connection defaults from current page origin"
    );
    assert!(
        body.contains("function onlineSeedConnectionDefaults()"),
        "online module must seed connection dialog defaults from current origin"
    );
    assert!(
        body.contains("if (!currentPort || currentPort === \"18080\")"),
        "online module must migrate stale legacy 18080 default to current origin port"
    );
    assert!(
        body.contains("void onlineConnect(withPort, null, { silent: true });"),
        "online module must auto-connect silently at startup in same-origin runtime mode"
    );

    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn unified_shell_tab_module_enforces_tab_aria_contract() {
    let project = make_project("tab-aria-contract");
    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    let mut response = ureq::get(&format!("{base}/ide/modules/ide-tabs.js"))
        .call()
        .expect("fetch ide-tabs.js");
    let body = response
        .body_mut()
        .read_to_string()
        .expect("read ide-tabs.js");

    assert!(
        body.contains("nav.setAttribute('role', 'tablist')"),
        "tab module must set tablist role"
    );
    assert!(
        body.contains("btn.setAttribute('role', 'tab')"),
        "tab module must set tab role on tab buttons"
    );
    assert!(
        body.contains("panel.setAttribute('role', 'tabpanel')"),
        "tab module must set tabpanel role on tab panels"
    );
    assert!(
        body.contains("btn.setAttribute('tabindex', isActive ? '0' : '-1')"),
        "tab module must keep keyboard tab order aligned with active tab"
    );
    assert!(
        body.contains("panel.classList.toggle('active', isActive)"),
        "tab module must keep active class on tab panels in sync with active tab"
    );

    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn unified_shell_base_css_enforces_hidden_attribute_contract() {
    let project = make_project("base-css-hidden-contract");
    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    let mut response = ureq::get(&format!("{base}/ide/base.css"))
        .call()
        .expect("fetch ide base.css");
    let body = response
        .body_mut()
        .read_to_string()
        .expect("read ide base.css");

    assert!(
        body.contains("[hidden] { display: none !important; }"),
        "base.css must enforce hidden attribute display contract"
    );

    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn unified_shell_settings_module_exposes_realtime_link_configuration_fields() {
    let project = make_project("settings-realtime-fields");
    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    let mut response = ureq::get(&format!("{base}/ide/modules/ide-settings.js"))
        .call()
        .expect("fetch ide-settings.js");
    let body = response
        .body_mut()
        .read_to_string()
        .expect("read ide-settings.js");

    assert!(
        body.contains("runtime_cloud.links.transports_json"),
        "settings module must expose runtime-cloud link transport JSON field"
    );
    assert!(
        body.contains("const SETTINGS_RUNTIME_LINK_TRANSPORTS"),
        "settings module must declare explicit runtime-cloud link transport allowlist"
    );
    for transport in [
        "realtime",
        "zenoh",
        "mesh",
        "mqtt",
        "modbus-tcp",
        "opcua",
        "discovery",
        "web",
    ] {
        assert!(
            body.contains(&format!("\"{transport}\"")),
            "settings module must include transport '{transport}' in allowlist/config text"
        );
    }
    assert!(
        body.contains("runtime_cloud.wan.allow_write_json"),
        "settings module must expose runtime-cloud WAN allow-write JSON field"
    );
    assert!(
        body.contains("function settingsTomlEncodeBindingValue"),
        "settings module must encode complex TOML binding values"
    );
    assert!(
        body.contains("function settingsTomlDecodeBindingRaw"),
        "settings module must decode complex TOML binding values"
    );
    assert!(
        body.contains("const SETTINGS_ONLINE_KEY_MAP"),
        "settings module must declare online key mapping for config.set"
    );
    assert!(
        body.contains("\"mesh.connect_json\": \"mesh.connect\""),
        "settings module must map mesh.connect_json to mesh.connect for online writes"
    );
    assert!(
        body.contains("\"mesh.subscribe_json\": \"mesh.subscribe\""),
        "settings module must map mesh.subscribe_json to mesh.subscribe for online writes"
    );
    assert!(
        body.contains("\"runtime_cloud.wan.allow_write_json\": \"runtime_cloud.wan.allow_write\""),
        "settings module must map runtime_cloud.wan.allow_write_json to backend key for online writes"
    );
    assert!(
        body.contains(
            "\"runtime_cloud.links.transports_json\": \"runtime_cloud.links.transports\""
        ),
        "settings module must map runtime_cloud.links.transports_json to backend key for online writes"
    );
    assert!(
        body.contains("\"discovery.interfaces_json\""),
        "settings module must include discovery.interfaces_json in online-capable keys"
    );
    assert!(
        body.contains("\"runtime_cloud.links.transports_json\""),
        "settings module must include runtime_cloud.links.transports_json in online-capable keys"
    );
    assert!(
        body.contains("\"opcua.username\""),
        "settings module must expose OPC UA username setting"
    );
    assert!(
        body.contains("\"opcua.password\""),
        "settings module must expose OPC UA password setting"
    );
    assert!(
        body.contains("\"observability.include_json\""),
        "settings module must expose observability include patterns JSON setting"
    );
    assert!(
        body.contains("\"observability.alerts_json\""),
        "settings module must expose observability alert rules JSON setting"
    );
    assert!(
        body.contains("\"io.mqtt.client_id\""),
        "settings module must expose MQTT client_id setting"
    );
    assert!(
        body.contains("\"io.mqtt.username\""),
        "settings module must expose MQTT username setting"
    );
    assert!(
        body.contains("\"io.mqtt.password\""),
        "settings module must expose MQTT password setting"
    );
    assert!(
        body.contains("\"io.mqtt.tls\""),
        "settings module must expose MQTT tls setting"
    );
    assert!(
        body.contains("\"io.gpio.backend\""),
        "settings module must expose GPIO backend setting"
    );
    assert!(
        body.contains("\"io.gpio.inputs_json\""),
        "settings module must expose GPIO inputs JSON setting"
    );
    assert!(
        body.contains("\"io.gpio.outputs_json\""),
        "settings module must expose GPIO outputs JSON setting"
    );
    assert!(
        body.contains("\"io.ethercat.adapter\""),
        "settings module must expose EtherCAT adapter setting"
    );
    assert!(
        body.contains("\"io.ethercat.modules_json\""),
        "settings module must expose EtherCAT modules JSON setting"
    );
    assert!(
        body.contains("\"io.ethercat.mock_fail_read\""),
        "settings module must expose EtherCAT mock_fail_read setting"
    );
    assert!(
        body.contains("\"io.ethercat.mock_fail_write\""),
        "settings module must expose EtherCAT mock_fail_write setting"
    );
    assert!(
        body.contains("\"io.simulated.inputs\""),
        "settings module must expose simulated input count setting"
    );
    assert!(
        body.contains("\"io.simulated.outputs\""),
        "settings module must expose simulated output count setting"
    );
    assert!(
        body.contains("\"io.simulated.scan_ms\""),
        "settings module must expose simulated scan interval setting"
    );
    assert!(
        body.contains("\"io.safe_state_json\""),
        "settings module must expose I/O safe state JSON setting"
    );
    assert!(
        body.contains("\"simulation.enabled\""),
        "settings module must expose simulation enabled setting"
    );
    assert!(
        body.contains("\"simulation.seed\""),
        "settings module must expose simulation seed setting"
    );
    assert!(
        body.contains("\"simulation.time_scale\""),
        "settings module must expose simulation time-scale setting"
    );
    assert!(
        body.contains("SETTINGS_SIMULATION_BINDINGS"),
        "settings module must declare simulation.toml bindings"
    );
    assert!(
        body.contains("settingsLoadSimulationConfigSnapshot"),
        "settings module must load simulation.toml in standalone mode"
    );
    assert!(
        body.contains("settingsPersistSimulationValue"),
        "settings module must persist simulation.toml setting edits"
    );
    assert!(
        body.contains("/api/ide/fs/create"),
        "settings module must create simulation.toml through fs/create when the file does not exist"
    );
    assert!(
        body.contains("\"resource.tasks_json\""),
        "settings module must expose resource task override JSON setting"
    );
    assert!(
        body.contains("json-array"),
        "settings module must support array JSON coercion for IO driver params"
    );
    assert!(
        body.contains("\"resource.tasks\": \"resource.tasks_json\""),
        "settings module must map resource.tasks online snapshot key to JSON-backed field key"
    );
    assert!(
        body.contains("\"observability.include\": \"observability.include_json\""),
        "settings module must map observability.include to JSON-backed field key"
    );
    assert!(
        body.contains("\"observability.alerts\": \"observability.alerts_json\""),
        "settings module must map observability.alerts to JSON-backed field key"
    );
    assert!(
        body.contains("observability-alert-rules-json"),
        "settings module must encode/decode observability alert rules from runtime.toml"
    );
    assert!(
        body.contains("resource-task-rules-json"),
        "settings module must encode/decode resource tasks from runtime.toml"
    );
    assert!(
        body.contains("function settingsParseSafeStateJsonOrThrow"),
        "settings module must validate safe_state JSON edits before save"
    );
    assert!(
        body.contains("function settingsEnqueueSave"),
        "settings module must serialize setting saves to avoid lost concurrent writes"
    );
    assert!(
        body.contains("function settingsNotifyRuntimeConfigUpdated"),
        "settings module must notify other tabs when runtime.toml values change"
    );
    assert!(
        body.contains("ide-runtime-config-updated"),
        "settings module must dispatch ide-runtime-config-updated events"
    );
    assert!(
        body.contains("Runtime State (Read-only)"),
        "settings module advanced panel must expose runtime read-only state"
    );
    assert!(
        body.contains("settingsImportBtn"),
        "settings advanced panel must expose import action"
    );
    assert!(
        body.contains("id: \"all\""),
        "settings module must expose an All Settings category"
    );
    assert!(
        body.contains("settings-category-search"),
        "settings module must expose settings filter/search control"
    );
    assert!(
        body.contains("settingsGroupsForAllCategory"),
        "settings module must support rendering all categories in one view"
    );
    assert!(
        body.contains("settingsLoadRuntimeTargets"),
        "settings module must load standalone runtime targets for per-runtime editing"
    );
    assert!(
        body.contains("/api/config-ui/runtime/lifecycle"),
        "settings module must query config-ui runtime lifecycle in standalone mode"
    );
    assert!(
        body.contains("settingsRenderRuntimeScopeBar"),
        "settings module must render runtime scope selector in standalone mode"
    );
    assert!(
        body.contains("const SETTINGS_RUNTIME_SELECTION_EVENT"),
        "settings module must share runtime selection event contract with hardware tab"
    );
    assert!(
        body.contains("/api/config-ui/io/config?runtime_id="),
        "settings module must read io.toml via config-ui scoped runtime endpoint"
    );
    assert!(
        body.contains("/api/config-ui/io/config"),
        "settings module must persist io.toml via config-ui scoped runtime endpoint"
    );
    assert!(
        body.contains("settings-filter-summary"),
        "settings module must expose active-filter summary to avoid hidden fields confusion"
    );
    assert!(
        body.contains("data-settings-clear-filter"),
        "settings module must provide one-click filter clear action"
    );

    let online_key_block = body
        .find("const SETTINGS_ONLINE_KEYS = new Set([")
        .and_then(|start| {
            body[start..]
                .find("]);")
                .map(|end| &body[start..start + end])
        })
        .expect("settings module must define SETTINGS_ONLINE_KEYS");
    assert!(
        !online_key_block.contains("\"opcua.username\""),
        "online key set must not include unsupported opcua.username runtime-control write"
    );
    assert!(
        !online_key_block.contains("\"opcua.password\""),
        "online key set must not include unsupported opcua.password runtime-control write"
    );
    assert!(
        !online_key_block.contains("\"observability.include_json\""),
        "online key set must not include unsupported observability.include_json runtime-control write"
    );
    assert!(
        !online_key_block.contains("\"observability.alerts_json\""),
        "online key set must not include unsupported observability.alerts_json runtime-control write"
    );

    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn unified_shell_hardware_module_exposes_runtime_cloud_link_transport_projection() {
    let project = make_project("hardware-cloud-links");
    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    let mut response = ureq::get(&format!("{base}/ide/modules/ide-hardware.js"))
        .call()
        .expect("fetch ide-hardware.js");
    let body = response
        .body_mut()
        .read_to_string()
        .expect("read ide-hardware.js");

    assert!(
        body.contains("runtime.cloud.links"),
        "hardware module must parse runtime.cloud.links section"
    );
    assert!(
        body.contains("const HW_RUNTIME_LINK_TRANSPORTS"),
        "hardware module must define runtime link transport selector options"
    );
    assert!(
        body.contains("const HW_RUNTIME_LINK_TRANSPORT_NOTES"),
        "hardware module must define runtime link transport descriptions for picker UX"
    );
    for transport in [
        "realtime",
        "zenoh",
        "mesh",
        "mqtt",
        "modbus-tcp",
        "opcua",
        "discovery",
        "web",
    ] {
        assert!(
            body.contains(&format!("id: \"{transport}\"")),
            "hardware module must expose runtime link transport '{transport}'"
        );
    }
    assert!(
        body.contains("Cloud Link Transports"),
        "hardware module must expose cloud link transport card label"
    );
    assert!(
        body.contains("hwParseCloudLinkTransportSection"),
        "hardware module must parse cloud link transport rules from runtime.toml"
    );
    assert!(
        body.contains("hwParseCloudWanAllowWriteSection"),
        "hardware module must parse runtime.cloud.wan allow_write rules from runtime.toml"
    );
    assert!(
        body.contains("Cloud WAN Access"),
        "hardware module must expose cloud WAN access card label"
    );
    assert!(
        body.contains("ide-runtime-config-updated"),
        "hardware module must react to runtime.toml updates from settings"
    );
    assert!(
        body.contains("client_id"),
        "hardware module must expose MQTT client_id parameter"
    );
    assert!(
        body.contains("allow_insecure_remote"),
        "hardware module must expose MQTT allow_insecure_remote parameter"
    );
    assert!(
        body.contains("inputs_json"),
        "hardware module must expose GPIO inputs JSON parameter"
    );
    assert!(
        body.contains("modules_json"),
        "hardware module must expose EtherCAT modules JSON parameter"
    );
    assert!(
        body.contains("mock_fail_read"),
        "hardware module must expose EtherCAT mock_fail_read parameter"
    );
    assert!(
        body.contains("/api/io/mqtt-test"),
        "hardware module must expose MQTT connection test endpoint call"
    );
    assert!(
        body.contains("Connection test is currently available for Modbus TCP and MQTT."),
        "hardware module should clearly communicate supported test drivers"
    );
    assert!(
        body.contains("runtime_cloud.links.transports_json"),
        "hardware module must deep-link cloud links nodes to settings transport rules"
    );
    assert!(
        body.contains("type: \"opcua\"")
            && body.contains("label: \"OPC UA\"")
            && body.contains("driver: \"opcua\""),
        "hardware palette OPC UA entry must use the OPC UA driver binding"
    );
    assert!(
        body.contains("if (name === \"simulated\") return \"io.simulated.inputs\""),
        "hardware module must deep-link simulated modules to simulated I/O settings"
    );
    assert!(
        body.contains("if (name === \"loopback\") return \"io.simulated.inputs\""),
        "hardware module must deep-link loopback modules to simulated I/O settings"
    );
    assert!(
        body.contains("data-hw-driver-settings"),
        "hardware driver cards must expose configure action deep-links into Settings"
    );
    assert!(
        body.contains("btn.dataset.hwDriverSettings"),
        "hardware driver card configure action must route by settings key"
    );
    assert!(
        body.contains("data-hw-driver-settings-category"),
        "hardware driver card configure action must include settings category routing metadata"
    );
    assert!(
        body.contains("function hwSettingsActionsForDriver"),
        "hardware module must provide per-driver settings action expansion"
    );
    assert!(
        body.contains("function hwSettingsCategoryForKey"),
        "hardware module must map settings keys to target categories"
    );
    assert!(
        body.contains("const HW_RUNTIME_SELECTION_EVENT = \"ide-runtime-selection-changed\""),
        "hardware module must share runtime selection event contract with settings tab"
    );
    assert!(
        body.contains("function hwBroadcastActiveRuntimeSelection"),
        "hardware module must broadcast runtime scope changes for settings synchronization"
    );
    assert!(
        body.contains("hwTransportModal"),
        "hardware module must integrate runtime link transport modal flow"
    );
    assert!(
        body.contains("data-hw-transport-option"),
        "hardware module must render clickable runtime link transport options"
    );
    assert!(
        body.contains("hwLinkFlowHint"),
        "hardware module must drive runtime link creation hint banner state"
    );
    assert!(
        body.contains("Runtime Control"),
        "hardware module must project runtime.control section as a hardware/runtime card"
    );
    assert!(
        body.contains("Deploy Security"),
        "hardware module must project runtime.deploy section as a hardware/runtime card"
    );
    assert!(
        body.contains("Observability"),
        "hardware module must project runtime.observability section as a hardware/runtime card"
    );
    assert!(
        body.contains("\"io.mqtt.topic_in\""),
        "hardware module must deep-link MQTT topic settings from driver cards"
    );
    assert!(
        body.contains("\"io.modbus.unit_id\""),
        "hardware module must deep-link Modbus unit ID settings from driver cards"
    );
    assert!(
        body.contains("\"tls.mode\""),
        "hardware module must deep-link TLS settings from runtime cards"
    );
    assert!(
        body.contains("\"control.debug_enabled\""),
        "hardware module must deep-link debug settings from runtime control card"
    );
    assert!(
        body.contains("el.hwCtxRuntimeCommSettingsBtn.classList.add(\"ide-hidden\")"),
        "endpoint context menu must hide duplicate protocol settings action"
    );
    assert!(
        body.contains("if (meta.type !== \"runtime\") return;"),
        "communication-settings context action must be runtime-only"
    );

    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn unified_shell_exposes_mqtt_connectivity_probe_api() {
    let project = make_project("mqtt-probe-api");
    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    let (missing_status, missing_json) = request_json(
        "POST",
        &format!("{base}/api/io/mqtt-test"),
        Some(json!({ "broker": "" })),
        &[],
    );
    assert_eq!(missing_status, 400, "empty broker must be rejected");
    assert!(
        !missing_json
            .get("ok")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        "empty broker must return ok=false"
    );

    let (probe_status, probe_json) = request_json(
        "POST",
        &format!("{base}/api/io/mqtt-test"),
        Some(json!({ "broker": "not-a-valid-host-@@@:1883", "timeout_ms": 30 })),
        &[],
    );
    assert_eq!(
        probe_status, 200,
        "MQTT probe endpoint must return a structured probe result"
    );
    assert!(
        !probe_json
            .get("ok")
            .and_then(|value| value.as_bool())
            .unwrap_or(true),
        "invalid host must return ok=false"
    );
    assert!(
        probe_json
            .get("error")
            .and_then(|value| value.as_str())
            .is_some(),
        "failed probe must include an error string"
    );

    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn unified_shell_serves_composed_ide_modules_required_for_bootstrap() {
    let project = make_project("composed-ide-modules");
    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    let checks: [(&str, &[&str]); 5] = [
        (
            "ide-editor-language.js",
            &[
                "function configureMonacoLanguageSupport()",
                "function buildFallbackHover",
            ],
        ),
        (
            "ide-editor-pane.js",
            &["function createEditor(", "function syncSecondaryEditor()"],
        ),
        (
            "ide-workspace-tree.js",
            &[
                "async function refreshProjectSelection()",
                "async function bootstrapFiles()",
                "async function doOpenProject",
            ],
        ),
        (
            "ide-observability.js",
            &[
                "async function loadPresenceModel()",
                "function refreshMultiTabCollision()",
            ],
        ),
        (
            "ide-commands.js",
            &[
                "async function workspaceSearchFlow()",
                "function ideConfirm(",
            ],
        ),
    ];

    for (module, required_symbols) in checks {
        let mut response = ureq::get(&format!("{base}/ide/modules/{module}"))
            .call()
            .unwrap_or_else(|err| panic!("fetch {module} failed: {err}"));
        let body = response
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|_| panic!("read {module} failed"));
        assert!(body.len() > 500, "{module} must have non-trivial content");
        for symbol in required_symbols {
            assert!(
                body.contains(symbol),
                "{module} must contain symbol `{symbol}` for runtime bootstrap"
            );
        }
    }

    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn unified_shell_ide_client_supports_wrapped_and_direct_api_payloads() {
    let project = make_project("ide-client-api-payloads");
    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    let mut ide_js_response = ureq::get(&format!("{base}/ide/ide.js"))
        .call()
        .expect("fetch ide.js");
    let ide_js = ide_js_response
        .body_mut()
        .read_to_string()
        .expect("read ide.js");

    assert!(
        ide_js.contains("Object.prototype.hasOwnProperty.call(payload, \"result\")"),
        "ide.js API client must detect wrapped payloads"
    );
    assert!(
        ide_js.contains("return payload;"),
        "ide.js API client must support direct payloads without a result envelope"
    );

    let mode_payload = ureq::get(&format!("{base}/api/ui/mode"))
        .call()
        .expect("fetch /api/ui/mode")
        .body_mut()
        .read_to_string()
        .expect("read /api/ui/mode payload");
    let mode_value: serde_json::Value =
        serde_json::from_str(&mode_payload).expect("parse /api/ui/mode payload");
    assert!(
        mode_value
            .get("mode")
            .and_then(|value| value.as_str())
            .is_some(),
        "/api/ui/mode must expose mode as a top-level string field"
    );

    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn unified_shell_html_contract_contains_tab_panels_and_status_bar() {
    let project = make_project("shell-contract");
    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    let shell = ureq::get(&format!("{base}/ide"))
        .call()
        .expect("fetch ide shell")
        .body_mut()
        .read_to_string()
        .expect("read ide shell");

    // Tab navigation
    assert!(
        shell.contains("id=\"ideTabNav\""),
        "shell must have tab navigation"
    );

    // Tab panels
    for tab in ["code", "hardware", "settings", "logs"] {
        assert!(
            shell.contains(&format!("data-tab=\"{tab}\"")),
            "shell must have {tab} tab panel"
        );
    }

    // Hardware tab elements
    assert!(
        shell.contains("id=\"hwCanvas\""),
        "shell must have hardware canvas"
    );
    assert!(
        shell.contains("id=\"hwAddressTable\""),
        "shell must have hardware address table"
    );
    assert!(
        !shell.contains("id=\"hwSummary\""),
        "shell must not expose removed hardware summary cards container"
    );
    assert!(
        shell.contains("id=\"hwDriverCards\""),
        "shell must have hardware driver cards container"
    );
    assert!(
        shell.contains("id=\"hwPropertyPanel\""),
        "shell must have hardware property panel"
    );
    assert!(
        shell.contains("id=\"hwLinkFlowHint\""),
        "shell must have runtime link creation guidance banner"
    );
    assert!(
        !shell.contains("id=\"hwCanvasToolbar\""),
        "shell must not expose removed hardware canvas toolbar controls"
    );
    assert!(
        !shell.contains("id=\"hwFabricFilterSelect\""),
        "shell must not expose the removed hardware communication filter control"
    );
    assert!(
        !shell.contains("id=\"hwRuntimeLinkStudio\""),
        "shell must not expose the removed runtime link studio panel"
    );
    assert!(
        !shell.contains("id=\"hwTransportPills\""),
        "shell must not expose removed runtime transport summary pills"
    );
    assert!(
        shell.contains("id=\"hwNodeContextMenu\""),
        "shell must expose hardware runtime node context menu"
    );
    assert!(
        shell.contains("id=\"hwEdgeContextMenu\""),
        "shell must expose hardware edge context menu"
    );
    assert!(
        shell.contains("id=\"hwCtxCreateLinkFromEdgeBtn\""),
        "shell must expose context action to add runtime links from existing links"
    );
    assert!(
        shell.contains("id=\"hwCtxOpenLinkSettingsBtn\""),
        "shell must expose context action to jump from link to settings"
    );
    assert!(
        !shell.contains("id=\"hwLegendToggleBtn\""),
        "shell must not expose removed legend toggle control"
    );
    assert!(
        !shell.contains("id=\"hwLegend\""),
        "shell must not expose removed hardware communication legend"
    );
    assert!(
        !shell.contains("id=\"hwToggleInspectorBtn\""),
        "shell must not expose removed inspector toggle control"
    );
    assert!(
        !shell.contains("id=\"hwToggleDriversBtn\""),
        "shell must not expose removed drivers toggle control"
    );
    assert!(
        !shell.contains("id=\"hwCenterCanvasBtn\""),
        "shell must not expose removed canvas center control"
    );
    assert!(
        !shell.contains("id=\"hwFullscreenBtn\""),
        "shell must not expose removed canvas fullscreen control"
    );
    assert!(
        shell.contains("id=\"hwDriversPanel\""),
        "shell must expose collapsible hardware driver panel"
    );
    assert!(
        shell.contains("id=\"hardwarePalette\""),
        "shell must have hardware palette"
    );
    assert!(
        shell.contains("id=\"hwTransportModal\""),
        "shell must have runtime link transport picker modal"
    );
    assert!(
        shell.contains("id=\"hwTransportOptions\""),
        "shell must have runtime link transport picker options container"
    );
    for removed_copy in [
        "Modules",
        "I/O Points",
        "Active Drivers",
        "Address Health",
        "Fabric",
        "Address Map",
        "Fit",
        "Center",
        "Inspector",
        "Fullscreen",
        "Reload",
        "Active links",
        "Legend",
    ] {
        assert!(
            !shell.contains(removed_copy),
            "shell must not render removed hardware chrome text `{removed_copy}`"
        );
    }

    // Connection dialog
    assert!(
        shell.contains("id=\"connectionDialog\""),
        "shell must have connection dialog"
    );

    // Debug toolbar and panels
    assert!(
        shell.contains("id=\"debugToolbar\""),
        "shell must have debug toolbar"
    );
    assert!(
        shell.contains("id=\"debugVariablesPanel\""),
        "shell must have debug variables panel"
    );
    assert!(
        shell.contains("id=\"debugCallStackPanel\""),
        "shell must have debug call stack panel"
    );
    assert!(
        shell.contains("id=\"debugWatchPanel\""),
        "shell must have debug watch panel"
    );

    // Settings workspace
    assert!(
        shell.contains("id=\"settingsCategories\""),
        "shell must have settings categories sidebar"
    );
    assert!(
        shell.contains("id=\"settingsFormPanel\""),
        "shell must have settings form panel"
    );

    // Logs workspace
    assert!(
        shell.contains("id=\"logsFilterBar\""),
        "shell must have logs filter bar"
    );
    assert!(
        shell.contains("id=\"logsTablePanel\""),
        "shell must have logs table panel"
    );

    // Status bar
    assert!(
        shell.contains("id=\"syncBadge\""),
        "shell must have sync badge in status bar"
    );
    assert!(
        shell.contains("id=\"statusLatency\""),
        "shell must have latency label in status bar"
    );

    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn unified_shell_removes_legacy_fleet_routes() {
    let project = make_project("fleet-compat");
    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    for path in [
        "/fleet",
        "/app.js",
        "/runtime-cloud-utils.js",
        "/styles.css",
        "/modules/fleet.js",
    ] {
        let response = ureq::get(&format!("{base}{path}"))
            .config()
            .http_status_as_error(false)
            .build()
            .call()
            .unwrap_or_else(|err| panic!("fetch {path} failed: {err}"));
        assert_eq!(response.status().as_u16(), 404, "{path} must return 404");
    }

    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn unified_shell_ide_io_config_route_tracks_active_workspace() {
    let project_a = make_project("ide-io-config-a");
    std::fs::write(
        project_a.join("io.toml"),
        "[io]\ndriver = \"simulated\"\n\n[io.params]\n\n[[io.safe_state]]\naddress = \"%QX0.0\"\nvalue = \"FALSE\"\n",
    )
    .expect("write project A io.toml");

    let project_b = make_project("ide-io-config-b");
    std::fs::write(
        project_b.join("io.toml"),
        "[io]\ndriver = \"mqtt\"\n\n[io.params]\nbroker = \"127.0.0.1:1883\"\ntopic_in = \"trust/in\"\ntopic_out = \"trust/out\"\n\n[[io.safe_state]]\naddress = \"%QX0.0\"\nvalue = \"FALSE\"\n",
    )
    .expect("write project B io.toml");

    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project_a.clone(), WebAuthMode::Local);

    let (_, session) = request_json(
        "POST",
        &format!("{base}/api/ide/session"),
        Some(json!({ "role": "viewer" })),
        &[],
    );
    let token = session["result"]["token"]
        .as_str()
        .expect("session token should exist")
        .to_string();
    let headers = [("X-Trust-Ide-Session", token.as_str())];

    let (open_status, open_body) = request_json(
        "POST",
        &format!("{base}/api/ide/project/open"),
        Some(json!({ "path": project_b.to_string_lossy() })),
        &headers,
    );
    assert_eq!(open_status, 200, "project open should succeed: {open_body}");

    let (cfg_status, cfg_body) =
        request_json("GET", &format!("{base}/api/ide/io/config"), None, &headers);
    assert_eq!(
        cfg_status, 200,
        "io config route should succeed: {cfg_body}"
    );
    assert_eq!(
        cfg_body["result"]["driver"].as_str(),
        Some("mqtt"),
        "active workspace io.toml must drive IDE io config payload"
    );
    let drivers = cfg_body["result"]["drivers"]
        .as_array()
        .expect("drivers list should be array");
    assert!(
        drivers
            .iter()
            .any(|entry| entry["name"].as_str() == Some("mqtt")),
        "drivers payload should include mqtt from project B"
    );

    let _ = std::fs::remove_dir_all(project_a);
    let _ = std::fs::remove_dir_all(project_b);
}

#[test]
fn unified_shell_control_proxy_supports_runtime_status_forwarding() {
    let local_project = make_project("control-proxy-local");
    let remote_project = make_project("control-proxy-remote");

    let local_state = control_state(source_fixture(), ControlMode::Debug, None);
    let remote_state = control_state(source_fixture(), ControlMode::Debug, None);
    let local_base = start_test_server(local_state, local_project.clone(), WebAuthMode::Local);
    let remote_base = start_test_server(remote_state, remote_project.clone(), WebAuthMode::Local);

    let (status, body) = request_json(
        "POST",
        &format!("{local_base}/api/control/proxy"),
        Some(json!({
            "target": remote_base,
            "control_request": {
                "id": 1,
                "type": "status"
            }
        })),
        &[],
    );
    assert_eq!(status, 200, "proxy status call should succeed: {body}");
    assert_eq!(body["ok"], json!(true));
    assert!(
        body["result"].is_object(),
        "proxied status must include result payload"
    );

    let _ = std::fs::remove_dir_all(local_project);
    let _ = std::fs::remove_dir_all(remote_project);
}

#[test]
fn unified_shell_ide_io_config_post_writes_active_workspace_io_file() {
    let project = make_project("ide-io-config-post");
    std::fs::write(
        project.join("io.toml"),
        "[io]\ndriver = \"simulated\"\n\n[io.params]\n\n[[io.safe_state]]\naddress = \"%QX0.0\"\nvalue = \"FALSE\"\n",
    )
    .expect("write initial io.toml");

    let state = control_state(source_fixture(), ControlMode::Debug, None);
    let base = start_test_server(state, project.clone(), WebAuthMode::Local);

    let (_, session) = request_json(
        "POST",
        &format!("{base}/api/ide/session"),
        Some(json!({ "role": "editor" })),
        &[],
    );
    let token = session["result"]["token"]
        .as_str()
        .expect("session token should exist")
        .to_string();
    let headers = [("X-Trust-Ide-Session", token.as_str())];

    let (save_status, save_body) = request_json(
        "POST",
        &format!("{base}/api/ide/io/config"),
        Some(json!({
            "drivers": [
                {
                    "name": "mqtt",
                    "params": {
                        "broker": "10.0.0.10:1883",
                        "topic_in": "factory/in",
                        "topic_out": "factory/out"
                    }
                }
            ],
            "safe_state": [
                {
                    "address": "%QX0.0",
                    "value": "FALSE"
                }
            ],
            "use_system_io": false
        })),
        &headers,
    );
    assert_eq!(
        save_status, 200,
        "io config save must succeed through /api/ide/io/config: {save_body}"
    );
    assert_eq!(save_body["ok"], json!(true));

    let io_text = std::fs::read_to_string(project.join("io.toml")).expect("read io.toml");
    assert!(
        io_text.contains("driver = \"mqtt\""),
        "saved io.toml should contain mqtt driver: {io_text}"
    );
    assert!(
        io_text.contains("broker = \"10.0.0.10:1883\""),
        "saved io.toml should contain updated broker: {io_text}"
    );
    assert!(
        io_text.contains("topic_in = \"factory/in\""),
        "saved io.toml should contain updated topic_in: {io_text}"
    );
    assert!(
        io_text.contains("topic_out = \"factory/out\""),
        "saved io.toml should contain updated topic_out: {io_text}"
    );

    let _ = std::fs::remove_dir_all(project);
}
