use clap::{Parser, Subcommand};
use rbxsync::config::Config;
use rbxsync::api::RobloxClient;
use log::{info, error};

#[derive(Parser)]
#[command(name = "rbxsync")]
#[command(about = "CLI for interacting with Roblox Cloud API", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Test connection by listing datastores (requires ROBLOX_UNIVERSE_ID)
    ListDatastores {
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Simple connectivity check (requires ROBLOX_UNIVERSE_ID)
    Ping,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Cli::parse();
    let config = Config::from_env()?;
    let client = RobloxClient::new(&config);

    match args.command {
        Commands::ListDatastores { limit } => {
            if let Some(universe_id) = config.universe_id {
                info!("Fetching datastores for universe: {}", universe_id);
                match client.list_datastores(universe_id, None, limit).await {
                    Ok(data) => {
                        println!("{}", serde_json::to_string_pretty(&data)?);
                    }
                    Err(e) => {
                        error!("Failed to list datastores: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                error!("ROBLOX_UNIVERSE_ID is required for this command");
                std::process::exit(1);
            }
        }
        Commands::Ping => {
            if let Some(universe_id) = config.universe_id {
                info!("Pinging Roblox API (Universe: {})...", universe_id);
                let start = std::time::Instant::now();
                match client.ping(universe_id).await {
                    Ok(_) => {
                        let duration = start.elapsed();
                        info!("Pong! API is accessible. Latency: {:?}", duration);
                    }
                    Err(e) => {
                        error!("Ping failed: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                error!("ROBLOX_UNIVERSE_ID is required for ping");
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
