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

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "benchmark", content = "report")]
enum BenchReport {
    #[serde(rename = "t0-shm")]
    T0Shm(T0ShmBenchReport),
    #[serde(rename = "mesh-zenoh")]
    MeshZenoh(MeshZenohBenchReport),
    #[serde(rename = "dispatch")]
    Dispatch(DispatchBenchReport),
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
