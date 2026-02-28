impl RetainSnapshot {
    pub fn from_runtime(runtime: &Runtime) -> Self {
        runtime.retain_snapshot()
    }
}

/// File-based retain store.
#[derive(Debug, Clone)]
pub struct FileRetainStore {
    path: PathBuf,
}

impl FileRetainStore {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), RuntimeError> {
        let mut file = fs::File::create(path)
            .map_err(|err| RuntimeError::RetainStore(format!("create {path:?}: {err}").into()))?;
        file.write_all(bytes)
            .map_err(|err| RuntimeError::RetainStore(format!("write {path:?}: {err}").into()))
    }

    fn read_bytes(path: &Path) -> Result<Vec<u8>, RuntimeError> {
        let mut file = fs::File::open(path)
            .map_err(|err| RuntimeError::RetainStore(format!("open {path:?}: {err}").into()))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|err| RuntimeError::RetainStore(format!("read {path:?}: {err}").into()))?;
        Ok(buf)
    }
}

impl RetainStore for FileRetainStore {
    fn load(&self) -> Result<RetainSnapshot, RuntimeError> {
        if !self.path.exists() {
            return Ok(RetainSnapshot::default());
        }
        let bytes = Self::read_bytes(&self.path)?;
        decode_snapshot(&bytes)
    }

    fn store(&self, snapshot: &RetainSnapshot) -> Result<(), RuntimeError> {
        let bytes = encode_snapshot(snapshot)?;
        Self::write_bytes(&self.path, &bytes)
    }
}
