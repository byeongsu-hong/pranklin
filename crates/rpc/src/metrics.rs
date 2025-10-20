use prometheus::{
    CounterVec, GaugeVec, HistogramVec, Registry, TextEncoder, register_counter_vec,
    register_gauge_vec, register_histogram_vec,
};
use std::sync::Arc;

/// Metrics collector for the RPC server
#[derive(Clone)]
pub struct Metrics {
    registry: Arc<Registry>,

    // Transaction metrics
    pub tx_submitted: CounterVec,
    pub tx_processed: CounterVec,
    pub tx_failed: CounterVec,

    // Order metrics
    pub orders_placed: CounterVec,
    pub orders_cancelled: CounterVec,
    pub orders_filled: CounterVec,

    // Position metrics
    pub positions_opened: CounterVec,
    pub positions_closed: CounterVec,
    pub liquidations: CounterVec,

    // Performance metrics
    pub request_duration: HistogramVec,
    pub tx_processing_duration: HistogramVec,

    // State metrics
    pub active_orders: GaugeVec,
    pub active_positions: GaugeVec,
    pub total_volume: CounterVec,

    // System metrics
    pub mempool_size: GaugeVec,
    pub block_height: GaugeVec,
}

impl Metrics {
    /// Create a new metrics collector
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        // Transaction metrics
        let tx_submitted = register_counter_vec!(
            "pranklin_tx_submitted_total",
            "Total number of transactions submitted",
            &["tx_type"]
        )?;
        registry.register(Box::new(tx_submitted.clone()))?;

        let tx_processed = register_counter_vec!(
            "pranklin_tx_processed_total",
            "Total number of transactions processed",
            &["tx_type", "status"]
        )?;
        registry.register(Box::new(tx_processed.clone()))?;

        let tx_failed = register_counter_vec!(
            "pranklin_tx_failed_total",
            "Total number of failed transactions",
            &["tx_type", "error_type"]
        )?;
        registry.register(Box::new(tx_failed.clone()))?;

        // Order metrics
        let orders_placed = register_counter_vec!(
            "pranklin_orders_placed_total",
            "Total number of orders placed",
            &["market_id", "side"]
        )?;
        registry.register(Box::new(orders_placed.clone()))?;

        let orders_cancelled = register_counter_vec!(
            "pranklin_orders_cancelled_total",
            "Total number of orders cancelled",
            &["market_id"]
        )?;
        registry.register(Box::new(orders_cancelled.clone()))?;

        let orders_filled = register_counter_vec!(
            "pranklin_orders_filled_total",
            "Total number of orders filled",
            &["market_id"]
        )?;
        registry.register(Box::new(orders_filled.clone()))?;

        // Position metrics
        let positions_opened = register_counter_vec!(
            "pranklin_positions_opened_total",
            "Total number of positions opened",
            &["market_id", "side"]
        )?;
        registry.register(Box::new(positions_opened.clone()))?;

        let positions_closed = register_counter_vec!(
            "pranklin_positions_closed_total",
            "Total number of positions closed",
            &["market_id"]
        )?;
        registry.register(Box::new(positions_closed.clone()))?;

        let liquidations = register_counter_vec!(
            "pranklin_liquidations_total",
            "Total number of liquidations",
            &["market_id"]
        )?;
        registry.register(Box::new(liquidations.clone()))?;

        // Performance metrics
        let request_duration = register_histogram_vec!(
            "pranklin_request_duration_seconds",
            "Request duration in seconds",
            &["endpoint", "method"]
        )?;
        registry.register(Box::new(request_duration.clone()))?;

        let tx_processing_duration = register_histogram_vec!(
            "pranklin_tx_processing_duration_seconds",
            "Transaction processing duration in seconds",
            &["tx_type"]
        )?;
        registry.register(Box::new(tx_processing_duration.clone()))?;

        // State metrics
        let active_orders = register_gauge_vec!(
            "pranklin_active_orders",
            "Number of active orders",
            &["market_id"]
        )?;
        registry.register(Box::new(active_orders.clone()))?;

        let active_positions = register_gauge_vec!(
            "pranklin_active_positions",
            "Number of active positions",
            &["market_id"]
        )?;
        registry.register(Box::new(active_positions.clone()))?;

        let total_volume = register_counter_vec!(
            "pranklin_total_volume",
            "Total trading volume",
            &["market_id"]
        )?;
        registry.register(Box::new(total_volume.clone()))?;

        // System metrics
        let mempool_size = register_gauge_vec!(
            "pranklin_mempool_size",
            "Number of transactions in mempool",
            &[]
        )?;
        registry.register(Box::new(mempool_size.clone()))?;

        let block_height =
            register_gauge_vec!("pranklin_block_height", "Current block height", &[])?;
        registry.register(Box::new(block_height.clone()))?;

        Ok(Self {
            registry: Arc::new(registry),
            tx_submitted,
            tx_processed,
            tx_failed,
            orders_placed,
            orders_cancelled,
            orders_filled,
            positions_opened,
            positions_closed,
            liquidations,
            request_duration,
            tx_processing_duration,
            active_orders,
            active_positions,
            total_volume,
            mempool_size,
            block_height,
        })
    }

    /// Export metrics in Prometheus format
    pub fn export(&self) -> Result<String, prometheus::Error> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        encoder.encode_to_string(&metric_families)
    }

    /// Record a transaction submission
    pub fn record_tx_submitted(&self, tx_type: &str) {
        self.tx_submitted.with_label_values(&[tx_type]).inc();
    }

    /// Record a transaction processing
    pub fn record_tx_processed(&self, tx_type: &str, success: bool) {
        let status = if success { "success" } else { "failure" };
        self.tx_processed
            .with_label_values(&[tx_type, status])
            .inc();
    }

    /// Record a transaction failure
    pub fn record_tx_failed(&self, tx_type: &str, error_type: &str) {
        self.tx_failed
            .with_label_values(&[tx_type, error_type])
            .inc();
    }

    /// Record an order placement
    pub fn record_order_placed(&self, market_id: u32, is_buy: bool) {
        let market_id_str = market_id.to_string();
        let side = if is_buy { "buy" } else { "sell" };
        self.orders_placed
            .with_label_values(&[market_id_str.as_str(), side])
            .inc();
    }

    /// Record an order cancellation
    pub fn record_order_cancelled(&self, market_id: u32) {
        self.orders_cancelled
            .with_label_values(&[&market_id.to_string()])
            .inc();
    }

    /// Record an order fill
    pub fn record_order_filled(&self, market_id: u32) {
        self.orders_filled
            .with_label_values(&[&market_id.to_string()])
            .inc();
    }

    /// Update active orders count
    pub fn set_active_orders(&self, market_id: u32, count: i64) {
        self.active_orders
            .with_label_values(&[&market_id.to_string()])
            .set(count as f64);
    }

    /// Update mempool size
    pub fn set_mempool_size(&self, size: i64) {
        self.mempool_size
            .with_label_values(&[] as &[&str])
            .set(size as f64);
    }

    /// Update block height
    pub fn set_block_height(&self, height: u64) {
        self.block_height
            .with_label_values(&[] as &[&str])
            .set(height as f64);
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new().expect("Failed to create metrics")
    }
}
