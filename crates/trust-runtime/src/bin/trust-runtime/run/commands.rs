pub fn run_default(verbose: bool) -> anyhow::Result<()> {
    match detect_bundle_path(None) {
        Ok(path) => run_play(
            Some(path),
            "cold".to_string(),
            verbose,
            ConsoleMode::Auto,
            false,
            false,
            1,
        ),
        Err(_) => {
            if std::io::stdin().is_terminal() {
                setup::run_setup_default()
            } else {
                run_play(
                    None,
                    "cold".to_string(),
                    verbose,
                    ConsoleMode::Disabled,
                    false,
                    false,
                    1,
                )
            }
        }
    }
}

pub fn run_play(
    project: Option<PathBuf>,
    restart: String,
    verbose: bool,
    console: ConsoleMode,
    beginner: bool,
    simulation: bool,
    time_scale: u32,
) -> anyhow::Result<()> {
    let mut created = false;
    let project_path = match project {
        Some(path) => {
            if should_auto_create(&path)? {
                created = true;
                wizard::create_bundle_auto(Some(path))?
            } else {
                path
            }
        }
        None => match detect_bundle_path(None).map_err(anyhow::Error::from) {
            Ok(path) => {
                if should_auto_create(&path)? {
                    created = true;
                    wizard::create_bundle_auto(Some(path))?
                } else {
                    path
                }
            }
            Err(_) => {
                created = true;
                wizard::create_bundle_auto(None)?
            }
        },
    };
    if created {
        println!(
            "{}",
            style::accent("Welcome to trueST! Creating your first PLC project...")
        );
        println!(
            "{}",
            style::success(format!(
                "Created project folder: {}",
                project_path.display()
            ))
        );
        println!("What’s next: open http://localhost:8080 to monitor this PLC.");
    }
    run_runtime(
        Some(project_path),
        None,
        None,
        restart,
        verbose,
        true,
        console,
        beginner,
        simulation,
        time_scale,
    )
}

pub fn run_validate(bundle: PathBuf, ci: bool) -> anyhow::Result<()> {
    let bundle = RuntimeBundle::load(&bundle)?;
    let _tls_materials = load_tls_materials(&bundle.runtime.tls, Some(bundle.root.as_path()))?;
    let control_endpoint = ControlEndpoint::parse(bundle.runtime.control_endpoint.as_str())?;
    if matches!(control_endpoint, ControlEndpoint::Tcp(_))
        && bundle.runtime.control_auth_token.is_none()
    {
        anyhow::bail!("tcp control endpoint requires runtime.control.auth_token");
    }
    let registry = IoDriverRegistry::default_registry();
    for driver in &bundle.io.drivers {
        registry
            .validate(driver.name.as_str(), &driver.params)
            .map_err(anyhow::Error::from)?;
    }
    let module = BytecodeModule::decode(&bundle.bytecode)?;
    module.validate()?;
    let metadata = module.metadata()?;
    let _resource = metadata
        .resource(bundle.runtime.resource_name.as_str())
        .or_else(|| metadata.primary_resource())
        .ok_or_else(|| anyhow::anyhow!("bytecode metadata missing resource definitions"))?;
    if ci {
        let io_drivers = bundle
            .io
            .drivers
            .iter()
            .map(|driver| driver.name.to_string())
            .collect::<Vec<_>>();
        let payload = json!({
            "version": 1,
            "command": "validate",
            "status": "ok",
            "project": bundle.root.display().to_string(),
            "resource": bundle.runtime.resource_name.to_string(),
            "control_endpoint": bundle.runtime.control_endpoint.to_string(),
            "io_driver": io_drivers.first().cloned().unwrap_or_default(),
            "io_drivers": io_drivers,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }
    println!("{}", style::success("Project ok"));
    Ok(())
}
