impl RuntimeToml {
    pub(crate) fn into_config(self) -> Result<RuntimeConfig, RuntimeError> {
        validate_base_sections(&self)?;

        let bundle_version = self.bundle.version;
        let resource_name = SmolStr::new(self.resource.name);
        let cycle_interval = Duration::from_millis(self.resource.cycle_interval_ms as i64);
        let tasks = parse_tasks(self.resource.tasks)?;

        let RuntimeSection {
            control,
            log,
            retain,
            watchdog,
            fault,
            web,
            tls,
            deploy,
            discovery,
            mesh,
            cloud,
            observability,
            opcua,
        } = self.runtime;

        let parsed_control = parse_control(&control)?;
        let parsed_retain_mode = parse_retain_mode(&retain)?;
        let watchdog_action = WatchdogAction::parse(&watchdog.action)?;
        let fault_policy = FaultPolicy::parse(&fault.policy)?;
        let parsed_tls = parse_tls_section(tls)?;
        let parsed_web = parse_web_section(
            web,
            parsed_control.auth_token.as_ref(),
            parsed_tls.mode,
            parsed_tls.require_remote,
        )?;
        let parsed_deploy = parse_deploy_section(deploy)?;
        let parsed_discovery = parse_discovery_section(discovery)?;
        let parsed_mesh = parse_mesh_section(mesh, parsed_tls.mode, parsed_tls.require_remote)?;
        let parsed_cloud = parse_runtime_cloud_section(cloud)?;
        let observability = parse_observability_section(observability)?;
        let opcua = parse_opcua_section(opcua)?;

        Ok(RuntimeConfig {
            bundle_version,
            resource_name,
            cycle_interval,
            control_endpoint: SmolStr::new(control.endpoint),
            control_auth_token: parsed_control.auth_token,
            control_debug_enabled: parsed_control.debug_enabled,
            control_mode: parsed_control.mode,
            log_level: SmolStr::new(log.level),
            retain_mode: parsed_retain_mode,
            retain_path: retain.path.map(PathBuf::from),
            retain_save_interval: Duration::from_millis(retain.save_interval_ms as i64),
            watchdog: WatchdogPolicy {
                enabled: watchdog.enabled,
                timeout: Duration::from_millis(watchdog.timeout_ms as i64),
                action: watchdog_action,
            },
            fault_policy,
            web: parsed_web.config,
            tls: parsed_tls.config,
            deploy: parsed_deploy.config,
            discovery: parsed_discovery.config,
            mesh: parsed_mesh.config,
            runtime_cloud_profile: parsed_cloud.profile,
            runtime_cloud_wan_allow_write: parsed_cloud.wan_allow_write,
            runtime_cloud_link_preferences: parsed_cloud.link_preferences,
            observability,
            opcua,
            tasks,
        })
    }
}
