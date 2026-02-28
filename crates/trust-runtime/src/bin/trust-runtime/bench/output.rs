fn render_bench_output(report: &BenchReport, format: BenchOutputFormat) -> anyhow::Result<String> {
    match format {
        BenchOutputFormat::Json => {
            let mut text = serde_json::to_string_pretty(report).context("encode bench json")?;
            text.push('\n');
            Ok(text)
        }
        BenchOutputFormat::Table => Ok(render_table(report)),
    }
}

fn render_table(report: &BenchReport) -> String {
    let mut out = String::new();
    match report {
        BenchReport::T0Shm(data) => {
            let _ = writeln!(out, "Benchmark: {}", data.scenario);
            render_latency_block(&mut out, "one-way latency", &data.one_way_latency);
            render_latency_block(&mut out, "round-trip latency", &data.round_trip_latency);
            render_latency_block(&mut out, "jitter", &data.jitter);
            let _ = writeln!(out, "overruns={} stale_reads={} spin_exhausted={} fallback_denied={}",
                data.overruns, data.stale_reads, data.spin_exhausted, data.fallback_denied);
            render_histogram(&mut out, data.histogram.as_slice());
        }
        BenchReport::MeshZenoh(data) => {
            let _ = writeln!(out, "Benchmark: {}", data.scenario);
            render_latency_block(&mut out, "pub/sub latency", &data.pub_sub_latency);
            render_latency_block(&mut out, "pub/sub jitter", &data.pub_sub_jitter);
            render_latency_block(&mut out, "query/reply latency", &data.query_reply_latency);
            let _ = writeln!(
                out,
                "loss_count={} reorder_count={} configured_loss_rate={:.3} configured_reorder_rate={:.3}",
                data.loss_count,
                data.reorder_count,
                data.configured_loss_rate,
                data.configured_reorder_rate
            );
            render_histogram(&mut out, data.histogram.as_slice());
        }
        BenchReport::Dispatch(data) => {
            let _ = writeln!(out, "Benchmark: {}", data.scenario);
            let _ = writeln!(out, "fanout={}", data.fanout);
            render_latency_block(&mut out, "preflight latency", &data.preflight_latency);
            render_latency_block(&mut out, "dispatch latency", &data.dispatch_latency);
            render_latency_block(&mut out, "end-to-end latency", &data.end_to_end_latency);
            render_latency_block(
                &mut out,
                "audit-correlation latency",
                &data.audit_correlation_latency,
            );
            render_histogram(&mut out, data.histogram.as_slice());
        }
    }
    out
}

fn render_latency_block(out: &mut String, label: &str, summary: &LatencySummary) {
    let _ = writeln!(
        out,
        "{label}: samples={} min={:.3}us p50={:.3}us p95={:.3}us p99={:.3}us max={:.3}us",
        summary.samples,
        summary.min_us,
        summary.p50_us,
        summary.p95_us,
        summary.p99_us,
        summary.max_us
    );
}

fn render_histogram(out: &mut String, buckets: &[HistogramBucket]) {
    let _ = writeln!(out, "histogram:");
    for bucket in buckets {
        match bucket.upper_us {
            Some(upper) => {
                let _ = writeln!(out, "  <= {:>6}us : {}", upper, bucket.count);
            }
            None => {
                let _ = writeln!(out, "  >  {:>6}us : {}", HISTOGRAM_LIMITS_US[HISTOGRAM_LIMITS_US.len() - 1], bucket.count);
            }
        }
    }
}
