use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use tracing::{info, warn, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Only import what we need for the simplified architecture
mod config;
mod database;
mod errors;
mod api_manager;
mod google_calendar;
mod http_utils;
mod context_sources;
mod sops_integration;
mod significance_engine;
mod new_daemon_core;
mod new_dbus_service;
mod waybar_adapter;

use config::Config;
use database::DatabaseInner;
use new_daemon_core::SimplifiedDaemonCore;
use new_dbus_service::SimplifiedDbusService;
use context_sources::ContextSourceManager;
use context_sources::weather::WeatherContextSource;
use google_calendar::GoogleCalendarService;
use api_manager::ApiManager;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Parser)]
#[command(name = "jasper-daemon")]
#[command(about = "Jasper Simplified Daemon - Backend AI Analysis Only")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    
    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the daemon
    Start,
    /// Check daemon status
    Status,
    /// Stop the daemon (via D-Bus)
    Stop,
    /// Set Claude API key in configuration
    SetApiKey {
        /// The Claude API key from console.anthropic.com
        key: String,
    },
    /// Get insights for waybar
    Waybar,
    /// Check waybar status
    WaybarStatus,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.debug { "debug" } else { "info" };
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("jasper_companion_daemon={},warn", log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    match cli.command.unwrap_or(Commands::Start) {
        Commands::Start => start_daemon().await,
        Commands::Status => show_status().await,
        Commands::Stop => stop_daemon().await,
        Commands::SetApiKey { key } => set_api_key(key).await,
        Commands::Waybar => waybar_mode().await,
        Commands::WaybarStatus => waybar_status_mode().await,
    }
}

async fn start_daemon() -> Result<()> {
    info!("Starting Jasper simplified daemon");

    // Load configuration
    let config_arc = Config::load().await.context("Failed to load configuration")?;

    // Initialize database
    let db_path = Config::get_data_dir()?.join("jasper.db");
    let database = DatabaseInner::new(&db_path).await
        .context("Failed to initialize database")?;

    // Initialize context source manager
    let mut context_manager = ContextSourceManager::new();

    // Register weather context source if configured
    {
        let config = config_arc.read();
        if let Some(weather_config) = config.get_weather_config() {
            if weather_config.enabled && !weather_config.api_key.is_empty() {
                let weather_source = WeatherContextSource::with_config(
                    Some(weather_config.api_key.clone()),
                    weather_config.location.clone(),
                    weather_config.units.clone(),
                    weather_config.cache_duration_minutes,
                );
                context_manager.add_source(Box::new(weather_source));
                info!("Weather context source registered for {}", weather_config.location);
            }
        }
    }

    // Initialize Google Calendar service if configured
    let calendar_service = {
        let config = config_arc.read();
        config.google_calendar.as_ref().and_then(|gc| {
            if gc.enabled && !gc.client_id.is_empty() && !gc.client_secret.is_empty() {
                let gcal_config = google_calendar::GoogleCalendarConfig {
                    client_id: gc.client_id.clone(),
                    client_secret: gc.client_secret.clone(),
                    redirect_uri: gc.redirect_uri.clone(),
                    calendar_ids: gc.calendar_ids.clone(),
                };
                let data_dir = Config::get_data_dir().ok()?;
                info!("Google Calendar service initialized");
                Some(GoogleCalendarService::new(gcal_config, data_dir))
            } else {
                None
            }
        })
    };

    // Initialize API manager
    let api_manager = ApiManager::new();

    // Create the simplified daemon core
    let daemon_core = Arc::new(RwLock::new(
        SimplifiedDaemonCore::new(database, context_manager, api_manager, config_arc, calendar_service)
    ));
    
    info!("Simplified daemon core created");
    
    // Start D-Bus service in background
    let dbus_daemon = daemon_core.clone();
    let dbus_handle = tokio::spawn(async move {
        if let Err(e) = SimplifiedDbusService::start(dbus_daemon).await {
            error!("D-Bus service failed: {}", e);
        }
    });

    // Give D-Bus service time to establish connection
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Initialize the D-Bus signal emitter for push notifications to frontends
    {
        let daemon = daemon_core.read().await;
        if let Err(e) = daemon.init_signal_emitter().await {
            warn!("Could not initialize signal emitter (frontends will use polling): {}", e);
        }
    }

    // Start the main daemon loop in a separate task
    // Using start_with_arc to avoid holding lock for entire runtime
    let daemon_core_clone = daemon_core.clone();
    let daemon_handle = tokio::spawn(async move {
        SimplifiedDaemonCore::start_with_arc(daemon_core_clone).await
    });
    
    // Wait for either the daemon or D-Bus service to finish
    tokio::select! {
        result = daemon_handle => {
            if let Err(e) = result {
                error!("Daemon loop failed: {}", e);
            }
        }
        _ = dbus_handle => {
            info!("D-Bus service stopped");
        }
    }
    
    info!("Simplified daemon stopped");
    Ok(())
}

async fn show_status() -> Result<()> {
    let connection = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(_) => {
            println!("Daemon Status: Not Running");
            println!("  D-Bus session bus unavailable");
            return Ok(());
        }
    };

    match connection
        .call_method(
            Some("org.jasper.Daemon"),
            "/org/jasper/Daemon",
            Some("org.jasper.Daemon1"),
            "GetStatus",
            &(),
        )
        .await
    {
        Ok(reply) => {
            let (is_running, active_frontends, insights_count): (bool, u32, i64) =
                reply.body().deserialize()?;
            println!("Daemon Status: {}", if is_running { "Running" } else { "Stopped" });
            println!("  Active frontends: {}", active_frontends);
            println!("  Total insights:   {}", insights_count);
        }
        Err(_) => {
            println!("Daemon Status: Not Running");
        }
    }
    Ok(())
}

async fn stop_daemon() -> Result<()> {
    let connection = zbus::Connection::session().await
        .context("Failed to connect to D-Bus session bus")?;

    match connection
        .call_method(
            Some("org.jasper.Daemon"),
            "/org/jasper/Daemon",
            Some("org.jasper.Daemon1"),
            "GetStatus",
            &(),
        )
        .await
    {
        Ok(_) => {
            println!("Daemon is running. Use 'systemctl --user stop jasper-daemon' or send SIGTERM.");
        }
        Err(_) => {
            println!("Daemon is not running.");
        }
    }
    Ok(())
}

async fn set_api_key(key: String) -> Result<()> {
    let config_arc = Config::load().await.context("Failed to load configuration")?;
    {
        let mut config = config_arc.write();
        config.ai.api_key = Some(key);
    }
    // Save after releasing the lock
    let config = config_arc.read();
    config.save().await.context("Failed to save configuration")?;
    
    println!("Claude API key updated successfully");
    Ok(())
}

async fn waybar_mode() -> Result<()> {
    waybar_adapter::run_waybar_mode().await
        .map_err(|e| anyhow::anyhow!("Waybar mode failed: {}", e))
}

async fn waybar_status_mode() -> Result<()> {
    waybar_adapter::waybar_status().await
        .map_err(|e| anyhow::anyhow!("Waybar status failed: {}", e))
}