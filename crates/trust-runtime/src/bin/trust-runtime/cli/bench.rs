#[derive(Debug, Subcommand)]
pub enum BenchAction {
    /// Benchmark T0 SHM one-way/round-trip latency and overrun counters.
    #[command(name = "t0-shm")]
    T0Shm {
        /// Number of benchmark samples.
        #[arg(long, default_value_t = 1_000)]
        samples: usize,
        /// Payload size in bytes.
        #[arg(long = "payload-bytes", default_value_t = 32)]
        payload_bytes: usize,
        /// Output format (`table`, `json`).
        #[arg(long, value_enum, default_value_t = BenchOutputFormat::Table)]
        output: BenchOutputFormat,
    },
    /// Benchmark synthetic mesh pub/sub + query/reply latency and jitter.
    #[command(name = "mesh-zenoh")]
    MeshZenoh {
        /// Number of benchmark samples.
        #[arg(long, default_value_t = 1_000)]
        samples: usize,
        /// Payload size in bytes.
        #[arg(long = "payload-bytes", default_value_t = 64)]
        payload_bytes: usize,
        /// Synthetic packet loss rate in `[0.0, 1.0]`.
        #[arg(long = "loss-rate", default_value_t = 0.0)]
        loss_rate: f64,
        /// Synthetic packet reorder rate in `[0.0, 1.0]`.
        #[arg(long = "reorder-rate", default_value_t = 0.0)]
        reorder_rate: f64,
        /// Output format (`table`, `json`).
        #[arg(long, value_enum, default_value_t = BenchOutputFormat::Table)]
        output: BenchOutputFormat,
    },
    /// Benchmark runtime-cloud dispatch preflight/dispatch/audit-correlation latency.
    #[command(name = "dispatch")]
    Dispatch {
        /// Number of benchmark samples.
        #[arg(long, default_value_t = 1_000)]
        samples: usize,
        /// Payload size in bytes.
        #[arg(long = "payload-bytes", default_value_t = 32)]
        payload_bytes: usize,
        /// Number of target runtimes per dispatch.
        #[arg(long, default_value_t = 3)]
        fanout: usize,
        /// Output format (`table`, `json`).
        #[arg(long, value_enum, default_value_t = BenchOutputFormat::Table)]
        output: BenchOutputFormat,
    },
}
