#[derive(Debug, Clone, Serialize)]
struct HistogramBucket {
    upper_us: Option<u64>,
    count: u64,
}

#[derive(Debug, Clone, Serialize)]
struct LatencySummary {
    samples: usize,
    min_us: f64,
    p50_us: f64,
    p95_us: f64,
    p99_us: f64,
    max_us: f64,
}

#[derive(Debug, Clone, Serialize)]
struct T0ShmBenchReport {
    scenario: &'static str,
    one_way_latency: LatencySummary,
    round_trip_latency: LatencySummary,
    jitter: LatencySummary,
    histogram: Vec<HistogramBucket>,
    overruns: u64,
    stale_reads: u64,
    spin_exhausted: u64,
    fallback_denied: u64,
}

#[derive(Debug, Clone, Serialize)]
struct MeshZenohBenchReport {
    scenario: &'static str,
    pub_sub_latency: LatencySummary,
    pub_sub_jitter: LatencySummary,
    query_reply_latency: LatencySummary,
    histogram: Vec<HistogramBucket>,
    loss_count: u64,
    reorder_count: u64,
    configured_loss_rate: f64,
    configured_reorder_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
struct DispatchBenchReport {
    scenario: &'static str,
    fanout: usize,
    preflight_latency: LatencySummary,
    dispatch_latency: LatencySummary,
    end_to_end_latency: LatencySummary,
    audit_correlation_latency: LatencySummary,
    histogram: Vec<HistogramBucket>,
}

#[cfg(feature = "legacy-interpreter")]
#[derive(Debug, Clone, Serialize)]
struct BackendComparisonSummary {
    latency: LatencySummary,
    throughput_cycles_per_sec: f64,
}

#[cfg(feature = "legacy-interpreter")]
#[derive(Debug, Clone, Serialize)]
struct VmProfileFallbackReasonReport {
    reason: String,
    count: u64,
}

#[cfg(feature = "legacy-interpreter")]
#[derive(Debug, Clone, Serialize)]
struct VmProfileHotBlockReport {
    pou_id: u32,
    block_id: u32,
    start_pc: u32,
    hits: u64,
}

#[cfg(feature = "legacy-interpreter")]
#[derive(Debug, Clone, Serialize)]
struct VmTier1SpecializedExecutorDeoptReasonReport {
    reason: String,
    count: u64,
}

#[cfg(feature = "legacy-interpreter")]
#[derive(Debug, Clone, Serialize)]
struct VmTier1SpecializedExecutorReport {
    enabled: bool,
    hot_block_threshold: u64,
    cache_capacity: usize,
    cached_blocks: usize,
    compile_attempts: u64,
    compile_successes: u64,
    compile_failures: u64,
    cache_evictions: u64,
    block_executions: u64,
    deopt_count: u64,
    deopt_reasons: Vec<VmTier1SpecializedExecutorDeoptReasonReport>,
}

#[cfg(feature = "legacy-interpreter")]
#[derive(Debug, Clone, Serialize)]
struct VmRegisterLoweringCacheReport {
    enabled: bool,
    cache_capacity: usize,
    cached_entries: usize,
    hits: u64,
    misses: u64,
    hit_ratio: f64,
    build_errors: u64,
    cache_evictions: u64,
    invalidations: u64,
}

#[cfg(feature = "legacy-interpreter")]
#[derive(Debug, Clone, Serialize)]
struct VmProfileReport {
    register_programs_executed: u64,
    register_program_fallbacks: u64,
    fallback_reasons: Vec<VmProfileFallbackReasonReport>,
    hot_blocks: Vec<VmProfileHotBlockReport>,
    profiling_overhead_ratio: f64,
    register_lowering_cache: VmRegisterLoweringCacheReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    tier1_specialized_executor: Option<VmTier1SpecializedExecutorReport>,
}

#[cfg(feature = "legacy-interpreter")]
#[derive(Debug, Clone, Serialize)]
struct ExecutionBackendFixtureReport {
    fixture: &'static str,
    interpreter: BackendComparisonSummary,
    vm: BackendComparisonSummary,
    median_latency_ratio: f64,
    p99_latency_ratio: f64,
    throughput_ratio: f64,
    vm_profile: Option<VmProfileReport>,
}

#[cfg(feature = "legacy-interpreter")]
#[derive(Debug, Clone, Serialize)]
struct ExecutionBackendBenchReport {
    scenario: &'static str,
    corpus: &'static str,
    cycles_per_fixture: usize,
    warmup_cycles: usize,
    fixtures: Vec<ExecutionBackendFixtureReport>,
    aggregate: ExecutionBackendFixtureReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "benchmark", content = "report")]
#[allow(clippy::large_enum_variant)]
enum BenchReport {
    #[serde(rename = "t0-shm")]
    T0Shm(T0ShmBenchReport),
    #[serde(rename = "mesh-zenoh")]
    MeshZenoh(MeshZenohBenchReport),
    #[serde(rename = "dispatch")]
    Dispatch(DispatchBenchReport),
    #[cfg(feature = "legacy-interpreter")]
    #[serde(rename = "execution-backend")]
    ExecutionBackend(ExecutionBackendBenchReport),
}

#[derive(Debug, Clone)]
struct BenchWorkload {
    samples: usize,
    payload_bytes: usize,
}

impl BenchWorkload {
    fn normalize(samples: usize, payload_bytes: usize) -> anyhow::Result<Self> {
        if samples == 0 {
            anyhow::bail!("--samples must be greater than zero");
        }
        if payload_bytes == 0 {
            anyhow::bail!("--payload-bytes must be greater than zero");
        }
        Ok(Self {
            samples,
            payload_bytes,
        })
    }
}

#[derive(Debug, Clone)]
struct MeshBenchWorkload {
    base: BenchWorkload,
    loss_rate: f64,
    reorder_rate: f64,
}

impl MeshBenchWorkload {
    fn normalize(
        samples: usize,
        payload_bytes: usize,
        loss_rate: f64,
        reorder_rate: f64,
    ) -> anyhow::Result<Self> {
        if !(0.0..=1.0).contains(&loss_rate) {
            anyhow::bail!("--loss-rate must be between 0.0 and 1.0");
        }
        if !(0.0..=1.0).contains(&reorder_rate) {
            anyhow::bail!("--reorder-rate must be between 0.0 and 1.0");
        }
        Ok(Self {
            base: BenchWorkload::normalize(samples, payload_bytes)?,
            loss_rate,
            reorder_rate,
        })
    }
}

#[derive(Debug, Clone)]
struct DispatchBenchWorkload {
    base: BenchWorkload,
    fanout: usize,
}

impl DispatchBenchWorkload {
    fn normalize(samples: usize, payload_bytes: usize, fanout: usize) -> anyhow::Result<Self> {
        if fanout == 0 {
            anyhow::bail!("--fanout must be greater than zero");
        }
        Ok(Self {
            base: BenchWorkload::normalize(samples, payload_bytes)?,
            fanout,
        })
    }
}

#[cfg(feature = "legacy-interpreter")]
#[derive(Debug, Clone)]
struct ExecutionBackendBenchWorkload {
    samples: usize,
    warmup_cycles: usize,
}

#[cfg(feature = "legacy-interpreter")]
impl ExecutionBackendBenchWorkload {
    fn normalize(samples: usize, warmup_cycles: usize) -> anyhow::Result<Self> {
        if samples == 0 {
            anyhow::bail!("--samples must be greater than zero");
        }
        Ok(Self {
            samples,
            warmup_cycles,
        })
    }
}

#[cfg(not(feature = "legacy-interpreter"))]
#[derive(Debug, Clone)]
struct ExecutionBackendBenchWorkload;

#[cfg(not(feature = "legacy-interpreter"))]
impl ExecutionBackendBenchWorkload {
    fn normalize(_samples: usize, _warmup_cycles: usize) -> anyhow::Result<Self> {
        anyhow::bail!(
            "bench execution-backend requires --features legacy-interpreter for interpreter-vs-vm comparison"
        );
    }
}
