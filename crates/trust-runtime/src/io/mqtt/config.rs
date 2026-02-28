#[derive(Debug, Clone)]
struct BrokerEndpoint {
    host: SmolStr,
    port: u16,
}

#[derive(Debug, Clone)]
struct MqttIoConfig {
    endpoint: BrokerEndpoint,
    client_id: SmolStr,
    topic_in: SmolStr,
    topic_out: SmolStr,
    username: Option<SmolStr>,
    password: Option<SmolStr>,
    reconnect: StdDuration,
}

#[derive(Debug, Deserialize)]
struct MqttToml {
    broker: String,
    client_id: Option<String>,
    topic_in: Option<String>,
    topic_out: Option<String>,
    username: Option<String>,
    password: Option<String>,
    reconnect_ms: Option<u64>,
    keep_alive_s: Option<u64>,
    tls: Option<bool>,
    allow_insecure_remote: Option<bool>,
}

impl MqttIoConfig {
    fn from_params(value: &toml::Value) -> Result<Self, RuntimeError> {
        let params: MqttToml = value
            .clone()
            .try_into()
            .map_err(|err| RuntimeError::InvalidConfig(format!("io.params: {err}").into()))?;
        let endpoint = parse_broker_endpoint(&params.broker)?;
        let tls = params.tls.unwrap_or(false);
        if tls {
            return Err(RuntimeError::InvalidConfig(
                "mqtt tls=true is not yet supported (set tls=false for now)".into(),
            ));
        }
        let allow_insecure_remote = params.allow_insecure_remote.unwrap_or(false);
        if !allow_insecure_remote && !is_local_host(endpoint.host.as_str()) {
            return Err(RuntimeError::InvalidConfig(
                format!(
                    "mqtt insecure remote broker '{}' requires allow_insecure_remote=true",
                    endpoint.host
                )
                .into(),
            ));
        }
        let username = params.username.map(SmolStr::new);
        let password = params.password.map(SmolStr::new);
        if username.is_some() ^ password.is_some() {
            return Err(RuntimeError::InvalidConfig(
                "mqtt username/password must be set together".into(),
            ));
        }
        let client_id = params
            .client_id
            .map(SmolStr::new)
            .unwrap_or_else(|| SmolStr::new(format!("trust-runtime-{}", std::process::id())));
        let topic_in = params
            .topic_in
            .map(SmolStr::new)
            .unwrap_or_else(|| SmolStr::new("trust/io/in"));
        let topic_out = params
            .topic_out
            .map(SmolStr::new)
            .unwrap_or_else(|| SmolStr::new("trust/io/out"));
        let reconnect = StdDuration::from_millis(params.reconnect_ms.unwrap_or(500).max(1));
        let keep_alive_s = params.keep_alive_s.unwrap_or(5).max(1);
        if keep_alive_s > u16::MAX.into() {
            return Err(RuntimeError::InvalidConfig(
                "mqtt keep_alive_s must be <= 65535".into(),
            ));
        }

        Ok(Self {
            endpoint,
            client_id,
            topic_in,
            topic_out,
            username,
            password,
            reconnect,
        })
    }
}
