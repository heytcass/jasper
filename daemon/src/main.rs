use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use std::sync::Arc;
use parking_lot::RwLock;

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
mod desktop_detection;

// New command dispatcher - gradually replacing trait-based commands
#[cfg(feature = "new-commands")]
mod command_dispatcher;

// New config system - gradually replacing Arc<RwLock<Config>>
#[cfg(feature = "new-config")]
mod config_v2;

#[cfg(not(feature = "new-config"))]
use config::Config;
use database::DatabaseInner;
use commands::{
    Command, CommandContext,
    auth::{AuthGoogleCommand, SetApiKeyCommand},
    calendar::{SyncTestCommand, TestCalendarCommand, AddTestEventsCommand, CleanDatabaseCommand},
    daemon_ops::{StartCommand, StatusCommand, StopCommand, ClearCacheCommand, TestNotificationCommand},
    waybar::WaybarCommand,
};
use desktop_detection::DesktopDetector;

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
    /// Detect desktop environment and components
    DetectDesktop,
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
    #[cfg(not(feature = "new-config"))]
    let config = Config::load().await
        .context("Failed to load application configuration")?;
    
    #[cfg(feature = "new-config")]
    {
        // Initialize the static config
        config_v2::init_config().await
            .context("Failed to initialize configuration")?;
    }
    
    info!("Configuration loaded successfully");

    // Initialize database
    #[cfg(not(feature = "new-config"))]
    let db_path = config.read().get_database_path()?;
    #[cfg(feature = "new-config")]
    let db_path = config_v2::config().get_database_path()?;
    
    let database = DatabaseInner::new(&db_path).await
        .with_context(|| format!("Failed to initialize database at {:?}", db_path))?;
    info!("Database initialized");

    // Create config wrapper for compatibility
    #[cfg(not(feature = "new-config"))]
    let config = Arc::new(RwLock::new(config));
    #[cfg(feature = "new-config")]
    let config = Arc::new(RwLock::new(config_v2::config().as_ref().clone()));

    // Initialize correlation engine
    let correlation_engine = correlation_engine::CorrelationEngine::new(database.clone(), config.clone());

    // Create shared command context
    let context = CommandContext::new(
        config.clone(),
        database.clone(),
        correlation_engine.clone(),
        cli.debug,
        cli.test_mode,
    );

    // Execute the appropriate command
    #[cfg(feature = "new-commands")]
    {
        use command_dispatcher::{CommandV2, ExecutionContext};
        
        let command_v2 = match cli.command.unwrap_or(Commands::Start) {
            Commands::Start => CommandV2::Start,
            Commands::Status => CommandV2::Status,
            Commands::Stop => CommandV2::Stop,
            Commands::AuthGoogle => CommandV2::AuthGoogle,
            Commands::TestCalendar => CommandV2::TestCalendar,
            Commands::SyncTest => CommandV2::SyncTest,
            Commands::CleanDatabase => CommandV2::CleanDatabase,
            Commands::SetApiKey { key } => CommandV2::SetApiKey { key },
            Commands::Waybar { simple } => CommandV2::Waybar { simple },
            Commands::AddTestEvents => CommandV2::AddTestEvents,
            Commands::ClearCache => CommandV2::ClearCache,
            Commands::TestNotification => CommandV2::TestNotification,
            Commands::DetectDesktop => CommandV2::DetectDesktop,
        };
        
        let exec_context = ExecutionContext::new(
            config,
            database,
            correlation_engine,
            cli.debug,
            cli.test_mode,
        );
        
        command_v2.execute(&exec_context).await
            .context("Failed to execute command")?;
    }
    
    #[cfg(feature = "legacy-commands")]
    #[cfg(not(feature = "new-commands"))]
    {
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
            Commands::DetectDesktop => Box::new(DetectDesktopCommand),
        };

        command.execute(&context).await
            .context("Failed to execute command")?;
    }

    Ok(())
}

// Simple inline command for desktop detection testing
struct DetectDesktopCommand;

#[async_trait::async_trait]
impl Command for DetectDesktopCommand {
    async fn execute(&mut self, _context: &CommandContext) -> Result<()> {
        let mut detector = DesktopDetector::new();
        
        match detector.detect() {
            Ok(context) => {
                println!("Desktop Environment Detection Results:");
                println!("=====================================");
                println!("Primary Desktop: {}", context.primary_de.name());
                println!("Session Type: {}", context.session_type.name());
                println!("Supports Extensions: {}", context.primary_de.supports_extensions());
                println!("Typical Status Bar: {:?}", context.primary_de.typical_status_bar());
                println!();
                println!("Available Components:");
                println!("- Waybar: {}", context.available_components.waybar);
                println!("- GNOME Shell: {}", context.available_components.gnome_shell);
                println!("- KDE Plasma: {}", context.available_components.kde_plasma);
                println!("- Mako: {}", context.available_components.mako);
                println!("- Dunst: {}", context.available_components.dunst);
                println!();
                println!("Available Status Bars: {:?}", context.available_components.available_status_bars());
                println!("Available Notification Services: {:?}", context.available_components.available_notification_services());
                println!();
                println!("Recommended Notification Service: {}", context.notification_service.name());
                println!();
                println!("Summary: {}", context.summary());
                
                // Test refresh functionality
                println!();
                println!("Testing refresh functionality...");
                if let Ok(refreshed_context) = detector.refresh() {
                    println!("Refresh successful: {}", refreshed_context.summary());
                } else {
                    println!("Refresh failed");
                }
            }
            Err(e) => {
                eprintln!("Desktop detection failed: {}", e);
                return Err(e);
            }
        }
        
        Ok(())
    }
}

