use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod database;
mod dbus_service;
mod correlation_engine;
mod test_data;
mod data_sanitizer;
mod api_manager;
mod google_calendar;
mod calendar_sync;
mod waybar_formatter;
mod http_utils;
mod commands;
mod services;
mod errors;
mod sops_integration;
mod context_sources;
mod error_recovery;
mod frontend_framework;
mod formatters;
mod frontend_manager;

use config::Config;
use database::DatabaseInner;
use commands::{
    Command, CommandContext,
    auth::{AuthGoogleCommand, SetApiKeyCommand},
    calendar::{SyncTestCommand, TestCalendarCommand, AddTestEventsCommand, CleanDatabaseCommand},
    daemon_ops::{StartCommand, StatusCommand, StopCommand, ClearCacheCommand, TestNotificationCommand},
    waybar::WaybarCommand,
};

#[derive(Parser)]
#[command(name = "jasper-companion-daemon")]
#[command(about = "Jasper Companion - Personal Digital Assistant Daemon")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    
    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
    
    /// Test mode - don't start as daemon
    #[arg(long)]
    test_mode: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the daemon
    Start,
    /// Check daemon status
    Status,
    /// Stop the daemon
    Stop,
    /// Setup Google Calendar integration
    AuthGoogle,
    /// Test Google Calendar API integration
    TestCalendar,
    /// Test calendar sync
    SyncTest,
    /// Clean test data from database
    CleanDatabase,
    /// Set Claude API key in configuration
    SetApiKey {
        /// The Claude API key from console.anthropic.com
        key: String,
    },
    /// Output insights in Waybar JSON format
    Waybar {
        /// Output only the most urgent insight as simple text
        #[arg(long)]
        simple: bool,
    },
    /// Add test events for demonstration
    AddTestEvents,
    /// Clear AI cache and context state
    ClearCache,
    /// Test the notification system
    TestNotification,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    let log_level = if cli.debug { "debug" } else { "info" };
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("jasper_companion_daemon={}", log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Jasper Companion Daemon starting up");

    // Load configuration
    let config = Config::load().await?;
    info!("Configuration loaded successfully");

    // Initialize database
    let db_path = config.read().get_database_path()?;
    let database = DatabaseInner::new(&db_path).await?;
    info!("Database initialized");

    // Initialize correlation engine
    let correlation_engine = correlation_engine::CorrelationEngine::new(database.clone(), config.clone());

    // Create shared command context
    let context = CommandContext::new(
        config,
        database,
        correlation_engine,
        cli.debug,
        cli.test_mode,
    );

    // Execute the appropriate command
    let mut command: Box<dyn Command> = match cli.command.unwrap_or(Commands::Start) {
        Commands::Start => Box::new(StartCommand),
        Commands::Status => Box::new(StatusCommand),
        Commands::Stop => Box::new(StopCommand),
        Commands::AuthGoogle => Box::new(AuthGoogleCommand),
        Commands::TestCalendar => Box::new(TestCalendarCommand),
        Commands::SyncTest => Box::new(SyncTestCommand),
        Commands::CleanDatabase => Box::new(CleanDatabaseCommand),
        Commands::SetApiKey { key } => Box::new(SetApiKeyCommand { api_key: key }),
        Commands::Waybar { simple } => Box::new(WaybarCommand { simple }),
        Commands::AddTestEvents => Box::new(AddTestEventsCommand),
        Commands::ClearCache => Box::new(ClearCacheCommand),
        Commands::TestNotification => Box::new(TestNotificationCommand),
    };

    command.execute(&context).await?;

    Ok(())
}

