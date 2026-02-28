pub struct MqttIoDriver {
    config: MqttIoConfig,
    factory: Arc<dyn MqttSessionFactory>,
    session: Option<Box<dyn MqttSession>>,
    health: IoDriverHealth,
    next_reconnect: Instant,
}

impl MqttIoDriver {
    pub fn from_params(value: &toml::Value) -> Result<Self, RuntimeError> {
        Self::from_params_with_factory(value, Arc::new(RumqttSessionFactory))
    }

    fn from_params_with_factory(
        value: &toml::Value,
        factory: Arc<dyn MqttSessionFactory>,
    ) -> Result<Self, RuntimeError> {
        let config = MqttIoConfig::from_params(value)?;
        Ok(Self {
            config,
            factory,
            session: None,
            health: IoDriverHealth::Degraded {
                error: SmolStr::new("mqtt initializing"),
            },
            next_reconnect: Instant::now(),
        })
    }

    pub fn validate_params(value: &toml::Value) -> Result<(), RuntimeError> {
        let _ = MqttIoConfig::from_params(value)?;
        Ok(())
    }

    fn set_degraded(&mut self, message: impl AsRef<str>) {
        self.health = IoDriverHealth::Degraded {
            error: SmolStr::new(message.as_ref()),
        };
    }

    fn ensure_session(&mut self) {
        let now = Instant::now();
        if let Some(session) = self.session.as_mut() {
            if session.is_connected() {
                self.health = IoDriverHealth::Ok;
                return;
            }
            if let Some(error) = session.last_error() {
                self.set_degraded(format!("mqtt disconnected: {error}"));
                if now < self.next_reconnect {
                    return;
                }
                self.session = None;
            } else {
                self.set_degraded("mqtt connecting");
                return;
            }
        }

        if now < self.next_reconnect {
            return;
        }
        match self.factory.connect(&self.config) {
            Ok(session) => {
                self.session = Some(session);
                self.set_degraded("mqtt connecting");
            }
            Err(err) => {
                self.session = None;
                self.set_degraded(format!("mqtt connect failed: {err}"));
                self.next_reconnect = now + self.config.reconnect;
            }
        }
    }
}

impl IoDriver for MqttIoDriver {
    fn read_inputs(&mut self, inputs: &mut [u8]) -> Result<(), RuntimeError> {
        self.ensure_session();
        if let Some(session) = self.session.as_mut() {
            if let Some(payload) = session.take_payload() {
                inputs.fill(0);
                for (dst, src) in inputs.iter_mut().zip(payload.iter()) {
                    *dst = *src;
                }
            }
            if session.is_connected() {
                self.health = IoDriverHealth::Ok;
            }
        }
        Ok(())
    }

    fn write_outputs(&mut self, outputs: &[u8]) -> Result<(), RuntimeError> {
        self.ensure_session();
        if let Some(session) = self.session.as_mut() {
            if let Err(err) = session.publish(self.config.topic_out.as_str(), outputs) {
                self.set_degraded(err.to_string());
                self.session = None;
                self.next_reconnect = Instant::now() + self.config.reconnect;
            } else if session.is_connected() {
                self.health = IoDriverHealth::Ok;
            }
        }
        Ok(())
    }

    fn health(&self) -> IoDriverHealth {
        self.health.clone()
    }
}
