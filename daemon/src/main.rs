use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Only import what we need for the simplified architecture
mod api_manager;
mod config;
mod context_sources;
mod database;
mod errors;
mod google_calendar;
mod http_utils;
mod new_daemon_core;
mod new_dbus_service;
mod noctalia_adapter;
mod significance_engine;
mod sops_integration;
mod waybar_adapter;

use api_manager::ApiManager;
use config::Config;
use context_sources::weather::WeatherContextSource;
use context_sources::ContextSourceManager;
use database::DatabaseInner;
use google_calendar::GoogleCalendarService;
use new_daemon_core::SimplifiedDaemonCore;
use new_dbus_service::SimplifiedDbusService;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
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
    /// Get insights for Noctalia bar widget (JSON output)
    Noctalia,
    /// Force refresh and get insights for Noctalia
    NoctaliaRefresh,
    /// Authenticate with Google Calendar (OAuth2 flow)
    AuthGoogle,
    /// List Google Calendars and choose which ones to sync
    ListCalendars,
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
        Commands::Noctalia => noctalia_mode().await,
        Commands::NoctaliaRefresh => noctalia_refresh_mode().await,
        Commands::AuthGoogle => auth_google().await,
        Commands::ListCalendars => list_calendars().await,
    }
}

async fn start_daemon() -> Result<()> {
    info!("Starting Jasper simplified daemon");

    // Load configuration
    let config_arc = Config::load()
        .await
        .context("Failed to load configuration")?;

    // Initialize database
    let db_path = Config::get_data_dir()?.join("jasper.db");
    let database = DatabaseInner::new(&db_path)
        .await
        .context("Failed to initialize database")?;

    // Initialize context source manager
    let mut context_manager = ContextSourceManager::new();

    // Register weather context source if configured
    {
        let config = config_arc.read();
        if let Some(weather_config) = config.get_weather_config() {
            if weather_config.enabled && !weather_config.google_api_key.is_empty() {
                let weather_source = WeatherContextSource::new(
                    weather_config.google_api_key.clone(),
                    weather_config.latitude,
                    weather_config.longitude,
                    weather_config.units.clone(),
                    weather_config.cache_duration_minutes,
                );
                context_manager.add_source(Box::new(weather_source));
                info!(
                    "Weather context source registered ({}, {})",
                    weather_config.latitude, weather_config.longitude
                );
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
                let tz = config.get_timezone();
                info!("Google Calendar service initialized");
                Some(GoogleCalendarService::new(gcal_config, data_dir, tz))
            } else {
                None
            }
        })
    };

    // Initialize API manager
    let api_manager = ApiManager::new();

    // Create the simplified daemon core
    let daemon_core = Arc::new(RwLock::new(SimplifiedDaemonCore::new(
        database,
        context_manager,
        api_manager,
        config_arc,
        calendar_service,
    )));

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
            warn!(
                "Could not initialize signal emitter (frontends will use polling): {}",
                e
            );
        }
    }

    // Start the main daemon loop in a separate task
    // Using start_with_arc to avoid holding lock for entire runtime
    let daemon_core_clone = daemon_core.clone();
    let daemon_handle =
        tokio::spawn(async move { SimplifiedDaemonCore::start_with_arc(daemon_core_clone).await });

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
            println!(
                "Daemon Status: {}",
                if is_running { "Running" } else { "Stopped" }
            );
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
    let connection = zbus::Connection::session()
        .await
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
            println!(
                "Daemon is running. Use 'systemctl --user stop jasper-daemon' or send SIGTERM."
            );
        }
        Err(_) => {
            println!("Daemon is not running.");
        }
    }
    Ok(())
}

async fn set_api_key(key: String) -> Result<()> {
    let config_arc = Config::load()
        .await
        .context("Failed to load configuration")?;
    {
        let mut config = config_arc.write();
        config.ai.api_key = Some(key);
    }
    // Clone config data before awaiting to avoid holding lock across await
    let config = config_arc.read().clone();
    config
        .save()
        .await
        .context("Failed to save configuration")?;

    println!("Claude API key updated successfully");
    Ok(())
}

async fn waybar_mode() -> Result<()> {
    waybar_adapter::run_waybar_mode()
        .await
        .map_err(|e| anyhow::anyhow!("Waybar mode failed: {}", e))
}

async fn waybar_status_mode() -> Result<()> {
    waybar_adapter::waybar_status()
        .await
        .map_err(|e| anyhow::anyhow!("Waybar status failed: {}", e))
}

async fn noctalia_mode() -> Result<()> {
    noctalia_adapter::run_noctalia_mode()
        .await
        .map_err(|e| anyhow::anyhow!("Noctalia mode failed: {}", e))
}

async fn noctalia_refresh_mode() -> Result<()> {
    noctalia_adapter::run_noctalia_refresh()
        .await
        .map_err(|e| anyhow::anyhow!("Noctalia refresh failed: {}", e))
}

async fn auth_google() -> Result<()> {
    let config_arc = Config::load()
        .await
        .context("Failed to load configuration")?;

    let gc = {
        let config = config_arc.read();
        config.google_calendar.as_ref()
            .filter(|gc| gc.enabled && !gc.client_id.is_empty() && !gc.client_secret.is_empty())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!(
                "Google Calendar is not configured. Set enabled=true, client_id, and client_secret in [google_calendar] section of config.toml"
            ))?
    };

    let gcal_config = google_calendar::GoogleCalendarConfig {
        client_id: gc.client_id,
        client_secret: gc.client_secret,
        redirect_uri: gc.redirect_uri.clone(),
        calendar_ids: gc.calendar_ids,
    };
    let data_dir = Config::get_data_dir()?;
    let tz = config_arc.read().get_timezone();
    let service = GoogleCalendarService::new(gcal_config, data_dir, tz);

    // Check if already authenticated
    if service.is_authenticated().await {
        println!("Already authenticated with Google Calendar.");
        println!("To re-authenticate, delete the token file and run this command again:");
        println!("  rm ~/.local/share/jasper-companion/google_calendar_token.json");
        return Ok(());
    }

    let (auth_url, csrf_token) = service
        .get_auth_url()
        .context("Failed to generate OAuth URL")?;

    println!("Open this URL in your browser to authorize Jasper:\n");
    println!("  {}\n", auth_url);
    println!("Waiting for callback on {} ...", gc.redirect_uri);

    // Try to open the URL automatically
    let _ = std::process::Command::new("xdg-open")
        .arg(&auth_url)
        .spawn();

    // Start one-shot callback server on 127.0.0.1:8080
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .context("Failed to bind to 127.0.0.1:8080 - is another process using this port?")?;

    let (mut stream, _addr) = listener
        .accept()
        .await
        .context("Failed to accept callback connection")?;

    // Read the HTTP request
    let mut buf = vec![0u8; 4096];
    let n = tokio::io::AsyncReadExt::read(&mut stream, &mut buf)
        .await
        .context("Failed to read callback request")?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse the request line to extract query params: GET /auth/callback?code=...&state=... HTTP/1.1
    let code = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1)) // "/auth/callback?code=...&state=..."
        .and_then(|path| path.split('?').nth(1)) // "code=...&state=..."
        .and_then(|query| {
            query
                .split('&')
                .find_map(|param| param.strip_prefix("code="))
        })
        .map(|c| c.to_string())
        .ok_or_else(|| anyhow::anyhow!("No authorization code found in callback"))?;

    // Send success response to browser
    let html = "<html><body><h2>Authentication successful!</h2><p>You can close this tab and return to the terminal.</p></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;

    // Exchange the code for a token
    println!("Authorization code received, exchanging for token...");
    service
        .authenticate_with_code(&code, csrf_token.secret())
        .await
        .context("Failed to exchange authorization code for token")?;

    println!("Google Calendar authentication successful!");
    println!("Token saved. Restart the daemon to begin syncing calendar events.");
    Ok(())
}

async fn list_calendars() -> Result<()> {
    let config_arc = Config::load()
        .await
        .context("Failed to load configuration")?;

    let gc = {
        let config = config_arc.read();
        config.google_calendar.as_ref()
            .filter(|gc| gc.enabled && !gc.client_id.is_empty() && !gc.client_secret.is_empty())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!(
                "Google Calendar is not configured. Set enabled=true, client_id, and client_secret in [google_calendar] section of config.toml"
            ))?
    };

    let gcal_config = google_calendar::GoogleCalendarConfig {
        client_id: gc.client_id,
        client_secret: gc.client_secret,
        redirect_uri: gc.redirect_uri,
        calendar_ids: gc.calendar_ids.clone(),
    };
    let synced_ids: std::collections::HashSet<String> = gc.calendar_ids.into_iter().collect();

    let data_dir = Config::get_data_dir()?;
    let tz = config_arc.read().get_timezone();
    let service = GoogleCalendarService::new(gcal_config, data_dir, tz);

    println!("Fetching calendars from Google...");
    let calendars = service.fetch_calendar_list().await.context(
        "Failed to fetch calendar list. Are you authenticated? Run 'auth-google' first.",
    )?;

    if calendars.is_empty() {
        println!("No calendars found on this Google account.");
        return Ok(());
    }

    // Build display labels and figure out which are pre-selected
    let labels: Vec<String> = calendars
        .iter()
        .map(|cal| {
            let name = cal.summary.as_deref().unwrap_or("(unnamed)");
            if cal.primary.unwrap_or(false) {
                format!("{} (primary)", name)
            } else {
                name.to_string()
            }
        })
        .collect();

    let defaults: Vec<bool> = calendars
        .iter()
        .map(|cal| {
            synced_ids.contains(&cal.id)
                || (cal.primary.unwrap_or(false) && synced_ids.contains("primary"))
        })
        .collect();

    // Interactive checkbox selector
    let selections = dialoguer::MultiSelect::new()
        .with_prompt("Select calendars to sync (space to toggle, enter to confirm)")
        .items(&labels)
        .defaults(&defaults)
        .interact_opt()
        .context("Calendar selection cancelled")?;

    let Some(selections) = selections else {
        println!("Cancelled, no changes made.");
        return Ok(());
    };

    // Map selected indices back to calendar IDs
    let new_calendar_ids: Vec<String> = selections
        .iter()
        .map(|&i| {
            let cal = &calendars[i];
            if cal.primary.unwrap_or(false) {
                "primary".to_string()
            } else {
                cal.id.clone()
            }
        })
        .collect();

    // Check if anything actually changed
    let old_set: std::collections::HashSet<&str> = synced_ids.iter().map(|s| s.as_str()).collect();
    let new_set: std::collections::HashSet<&str> =
        new_calendar_ids.iter().map(|s| s.as_str()).collect();
    if old_set == new_set {
        println!("No changes made.");
        return Ok(());
    }

    // Show what changed
    for id in new_set.difference(&old_set) {
        let name = calendars
            .iter()
            .find(|c| c.id == *id || (*id == "primary" && c.primary.unwrap_or(false)))
            .and_then(|c| c.summary.as_deref())
            .unwrap_or(id);
        println!("  + {}", name);
    }
    for id in old_set.difference(&new_set) {
        let name = calendars
            .iter()
            .find(|c| c.id == *id || (*id == "primary" && c.primary.unwrap_or(false)))
            .and_then(|c| c.summary.as_deref())
            .unwrap_or(id);
        println!("  - {}", name);
    }

    // Save to config
    {
        let mut config = config_arc.write();
        if let Some(ref mut gc) = config.google_calendar {
            gc.calendar_ids = new_calendar_ids;
        }
    }
    let config = config_arc.read().clone();
    config
        .save()
        .await
        .context("Failed to save configuration")?;

    println!("\nConfiguration saved. Restart the daemon to apply changes.");
    Ok(())
}
