use clap::{Parser, Subcommand};
use log::{error, info};
use rblxsync::api::{RobloxClient, RobloxCookieClient};
use rblxsync::commands;
use rblxsync::config::{Config, RblxSyncConfig};
use rblxsync::state::SyncState;
use std::path::Path;

#[derive(Parser)]
#[command(name = "rblxsync")]
#[command(version)]
#[command(about = "Manage Roblox experience metadata via Open Cloud", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to config file
    #[arg(short, long, global = true, default_value = "rblxsync.yml")]
    config: String,

    /// Environment from the config's `environments:` to sync (a dev or test
    /// universe). Leave it out for the top-level universe.
    #[arg(short, long, global = true)]
    env: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Sync universe settings and assets (default)
    Run {
        /// Preview changes without applying them
        #[arg(long)]
        dry_run: bool,
        /// Create badges past today's free quota at 100 Robux each. Without
        /// it, a run that needs a paid badge stops before changing anything.
        #[arg(long)]
        allow_paid_badges: bool,
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
        /// Use a .lua output extension instead of .luau (only changes the default
        /// filename; the generated content is identical and valid Lua/Luau)
        #[arg(long)]
        lua: bool,
    },
    /// Import existing experience metadata into rblxsync.yml and the lock file
    Import {
        /// Universe ID to import (falls back to universe.id in an existing config)
        #[arg(long)]
        universe_id: Option<u64>,
        /// Additional place IDs to import (repeatable). The API key can only
        /// auto-discover the root place; pass --place-id for each extra place.
        #[arg(long = "place-id")]
        place_id: Vec<u64>,
        /// Badge IDs to import (repeatable). Roblox's API cannot list DISABLED
        /// badges, so pass --badge-id to pull each disabled badge in by id.
        #[arg(long = "badge-id")]
        badge_id: Vec<u64>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Cli::parse();

    // Check for "Validate" command early to avoid needing API key if possible,
    // but for now we'll load env for all.
    let env_config = Config::from_env();

    let command = args.command.unwrap_or(Commands::Run {
        dry_run: false,
        allow_paid_badges: false,
    });

    if let Commands::Validate = command {
        let path = Path::new(&args.config);
        if !path.exists() {
            error!("Config file not found: {}", args.config);
            std::process::exit(1);
        }
        match RblxSyncConfig::load(path) {
            Ok(config) => {
                // Run additional validation checks
                let checked = commands::validate(&config)
                    .and_then(|_| config.resolve(args.env.as_deref()).map(|_| ()));
                if let Err(e) = checked {
                    error!("Config validation failed: {:#}", e);
                    std::process::exit(1);
                }
                info!("Config file is valid.");
            }
            Err(e) => {
                error!("Config validation failed: {:#}", e);
                std::process::exit(1);
            }
        }
        return Ok(());
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
        Commands::Run {
            dry_run,
            allow_paid_badges,
        } => {
            if dry_run {
                info!("Dry-run mode enabled.");
            }
            let config_path = Path::new(&args.config);
            let config = RblxSyncConfig::load(config_path)?.resolve(args.env.as_deref())?;
            let root = config_path.parent().unwrap_or(Path::new("."));
            let state = SyncState::load(root, config.env.as_deref())?;

            // Check if universe settings are defined and require ROBLOX_COOKIE
            let cookie_client = if config.universe.has_settings() {
                match &env_config.roblox_cookie {
                    Some(cookie) => {
                        info!("Universe settings detected, using cookie authentication for develop.roblox.com API");
                        Some(RobloxCookieClient::new(cookie.clone()))
                    }
                    None => {
                        error!(
                            "Universe settings are defined in {} but ROBLOX_COOKIE is not set.",
                            args.config
                        );
                        if let Some(env) = &config.env {
                            error!(
                                "Environment '{}' inherits the top-level `universe:` settings. Set any it shouldn't change to null under environments.{}.universe.",
                                env, env
                            );
                        }
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

            commands::run(
                config,
                state,
                client,
                cookie_client,
                dry_run,
                allow_paid_badges,
                config_path,
            )
            .await?;
        }
        Commands::Publish => {
            let config =
                RblxSyncConfig::load(Path::new(&args.config))?.resolve(args.env.as_deref())?;
            commands::publish(config, client).await?;
        }
        Commands::Export { output, lua } => {
            let config =
                RblxSyncConfig::load(Path::new(&args.config))?.resolve(args.env.as_deref())?;
            commands::export(config, client, output, lua).await?;
        }
        Commands::Import {
            universe_id,
            place_id,
            badge_id,
        } => {
            if let Some(env) = &args.env {
                // ponytail: import rewrites the shared entries from one
                // universe, so an environment's values would overwrite every
                // other's, and `run --env` already adopts by name. An import
                // that only fills `ids.<env>` is the upgrade if adopting through
                // `run` (which also patches each item) proves too heavy.
                error!("import pulls the top-level universe only: an environment's names and prices would overwrite the entries every environment shares.");
                error!(
                    "To link environment '{}' to items it already has, run `rblxsync run --env {} --dry-run`, then without --dry-run: it finds them by name, updates them to match the config, and writes each id into `ids: {{ {}: ... }}`. Place ids go in by hand.",
                    env, env, env
                );
                std::process::exit(1);
            }
            commands::import(
                client,
                Path::new(&args.config),
                universe_id,
                place_id,
                badge_id,
            )
            .await?;
        }
        Commands::Validate => unreachable!(), // Handled above
    }

    Ok(())
}
