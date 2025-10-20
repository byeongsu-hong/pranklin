mod config;
mod init;
mod server;

use clap::Parser;
use config::{Cli, Commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start {
            grpc_addr,
            rpc_addr,
            db_path,
            chain_id,
            debug,
            bridge_operators,
            snapshot_enable,
            snapshot_interval,
            snapshot,
        } => {
            server::init_tracing(debug);
            server::start_server(
                &grpc_addr,
                &rpc_addr,
                &db_path,
                &chain_id,
                &bridge_operators,
                snapshot_enable,
                snapshot_interval,
                *snapshot,
            )
            .await?;
        }
        Commands::Version => {
            println!("Pranklin Perp DEX v{}", env!("CARGO_PKG_VERSION"));
            println!("Optimized perpetual futures exchange on Rollkit");
        }
    }

    Ok(())
}
