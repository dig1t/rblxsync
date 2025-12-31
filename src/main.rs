use clap::{Parser, Subcommand};
use rbxsync::config::{Config, RbxSyncConfig};
use rbxsync::api::{RobloxClient, RobloxCookieClient};
use rbxsync::state::SyncState;
use rbxsync::commands;
use log::{info, error};
use std::path::Path;

#[derive(Parser)]
#[command(name = "rbxsync")]
#[command(about = "Manage Roblox experience metadata via Open Cloud", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to config file
    #[arg(short, long, default_value = "rbxsync.yml")]
    config: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Sync universe settings and assets (default)
    Run {
        /// Preview changes without applying them
        #[arg(long)]
        dry_run: bool,
    },
    /// Publish place files
    Publish,
    /// Validate configuration file
    Validate,
    /// Export existing resources to Luau/Lua
    Export {
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
        /// Export as Lua instead of Luau
        #[arg(long)]
        lua: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Cli::parse();
    
    // Check for "Validate" command early to avoid needing API key if possible, 
    // but for now we'll load env for all.
    let env_config = Config::from_env(); 

    let command = args.command.unwrap_or(Commands::Run { dry_run: false });

    match command {
        Commands::Validate => {
            let path = Path::new(&args.config);
            if !path.exists() {
                error!("Config file not found: {}", args.config);
                std::process::exit(1);
            }
            match RbxSyncConfig::load(path) {
                Ok(config) => {
                    // Run additional validation checks
                    if let Err(e) = commands::validate(&config) {
                        error!("Config validation failed: {}", e);
                        std::process::exit(1);
                    }
                    info!("Config file is valid.");
                }
                Err(e) => {
                    error!("Config validation failed: {}", e);
                    std::process::exit(1);
                }
            }
            return Ok(());
        }
        _ => {}
    }

    // Load Env Config (API Key)
    let env_config = match env_config {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load environment: {}", e);
            error!("Ensure ROBLOX_API_KEY is set.");
            std::process::exit(1);
        }
    };

    let client = RobloxClient::new(env_config.api_key);

    match command {
        Commands::Run { dry_run } => {
            if dry_run {
                info!("Dry-run mode enabled.");
            }
            let config_path = Path::new(&args.config);
            let config = RbxSyncConfig::load(config_path)?;
            let root = config_path.parent().unwrap_or(Path::new("."));
            let state = SyncState::load(root)?;
            
            // Check if universe settings are defined and require ROBLOX_COOKIE
            let cookie_client = if config.universe.has_settings() {
                match &env_config.roblox_cookie {
                    Some(cookie) => {
                        info!("Universe settings detected, using cookie authentication for develop.roblox.com API");
                        Some(RobloxCookieClient::new(cookie.clone()))
                    }
                    None => {
                        error!("Universe settings are defined in {} but ROBLOX_COOKIE is not set.", args.config);
                        error!("");
                        error!("To update universe settings (name, description, etc.), you must provide your");
                        error!(".ROBLOSECURITY cookie. Add the following to your .env file:");
                        error!("");
                        error!("  ROBLOX_COOKIE=your_.ROBLOSECURITY_cookie_value_here");
                        error!("");
                        error!("To get your .ROBLOSECURITY cookie:");
                        error!("  1. Log into roblox.com in your browser");
                        error!("  2. Open Developer Tools (F12) > Application > Cookies");
                        error!("  3. Copy the value of .ROBLOSECURITY");
                        error!("");
                        error!("WARNING: Keep this cookie secret! Anyone with it can access your account.");
                        std::process::exit(1);
                    }
                }
            } else {
                None
            };
            
            commands::run(config, state, client, cookie_client, dry_run).await?;
        }
        Commands::Publish => {
            let config = RbxSyncConfig::load(Path::new(&args.config))?;
            commands::publish(config, client).await?;
        }
        Commands::Export { output, lua } => {
            let config = RbxSyncConfig::load(Path::new(&args.config))?;
            commands::export(config, client, output, lua).await?;
        }
        Commands::Validate => unreachable!(), // Handled above
    }

    Ok(())
}
