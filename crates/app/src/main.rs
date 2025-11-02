mod config;
mod init;
mod server;

use clap::Parser;
use config::{Cli, Commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Commands::Start(config) => {
            server::init_tracing(config.debug);
            server::start_server(&config).await
        }
        Commands::Version => {
            println!("Pranklin Perp DEX v{}", env!("CARGO_PKG_VERSION"));
            println!("Optimized perpetual futures exchange on Rollkit");
            Ok(())
        }
    }
}
