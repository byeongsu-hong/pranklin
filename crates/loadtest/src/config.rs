use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "pranklin-loadtest")]
#[command(about = "Load testing tool for Pranklin RPC endpoints")]
pub struct LoadTestConfig {
    /// RPC endpoint URL
    #[arg(long, default_value = "http://localhost:3000")]
    pub rpc_url: String,

    /// Number of concurrent workers
    #[arg(long, short = 'w', default_value = "10")]
    pub num_workers: usize,

    /// Target transactions per second
    #[arg(long, short = 't', default_value = "100")]
    pub target_tps: usize,

    /// Duration of load test in seconds
    #[arg(long, short = 'd', default_value = "30")]
    pub duration_secs: u64,

    /// Load test mode
    #[arg(long, short = 'm', value_enum, default_value = "sustained")]
    pub mode: LoadTestMode,

    /// Transaction type to test
    #[arg(long, value_enum, default_value = "mixed")]
    pub tx_type: TransactionType,

    /// Number of unique wallets to use (for distributing nonces)
    #[arg(long, default_value = "100")]
    pub num_wallets: usize,

    /// Market ID for order transactions
    #[arg(long, default_value = "0")]
    pub market_id: u32,

    /// Asset ID for deposit/withdraw transactions
    #[arg(long, default_value = "0")]
    pub asset_id: u32,

    /// Ramp up duration in seconds (for ramp mode)
    #[arg(long, default_value = "10")]
    pub ramp_up_secs: u64,

    /// Burst duration in seconds (for burst mode)
    #[arg(long, default_value = "5")]
    pub burst_duration_secs: u64,

    /// Burst interval in seconds (for burst mode)
    #[arg(long, default_value = "15")]
    pub burst_interval_secs: u64,

    /// Bridge operator mode: initialize accounts with mock balances
    #[arg(long)]
    pub operator_mode: bool,

    /// Initial balance per wallet (in base units, e.g., 10000_000000 for $10k with 6 decimals)
    #[arg(long, default_value = "10000000000")]
    pub initial_balance: u128,

    /// Scenario mode: which test scenario to run
    #[arg(long, value_enum, default_value = "standard")]
    pub scenario: TestScenario,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum TestScenario {
    /// Standard load test (original behavior)
    Standard,
    /// Order spam: submit & cancel rapidly
    OrderSpam,
    /// Order matching: create matching buy/sell orders
    OrderMatching,
    /// Aggressive: build orderbook then spam market orders
    Aggressive,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum LoadTestMode {
    /// Sustained constant load
    Sustained,
    /// Gradually increasing load
    Ramp,
    /// Periodic bursts of high load
    Burst,
    /// Maximum throughput test
    Stress,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum TransactionType {
    /// Place order transactions only
    PlaceOrder,
    /// Cancel order transactions only
    CancelOrder,
    /// Deposit transactions only
    Deposit,
    /// Withdraw transactions only
    Withdraw,
    /// Transfer transactions only
    Transfer,
    /// Mixed transaction types
    Mixed,
}
