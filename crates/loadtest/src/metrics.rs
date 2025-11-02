use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Trait for converting histogram values to milliseconds
trait ToMillis {
    fn to_millis(&self) -> f64;
}

impl ToMillis for u64 {
    fn to_millis(&self) -> f64 {
        *self as f64 / 1000.0
    }
}

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
        let _ = inner.latency_histogram.record(latency.as_micros() as u64);
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
        let hist = &inner.latency_histogram;

        LoadTestResults {
            total_requests: inner.total_requests,
            successful_requests: inner.successful_requests,
            failed_requests: inner.failed_requests,
            duration_secs: inner.start_time.elapsed().as_secs_f64(),
            latency_min_ms: hist.min().to_millis(),
            latency_max_ms: hist.max().to_millis(),
            latency_mean_ms: hist.mean() / 1000.0,
            latency_p50_ms: hist.value_at_quantile(0.50).to_millis(),
            latency_p95_ms: hist.value_at_quantile(0.95).to_millis(),
            latency_p99_ms: hist.value_at_quantile(0.99).to_millis(),
            latency_p999_ms: hist.value_at_quantile(0.999).to_millis(),
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

impl LoadTestResults {
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            (self.successful_requests as f64 / self.total_requests as f64) * 100.0
        }
    }

    pub fn actual_tps(&self) -> f64 {
        if self.duration_secs == 0.0 {
            0.0
        } else {
            self.total_requests as f64 / self.duration_secs
        }
    }

    pub fn log_summary(&self) {
        tracing::info!("\n📊 Load Test Results:");
        tracing::info!("  Total Requests: {}", self.total_requests);
        tracing::info!("  Successful: {}", self.successful_requests);
        tracing::info!("  Failed: {}", self.failed_requests);
        tracing::info!("  Success Rate: {:.2}%", self.success_rate());
        tracing::info!("  Duration: {:.2}s", self.duration_secs);
        tracing::info!("  Actual TPS: {:.2}", self.actual_tps());

        tracing::info!("\n⏱️  Latency Statistics (ms):");
        tracing::info!("  Min: {:.2}", self.latency_min_ms);
        tracing::info!("  Max: {:.2}", self.latency_max_ms);
        tracing::info!("  Mean: {:.2}", self.latency_mean_ms);
        tracing::info!("  P50: {:.2}", self.latency_p50_ms);
        tracing::info!("  P95: {:.2}", self.latency_p95_ms);
        tracing::info!("  P99: {:.2}", self.latency_p99_ms);
        tracing::info!("  P99.9: {:.2}", self.latency_p999_ms);

        if !self.errors.is_empty() {
            tracing::warn!("\n⚠️  Errors encountered:");
            for (error, count) in self.errors.iter().take(10) {
                tracing::warn!("  [{}x] {}", count, error);
            }
            if self.errors.len() > 10 {
                tracing::warn!("  ... and {} more", self.errors.len() - 10);
            }
        }
    }
}
