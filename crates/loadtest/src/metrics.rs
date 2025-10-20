use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Metrics collector for load test results
#[derive(Clone)]
pub struct MetricsCollector {
    inner: Arc<Mutex<MetricsInner>>,
}

struct MetricsInner {
    start_time: Instant,
    total_requests: u64,
    successful_requests: u64,
    failed_requests: u64,
    latency_histogram: Histogram<u64>,
    errors: HashMap<String, u64>,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MetricsInner {
                start_time: Instant::now(),
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                latency_histogram: Histogram::new(3).expect("Failed to create histogram"),
                errors: HashMap::new(),
            })),
        }
    }

    /// Record a successful request
    pub async fn record_success(&self, latency: Duration) {
        let mut inner = self.inner.lock().await;
        inner.total_requests += 1;
        inner.successful_requests += 1;

        let latency_us = latency.as_micros() as u64;
        let _ = inner.latency_histogram.record(latency_us);
    }

    /// Record a failed request
    pub async fn record_failure(&self, error: String) {
        let mut inner = self.inner.lock().await;
        inner.total_requests += 1;
        inner.failed_requests += 1;
        *inner.errors.entry(error).or_insert(0) += 1;
    }

    /// Get the current results
    pub async fn get_results(&self) -> LoadTestResults {
        let inner = self.inner.lock().await;
        let duration = inner.start_time.elapsed();

        LoadTestResults {
            total_requests: inner.total_requests,
            successful_requests: inner.successful_requests,
            failed_requests: inner.failed_requests,
            duration_secs: duration.as_secs_f64(),
            latency_min_ms: inner.latency_histogram.min() as f64 / 1000.0,
            latency_max_ms: inner.latency_histogram.max() as f64 / 1000.0,
            latency_mean_ms: inner.latency_histogram.mean() / 1000.0,
            latency_p50_ms: inner.latency_histogram.value_at_quantile(0.50) as f64 / 1000.0,
            latency_p95_ms: inner.latency_histogram.value_at_quantile(0.95) as f64 / 1000.0,
            latency_p99_ms: inner.latency_histogram.value_at_quantile(0.99) as f64 / 1000.0,
            latency_p999_ms: inner.latency_histogram.value_at_quantile(0.999) as f64 / 1000.0,
            errors: inner.errors.clone(),
        }
    }

    /// Print periodic stats
    pub async fn print_stats(&self) {
        let results = self.get_results().await;
        tracing::info!(
            "📈 {} requests ({} success, {} failed) | TPS: {:.1} | Latency p50/p95/p99: {:.1}/{:.1}/{:.1}ms",
            results.total_requests,
            results.successful_requests,
            results.failed_requests,
            results.total_requests as f64 / results.duration_secs,
            results.latency_p50_ms,
            results.latency_p95_ms,
            results.latency_p99_ms,
        );
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Load test results
#[derive(Debug, Clone)]
pub struct LoadTestResults {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub duration_secs: f64,
    pub latency_min_ms: f64,
    pub latency_max_ms: f64,
    pub latency_mean_ms: f64,
    pub latency_p50_ms: f64,
    pub latency_p95_ms: f64,
    pub latency_p99_ms: f64,
    pub latency_p999_ms: f64,
    pub errors: HashMap<String, u64>,
}

