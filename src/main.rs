mod auth;
mod client;
mod commands;
mod error;
mod output;
mod time;
pub mod ui;
mod validation;
pub mod ws;

use clap::{Parser, Subcommand};
use error::AppError;
use output::OutputMode;

#[derive(Parser)]
#[command(name = "homeassist")]
#[command(about = "Home Assistant CLI for LLM agents - JSON output, minimal tokens")]
#[command(version)]
pub struct Cli {
    /// Home Assistant URL (env: HA_URL)
    #[arg(long)]
    url: Option<String>,

    /// Access token (env: HA_TOKEN)
    #[arg(long)]
    token: Option<String>,

    /// Human-readable output instead of JSON
    #[arg(short = 'H', long)]
    human: bool,

    /// Compact output for LLM token savings (auto-enabled in Claude Code)
    #[arg(short, long)]
    compact: bool,

    /// Disable compact mode even in LLM context
    #[arg(long)]
    no_compact: bool,

    /// Max items in list output (default: 50 in compact mode, unlimited otherwise)
    #[arg(long)]
    limit: Option<usize>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Entity operations
    Entities {
        #[command(subcommand)]
        action: EntityAction,
    },
    /// Service operations
    Services {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// Template operations
    Templates {
        #[command(subcommand)]
        action: TemplateAction,
    },
    /// Configuration operations
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Entity state history
    History {
        #[command(subcommand)]
        action: HistoryAction,
    },
    /// Error log access
    Logs {
        #[command(subcommand)]
        action: LogAction,
    },
    /// Event timeline (logbook)
    Logbook {
        #[command(subcommand)]
        action: LogbookAction,
    },
    /// Fire events
    Events {
        #[command(subcommand)]
        action: EventAction,
    },
    /// Automation operations
    Automations {
        #[command(subcommand)]
        action: AutomationAction,
    },
    /// Script operations
    Scripts {
        #[command(subcommand)]
        action: ScriptAction,
    },
    /// Execute multiple commands from JSONL
    Batch {
        /// Path to JSONL file (reads stdin if omitted)
        #[arg(long)]
        file: Option<String>,
    },
    /// Show entities that changed state recently
    Diff {
        /// Hours to look back (default: 1)
        #[arg(long, default_value = "1")]
        since: u32,
        /// Filter by domain
        #[arg(long)]
        domain: Option<String>,
    },
    /// Watch an entity for state changes
    Watch {
        /// Entity ID to watch
        entity_id: String,
        /// Timeout in seconds (default: 300, min: 1)
        #[arg(long, default_value = "300", value_parser = clap::value_parser!(u32).range(1..))]
        timeout: u32,
        /// Poll interval in seconds (default: 2, min: 1)
        #[arg(long, default_value = "2", value_parser = clap::value_parser!(u32).range(1..))]
        interval: u32,
        /// Wait for specific state value
        #[arg(long)]
        state: Option<String>,
    },
    /// Audit system health: unavailable/unknown entities, domain summary
    Inspect,
    /// Validate HA YAML config files (syntax, duplicates, common errors)
    Validate {
        /// Path to config directory or file (default: current directory)
        #[arg(default_value = ".")]
        path: String,
        /// Check entity references against live HA
        #[arg(long)]
        check_entities: bool,
        /// Check entity registry for orphaned entries (requires WebSocket)
        #[arg(long)]
        check_registry: bool,
        /// Check service calls against live HA services
        #[arg(long)]
        check_services: bool,
    },
    /// Post-deploy verification: health, entity counts, config validity
    Verify {
        /// Baseline snapshot file for delta comparison
        #[arg(long)]
        baseline: Option<String>,
    },
    /// Generate shell completions
    Completions {
        /// Shell type (bash, zsh, fish, powershell, elvish)
        shell: String,
    },
    /// Get server health and connection status
    Health,
    /// System status overview
    Stats {
        /// Path to dashboard YAML config
        #[arg(long)]
        dashboard: Option<String>,
        /// Generate starter dashboard config from current HA state
        #[arg(long)]
        init: bool,
    },
    /// Show LLM-optimized usage documentation
    Usage,
}

#[derive(Subcommand)]
enum EntityAction {
    /// List entities
    List {
        /// Filter by domain (e.g., light, sensor)
        #[arg(long)]
        domain: Option<String>,
        /// Filter by regex pattern
        #[arg(long)]
        pattern: Option<String>,
        /// Filter by regex on entity_id/friendly_name
        #[arg(long)]
        name: Option<String>,
        /// Filter by exact state value (e.g., on, off, unavailable)
        #[arg(long)]
        state: Option<String>,
    },
    /// Get single entity state
    Get {
        /// Entity ID
        entity_id: String,
    },
    /// Search entities by pattern
    Search {
        /// Search pattern (regex)
        pattern: String,
    },
}

#[derive(Subcommand)]
enum ServiceAction {
    /// List available services
    List {
        /// Filter by domain
        domain: Option<String>,
    },
    /// Call a service (format: domain.service)
    Call {
        /// Service name (e.g., light.turn_on)
        service: String,
        /// Service data as JSON
        #[arg(long)]
        data: Option<String>,
        /// Target entities/areas/devices as JSON
        #[arg(long)]
        target: Option<String>,
    },
}

#[derive(Subcommand)]
enum TemplateAction {
    /// Render a Jinja2 template
    Render {
        /// Template string
        template: String,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Validate Home Assistant configuration
    Check,
    /// Reload configuration (automations, scripts, scenes, all)
    Reload {
        /// Component to reload
        component: Option<String>,
    },
}

#[derive(Subcommand)]
enum HistoryAction {
    /// Get entity state history
    Get {
        /// Entity ID
        entity_id: String,
        /// Hours of history (default: 24)
        #[arg(long, default_value = "24")]
        hours: u32,
    },
}

#[derive(Subcommand)]
enum LogAction {
    /// Show error log entries
    Errors {
        /// Show only last N lines
        #[arg(long)]
        tail: Option<usize>,
        /// Filter by regex pattern
        #[arg(long)]
        pattern: Option<String>,
    },
}

#[derive(Subcommand)]
enum LogbookAction {
    /// Get entity event timeline
    Get {
        /// Entity ID
        entity_id: String,
        /// Hours of history (default: 24)
        #[arg(long, default_value = "24")]
        hours: u32,
    },
}

#[derive(Subcommand)]
enum EventAction {
    /// Fire an event
    Fire {
        /// Event type (e.g., custom_event)
        event_type: String,
        /// Event data as JSON
        #[arg(long)]
        data: Option<String>,
    },
}

#[derive(Subcommand)]
enum AutomationAction {
    /// List all automations
    List,
    /// Trigger an automation
    Trigger {
        /// Automation entity ID
        entity_id: String,
    },
}

#[derive(Subcommand)]
enum ScriptAction {
    /// List all scripts
    List,
    /// Run a script
    Run {
        /// Script entity ID (e.g., script.my_script)
        entity_id: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let mode = OutputMode::auto_detect(cli.human, cli.compact, cli.no_compact);
    let effective_limit = match (cli.limit, mode) {
        (Some(n), _) => Some(n),
        (None, OutputMode::Compact) => Some(50),
        _ => None,
    };

    if let Err(e) = run(cli, mode, effective_limit).await {
        let output = e.to_error_output();
        match serde_json::to_string_pretty(&output) {
            Ok(json) => eprintln!("{json}"),
            Err(_) => {
                // Escape quotes in error message to produce valid JSON
                let msg = e.to_string().replace('\\', "\\\\").replace('"', "\\\"");
                eprintln!("{{\"error\":\"{msg}\",\"code\":\"{}\"}}", e.code());
            }
        }
        std::process::exit(1);
    }
}

async fn run(cli: Cli, mode: OutputMode, limit: Option<usize>) -> Result<(), AppError> {
    // Commands that don't need auth
    match &cli.command {
        Commands::Usage => {
            print_usage();
            return Ok(());
        }
        Commands::Completions { shell } => {
            let completions = commands::completions::generate_completions(shell)?;
            print!("{completions}");
            return Ok(());
        }
        Commands::Validate {
            path,
            check_entities,
            check_registry,
            check_services,
        } => {
            // Validate can run without auth (local files only), but entity/registry/service checks need it
            let needs_auth = *check_entities || *check_registry || *check_services;
            let auth = if needs_auth {
                Some(auth::resolve_auth(
                    cli.url.as_deref(),
                    cli.token.as_deref(),
                )?)
            } else {
                None
            };
            let client = auth.as_ref().map(client::HaClient::new).transpose()?;
            let output = commands::validate::run(
                client.as_ref(),
                path,
                *check_entities,
                *check_registry,
                *check_services,
                auth.as_ref(),
                mode,
            )
            .await?;
            if !output.is_empty() {
                println!("{output}");
            }
            return Ok(());
        }
        _ => {}
    }

    let auth_config = auth::resolve_auth(cli.url.as_deref(), cli.token.as_deref())?;
    let client = client::HaClient::new(&auth_config)?;

    let output = match cli.command {
        Commands::Entities { action } => match action {
            EntityAction::List {
                domain,
                pattern,
                name,
                state,
            } => {
                commands::entities::list(
                    &client,
                    domain.as_deref(),
                    pattern.as_deref(),
                    name.as_deref(),
                    state.as_deref(),
                    mode,
                    limit,
                )
                .await?
            }
            EntityAction::Get { entity_id } => {
                commands::entities::get(&client, &entity_id, mode).await?
            }
            EntityAction::Search { pattern } => {
                commands::entities::search(&client, &pattern, mode, limit).await?
            }
        },
        Commands::Services { action } => match action {
            ServiceAction::List { domain } => {
                commands::services::list(&client, domain.as_deref(), mode).await?
            }
            ServiceAction::Call {
                service,
                data,
                target,
            } => {
                commands::services::call(
                    &client,
                    &service,
                    data.as_deref(),
                    target.as_deref(),
                    mode,
                )
                .await?
            }
        },
        Commands::Templates { action } => match action {
            TemplateAction::Render { template } => {
                commands::templates::render(&client, &template, mode).await?
            }
        },
        Commands::Config { action } => match action {
            ConfigAction::Check => commands::config::check(&client, mode).await?,
            ConfigAction::Reload { component } => {
                commands::config::reload(&client, component.as_deref(), mode).await?
            }
        },
        Commands::History { action } => match action {
            HistoryAction::Get { entity_id, hours } => {
                commands::history::get(&client, &entity_id, hours, mode, limit).await?
            }
        },
        Commands::Logs { action } => match action {
            LogAction::Errors { tail, pattern } => {
                commands::logs::errors(
                    &client,
                    &auth_config.url,
                    &auth_config.token,
                    tail,
                    pattern.as_deref(),
                    mode,
                )
                .await?
            }
        },
        Commands::Logbook { action } => match action {
            LogbookAction::Get { entity_id, hours } => {
                commands::logbook::get(&client, &entity_id, hours, mode, limit).await?
            }
        },
        Commands::Events { action } => match action {
            EventAction::Fire { event_type, data } => {
                commands::events::fire(&client, &event_type, data.as_deref(), mode).await?
            }
        },
        Commands::Automations { action } => match action {
            AutomationAction::List => commands::automations::list(&client, mode, limit).await?,
            AutomationAction::Trigger { entity_id } => {
                commands::automations::trigger(&client, &entity_id, mode).await?
            }
        },
        Commands::Scripts { action } => match action {
            ScriptAction::List => commands::automations::scripts_list(&client, mode, limit).await?,
            ScriptAction::Run { entity_id } => {
                commands::automations::scripts_run(&client, &entity_id, mode).await?
            }
        },
        Commands::Batch { file } => {
            commands::batch::run(&client, &auth_config.url, file.as_deref(), mode).await?
        }
        Commands::Diff { since, domain } => {
            commands::diff::since(&client, since, domain.as_deref(), mode, limit).await?
        }
        Commands::Watch {
            entity_id,
            timeout,
            interval,
            state,
        } => {
            commands::watch::entity(
                &client,
                &entity_id,
                timeout,
                interval,
                state.as_deref(),
                mode,
            )
            .await?
        }
        Commands::Inspect => commands::inspect::triage(&client, mode, limit).await?,
        Commands::Verify { baseline } => {
            commands::verify::check(&client, &auth_config.url, baseline.as_deref(), mode).await?
        }
        Commands::Health => commands::health::check(&client, &auth_config.url, mode).await?,
        Commands::Stats { dashboard, init } => {
            if init {
                commands::stats::init(&client).await?
            } else {
                commands::stats::status(
                    &client,
                    &auth_config.url,
                    &auth_config.token,
                    dashboard.as_deref(),
                    mode,
                )
                .await?
            }
        }
        Commands::Usage | Commands::Completions { .. } | Commands::Validate { .. } => {
            unreachable!()
        }
    };

    if !output.is_empty() {
        println!("{output}");
    }
    Ok(())
}

fn print_usage() {
    print!(
        "homeassist - Home Assistant CLI for LLM agents

QUICK REFERENCE:
  homeassist entities list --domain light
  homeassist entities get light.kitchen
  homeassist services call light.turn_on --data '{{\"entity_id\":\"light.kitchen\"}}'
  homeassist templates render \"{{{{ states('sensor.temp') }}}}\"

ENTITY OPERATIONS:
  homeassist entities list [--domain X] [--pattern X] [--name X]
  homeassist entities get <entity_id>
  homeassist entities search <pattern>

SERVICE OPERATIONS:
  homeassist services list [domain]
  homeassist services call <domain.service> --data '{{...}}'

TEMPLATE OPERATIONS:
  homeassist templates render \"<template>\"

CONFIG OPERATIONS:
  homeassist config check
  homeassist config reload [automations|scripts|scenes|all]

HISTORY:
  homeassist history get <entity_id> [--hours 24]

LOGS:
  homeassist logs errors [--tail 50] [--pattern \"zigbee\"]

LOGBOOK:
  homeassist logbook get <entity_id> [--hours 24]

EVENTS:
  homeassist events fire <event_type> [--data '{{...}}']

AUTOMATIONS:
  homeassist automations list
  homeassist automations trigger <entity_id>

SCRIPTS:
  homeassist scripts list
  homeassist scripts run <entity_id>

BATCH:
  homeassist batch [--file commands.jsonl]
  echo '{{\"command\":\"health\"}}' | homeassist batch

DIFF:
  homeassist diff --since 1 [--domain sensor]

WATCH:
  homeassist watch <entity_id> [--timeout 300] [--interval 2] [--state on]

INSPECT:
  homeassist inspect

VALIDATE:
  homeassist validate ./packages
  homeassist validate . --check-entities

VERIFY:
  homeassist verify
  homeassist verify --baseline snapshot.json

COMPLETIONS:
  homeassist completions bash >> ~/.bashrc
  homeassist completions zsh >> ~/.zshrc
  homeassist completions fish > ~/.config/fish/completions/homeassist.fish

HEALTH:
  homeassist health

STATS:
  homeassist stats
  homeassist stats --init > ~/.config/homeassist/dashboard.yaml

OUTPUT: JSON default, --human for readable, --compact for LLM token savings
AUTH: HA_URL + HA_TOKEN env vars, or --url/--token flags
"
    );
}
