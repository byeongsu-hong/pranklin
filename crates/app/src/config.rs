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
    Start {
        /// gRPC server address
        #[arg(long = "grpc.addr", default_value = "0.0.0.0:50051")]
        grpc_addr: String,

        /// RPC server address
        #[arg(long = "rpc.addr", default_value = "0.0.0.0:3000")]
        rpc_addr: String,

        /// Database path
        #[arg(long = "db.path", default_value = "./data/pranklin_db")]
        db_path: String,

        /// Chain ID
        #[arg(long = "chain.id", default_value = "pranklin-mainnet-1")]
        chain_id: String,

        /// Enable debug logging
        #[arg(long = "log.debug")]
        debug: bool,

        /// Bridge operator addresses (comma-separated)
        #[arg(long = "bridge.operators", value_delimiter = ',')]
        bridge_operators: Vec<String>,

        /// Enable snapshot auto-export
        #[arg(long = "snapshot.enable")]
        snapshot_enable: bool,

        /// Snapshot export interval in blocks
        #[arg(long = "snapshot.interval", default_value = "10000")]
        snapshot_interval: u64,

        #[command(flatten)]
        snapshot: Box<SnapshotConfig>,
    },

    /// Display version information
    Version,
}

/// Snapshot storage configuration
#[derive(Args)]
#[group(multiple = false)]
pub struct SnapshotConfig {
    #[command(flatten)]
    pub s3: Option<S3Config>,

    #[command(flatten)]
    pub gcs: Option<GcsConfig>,

    #[command(flatten)]
    pub local: Option<LocalConfig>,
}

/// AWS S3 snapshot configuration
#[derive(Args)]
pub struct S3Config {
    /// S3 bucket name
    #[arg(long = "snapshot.s3.bucket", required = false)]
    pub s3_bucket: Option<String>,

    /// S3 region
    #[arg(
        long = "snapshot.s3.region",
        default_value = "us-east-1",
        required = false
    )]
    pub region: String,

    /// S3 prefix/path in bucket
    #[arg(
        long = "snapshot.s3.prefix",
        default_value = "snapshots",
        required = false
    )]
    pub s3_prefix: String,
}

/// Google Cloud Storage snapshot configuration
#[derive(Args)]
pub struct GcsConfig {
    /// GCS bucket name
    #[arg(long = "snapshot.gcs.bucket", required = false)]
    pub gcs_bucket: Option<String>,

    /// GCS prefix/path in bucket
    #[arg(
        long = "snapshot.gcs.prefix",
        default_value = "snapshots",
        required = false
    )]
    pub gcs_prefix: String,
}

/// Local filesystem snapshot configuration
#[derive(Args)]
pub struct LocalConfig {
    /// Local snapshot directory path
    #[arg(
        long = "snapshot.local.path",
        default_value = "./snapshots",
        required = false
    )]
    pub path: String,
}

impl SnapshotConfig {
    /// Determine storage provider from configuration
    pub fn to_provider(&self) -> anyhow::Result<pranklin_state::CloudProvider> {
        use pranklin_state::{
            CloudProvider, GCSConfig as StateGCSConfig, S3Config as StateS3Config,
        };

        // Priority: S3 > GCS > Local
        if let Some(ref s3) = self.s3
            && let Some(ref bucket) = s3.s3_bucket
        {
            return Ok(CloudProvider::S3(StateS3Config::new(
                bucket.clone(),
                &s3.region,
                &s3.s3_prefix,
            )));
        }

        if let Some(ref gcs) = self.gcs
            && let Some(ref bucket) = gcs.gcs_bucket
        {
            return Ok(CloudProvider::GCS(StateGCSConfig::new(
                bucket.clone(),
                &gcs.gcs_prefix,
            )));
        }

        // Default to local
        let path = self
            .local
            .as_ref()
            .map(|l| l.path.clone())
            .unwrap_or_else(|| "./snapshots".to_string());

        Ok(CloudProvider::Local {
            path: std::path::PathBuf::from(path),
        })
    }
}
