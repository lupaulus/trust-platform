use super::*;

#[test]
fn summarize_ns_computes_quantiles() {
    let summary = summarize_ns(&[1_000, 2_000, 3_000, 4_000, 5_000]);
    assert_eq!(summary.samples, 5);
    assert!((summary.min_us - 1.0).abs() < f64::EPSILON);
    assert!((summary.p50_us - 3.0).abs() < f64::EPSILON);
    assert!((summary.p95_us - 5.0).abs() < f64::EPSILON);
    assert!((summary.max_us - 5.0).abs() < f64::EPSILON);
}

#[test]
fn histogram_includes_overflow_bucket() {
    let histogram = histogram_from_ns(&[1_000, 2_000, 30_000_000]);
    assert_eq!(histogram.len(), HISTOGRAM_LIMITS_US.len() + 1);
    assert_eq!(histogram[0].count, 2);
    assert_eq!(histogram[histogram.len() - 1].count, 1);
}

#[test]
fn t0_shm_bench_json_output_contains_latency_and_overrun_fields() {
    let (report, format) = execute_bench(BenchAction::T0Shm {
        samples: 16,
        payload_bytes: 16,
        output: BenchOutputFormat::Json,
    })
    .expect("run t0 benchmark");
    let rendered = render_bench_output(&report, format).expect("render json");
    let value: serde_json::Value = serde_json::from_str(&rendered).expect("parse bench json");
    assert_eq!(
        value.get("benchmark").and_then(serde_json::Value::as_str),
        Some("t0-shm")
    );
    assert!(value
        .pointer("/report/round_trip_latency/p95_us")
        .and_then(serde_json::Value::as_f64)
        .is_some());
    assert!(value
        .pointer("/report/overruns")
        .and_then(serde_json::Value::as_u64)
        .is_some());
}

#[test]
fn mesh_zenoh_bench_json_output_contains_loss_and_reorder_fields() {
    let (report, format) = execute_bench(BenchAction::MeshZenoh {
        samples: 20,
        payload_bytes: 24,
        loss_rate: 0.1,
        reorder_rate: 0.2,
        output: BenchOutputFormat::Json,
    })
    .expect("run mesh benchmark");
    let rendered = render_bench_output(&report, format).expect("render json");
    let value: serde_json::Value = serde_json::from_str(&rendered).expect("parse bench json");
    assert_eq!(
        value.get("benchmark").and_then(serde_json::Value::as_str),
        Some("mesh-zenoh")
    );
    assert!(value
        .pointer("/report/loss_count")
        .and_then(serde_json::Value::as_u64)
        .is_some());
    assert!(value
        .pointer("/report/reorder_count")
        .and_then(serde_json::Value::as_u64)
        .is_some());
}

#[test]
fn dispatch_bench_table_output_contains_fanout_and_audit_metrics() {
    let (report, format) = execute_bench(BenchAction::Dispatch {
        samples: 12,
        payload_bytes: 8,
        fanout: 3,
        output: BenchOutputFormat::Table,
    })
    .expect("run dispatch benchmark");
    let rendered = render_bench_output(&report, format).expect("render table");
    assert!(rendered.contains("fanout=3"));
    assert!(rendered.contains("audit-correlation latency"));
}

#[test]
fn mesh_workload_rejects_out_of_range_rates() {
    let err = MeshBenchWorkload::normalize(10, 32, -0.1, 0.0).expect_err("invalid rate");
    assert!(err.to_string().contains("--loss-rate"));

    let err = MeshBenchWorkload::normalize(10, 32, 0.0, 1.1).expect_err("invalid rate");
    assert!(err.to_string().contains("--reorder-rate"));
}
