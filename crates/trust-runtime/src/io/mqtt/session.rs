trait MqttSession: Send {
    fn is_connected(&self) -> bool;
    fn take_payload(&mut self) -> Option<Vec<u8>>;
    fn publish(&mut self, topic: &str, payload: &[u8]) -> Result<(), RuntimeError>;
    fn last_error(&self) -> Option<SmolStr>;
}

trait MqttSessionFactory: Send + Sync {
    fn connect(&self, config: &MqttIoConfig) -> Result<Box<dyn MqttSession>, RuntimeError>;
}

#[derive(Debug, Default)]
struct RumqttSessionFactory;

struct RumqttSession {
    client: Client,
    incoming: Arc<Mutex<Option<Vec<u8>>>>,
    connected: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<SmolStr>>>,
    _worker: thread::JoinHandle<()>,
}

impl MqttSessionFactory for RumqttSessionFactory {
    fn connect(&self, config: &MqttIoConfig) -> Result<Box<dyn MqttSession>, RuntimeError> {
        let mut options = MqttOptions::new(
            config.client_id.as_str(),
            config.endpoint.host.as_str(),
            config.endpoint.port,
        );
        options.set_keep_alive(StdDuration::from_secs(5));
        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            options.set_credentials(username.as_str(), password.as_str());
        }
        let (client, mut connection) = Client::new(options, 64);
        client
            .subscribe(config.topic_in.as_str(), QoS::AtMostOnce)
            .map_err(|err| RuntimeError::IoDriver(format!("mqtt subscribe: {err}").into()))?;

        let incoming = Arc::new(Mutex::new(None));
        let connected = Arc::new(AtomicBool::new(false));
        let last_error = Arc::new(Mutex::new(None));
        let incoming_ref = Arc::clone(&incoming);
        let connected_ref = Arc::clone(&connected);
        let last_error_ref = Arc::clone(&last_error);
        let topic_in = config.topic_in.clone();
        let worker = thread::spawn(move || {
            for event in connection.iter() {
                match event {
                    Ok(Event::Incoming(Packet::ConnAck(_)))
                    | Ok(Event::Incoming(Packet::SubAck(_)))
                    | Ok(Event::Outgoing(_)) => {
                        connected_ref.store(true, Ordering::SeqCst);
                    }
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        connected_ref.store(true, Ordering::SeqCst);
                        if publish.topic == topic_in {
                            let mut guard = incoming_ref.lock().unwrap_or_else(|e| e.into_inner());
                            *guard = Some(publish.payload.to_vec());
                        }
                    }
                    Ok(_) => {}
                    Err(err) => {
                        connected_ref.store(false, Ordering::SeqCst);
                        let mut guard = last_error_ref.lock().unwrap_or_else(|e| e.into_inner());
                        *guard = Some(SmolStr::new(format!("mqtt event loop: {err}")));
                        break;
                    }
                }
            }
            connected_ref.store(false, Ordering::SeqCst);
            let mut guard = last_error_ref.lock().unwrap_or_else(|e| e.into_inner());
            if guard.is_none() {
                *guard = Some(SmolStr::new("mqtt connection closed"));
            }
        });

        Ok(Box::new(RumqttSession {
            client,
            incoming,
            connected,
            last_error,
            _worker: worker,
        }))
    }
}

impl MqttSession for RumqttSession {
    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn take_payload(&mut self) -> Option<Vec<u8>> {
        let mut guard = self.incoming.lock().unwrap_or_else(|e| e.into_inner());
        guard.take()
    }

    fn publish(&mut self, topic: &str, payload: &[u8]) -> Result<(), RuntimeError> {
        self.client
            .publish(topic, QoS::AtMostOnce, false, payload.to_vec())
            .map_err(|err| RuntimeError::IoDriver(format!("mqtt publish: {err}").into()))
    }

    fn last_error(&self) -> Option<SmolStr> {
        let guard = self.last_error.lock().unwrap_or_else(|e| e.into_inner());
        guard.clone()
    }
}
