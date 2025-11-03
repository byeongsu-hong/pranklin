use alloy_primitives::Address;
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pranklin")]
#[command(version, about = "Pranklin Perp DEX - Decentralized Perpetual Exchange", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the Pranklin daemon
    Start(Box<StartConfig>),
    /// Display version information
    Version,
}

#[derive(Args)]
pub struct StartConfig {
    /// gRPC server address
    #[arg(long = "grpc.addr", default_value = "0.0.0.0:50051")]
    pub grpc_addr: String,

    /// RPC server address
    #[arg(long = "rpc.addr", default_value = "0.0.0.0:3000")]
    pub rpc_addr: String,

    /// Database path
    #[arg(long = "db.path", default_value = "./data/pranklin_db")]
    pub db_path: String,

    /// Chain ID
    #[arg(long = "chain.id", default_value = "pranklin-mainnet-1")]
    pub chain_id: String,

    /// Enable debug logging
    #[arg(long = "log.debug")]
    pub debug: bool,

    /// Bridge operator addresses (comma-separated)
    #[arg(long = "bridge.operators", value_delimiter = ',')]
    pub bridge_operators: Vec<String>,

    /// Enable snapshot auto-export
    #[arg(long = "snapshot.enable")]
    pub snapshot_enable: bool,

    /// Snapshot export interval in blocks
    #[arg(long = "snapshot.interval", default_value = "10000")]
    pub snapshot_interval: u64,

    #[command(flatten)]
    pub snapshot: SnapshotConfig,
}

/// Snapshot storage configuration
#[derive(Args)]
pub struct SnapshotConfig {
    /// S3 bucket name
    #[arg(long = "snapshot.s3.bucket")]
    pub s3_bucket: Option<String>,

    /// S3 region
    #[arg(long = "snapshot.s3.region", default_value = "us-east-1")]
    pub s3_region: String,

    /// S3 prefix/path in bucket
    #[arg(long = "snapshot.s3.prefix", default_value = "snapshots")]
    pub s3_prefix: String,

    /// GCS bucket name
    #[arg(long = "snapshot.gcs.bucket")]
    pub gcs_bucket: Option<String>,

    /// GCS prefix/path in bucket
    #[arg(long = "snapshot.gcs.prefix", default_value = "snapshots")]
    pub gcs_prefix: String,

    /// Local snapshot directory path
    #[arg(long = "snapshot.local.path", default_value = "./snapshots")]
    pub local_path: String,
}

impl StartConfig {
    pub fn snapshot_exporter_config(&self) -> Option<pranklin_state::SnapshotExporterConfig> {
        self.snapshot_enable
            .then(|| pranklin_state::SnapshotExporterConfig {
                provider: (&self.snapshot).into(),
                auto_export_interval: self.snapshot_interval,
                chain_id: self.chain_id.clone(),
            })
    }

    pub fn parse_bridge_operators(&self) -> anyhow::Result<Vec<Address>> {
        self.bridge_operators
            .iter()
            .map(|s| s.parse())
            .collect::<Result<_, _>>()
            .map_err(Into::into)
    }

    pub fn has_bridge_operators(&self) -> bool {
        !self.bridge_operators.is_empty()
    }
}

impl From<&SnapshotConfig> for pranklin_state::CloudProvider {
    fn from(config: &SnapshotConfig) -> Self {
        use pranklin_state::{CloudProvider, GCSConfig, S3Config};

        // Priority: S3 > GCS > Local
        config
            .s3_bucket
            .as_ref()
            .map(|bucket| {
                CloudProvider::S3(S3Config {
                    bucket: bucket.clone(),
                    region: config.s3_region.clone(),
                    prefix: config.s3_prefix.clone(),
                })
            })
            .or_else(|| {
                config.gcs_bucket.as_ref().map(|bucket| {
                    CloudProvider::GCS(GCSConfig {
                        bucket: bucket.clone(),
                        prefix: config.gcs_prefix.clone(),
                    })
                })
            })
            .unwrap_or_else(|| CloudProvider::Local {
                path: config.local_path.as_str().into(),
            })
    }
}
