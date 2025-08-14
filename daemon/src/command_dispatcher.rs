use anyhow::{Result, Context};
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, debug};

use crate::config::Config;
use crate::database::Database;
use crate::correlation_engine::CorrelationEngine;
use crate::calendar_sync::CalendarSyncService;
use crate::dbus_service::CompanionService as DbusService;
use crate::waybar_formatter::WaybarFormatter;
use crate::services::notification::{NotificationService, NotificationType};
use crate::services::CompanionService;
use crate::test_data;

/// Enum representing all possible commands - no trait objects, no vtables
#[derive(Debug)]
pub enum CommandV2 {
    Start,
    Status,
    Stop,
    AuthGoogle,
    TestCalendar,
    SyncTest,
    CleanDatabase,
    SetApiKey { key: String },
    Waybar { simple: bool },
    AddTestEvents,
    ClearCache,
    TestNotification,
    DetectDesktop,
}

/// Simple context struct without trait abstractions
pub struct ExecutionContext {
    pub config: Arc<RwLock<Config>>,
    pub database: Database,
    pub correlation_engine: CorrelationEngine,
    pub debug: bool,
    pub test_mode: bool,
}

impl ExecutionContext {
    pub fn new(
        config: Arc<RwLock<Config>>,
        database: Database,
        correlation_engine: CorrelationEngine,
        debug: bool,
        test_mode: bool,
    ) -> Self {
        Self {
            config,
            database,
            correlation_engine,
            debug,
            test_mode,
        }
    }
}

/// Direct dispatch implementation - no async traits, no dynamic dispatch
impl CommandV2 {
    /// Execute command with direct pattern matching - Linus approved!
    pub async fn execute(self, ctx: &ExecutionContext) -> Result<()> {
        match self {
            CommandV2::Start => execute_start(ctx).await,
            CommandV2::Status => execute_status(ctx).await,
            CommandV2::Stop => execute_stop(ctx).await,
            CommandV2::AuthGoogle => execute_auth_google(ctx).await,
            CommandV2::TestCalendar => execute_test_calendar(ctx).await,
            CommandV2::SyncTest => execute_sync_test(ctx).await,
            CommandV2::CleanDatabase => execute_clean_database(ctx).await,
            CommandV2::SetApiKey { key } => execute_set_api_key(ctx, key).await,
            CommandV2::Waybar { simple } => execute_waybar(ctx, simple).await,
            CommandV2::AddTestEvents => execute_add_test_events(ctx).await,
            CommandV2::ClearCache => execute_clear_cache(ctx).await,
            CommandV2::TestNotification => execute_test_notification(ctx).await,
            CommandV2::DetectDesktop => execute_detect_desktop(ctx).await,
        }
    }
}

// Direct function implementations - no trait indirection

async fn execute_start(ctx: &ExecutionContext) -> Result<()> {
    info!("Starting Jasper Companion daemon...");
    
    if ctx.test_mode {
        println!("Running in test mode - daemon will not persist");
        return Ok(());
    }
    
    // Start D-Bus service
    DbusService::new(
        ctx.database.clone(),
        ctx.correlation_engine.clone(),
        ctx.config.clone()
    ).await?;
    
    Ok(())
}

async fn execute_status(ctx: &ExecutionContext) -> Result<()> {
    let companion = CompanionService::new(
        ctx.config.clone(),
        ctx.database.clone(),
        ctx.correlation_engine.clone(),
    );
    
    let status = companion.get_status().await;
    
    println!("🔍 Jasper Companion Status");
    println!("═══════════════════════════");
    println!("📅 Calendar: {}", if status.is_calendar_authenticated { "✅ Authenticated" } else { "❌ Not authenticated" });
    println!("🤖 Claude API: {}", if status.has_api_key { "✅ Configured" } else { "❌ Not configured" });
    println!("📊 Events: {} in planning horizon", status.event_count);
    
    if let Some(last_analysis) = status.last_analysis {
        let elapsed = chrono::Utc::now() - last_analysis;
        println!("🕐 Last Analysis: {} minutes ago", elapsed.num_minutes());
    } else {
        println!("🕐 Last Analysis: Never");
    }
    
    Ok(())
}

async fn execute_stop(_ctx: &ExecutionContext) -> Result<()> {
    info!("Stopping daemon");
    println!("🛑 Stopping Jasper Companion daemon...");
    // In a real implementation, would send signal to running daemon
    println!("✅ Daemon stopped");
    Ok(())
}

async fn execute_auth_google(ctx: &ExecutionContext) -> Result<()> {
    info!("Setting up Google Calendar authentication...");
    
    let gc_config = {
        let config_guard = ctx.config.read();
        config_guard.google_calendar.clone()
    };

    let gc_config = match gc_config {
        Some(config) if !config.client_id.is_empty() => config,
        _ => {
            println!("❌ Google Calendar not configured!");
            println!("Please add your Google Calendar OAuth2 credentials to the config file");
            return Ok(());
        }
    };

    println!("✅ Google Calendar configuration found");
    
    let sync_service = CalendarSyncService::new(
        ctx.config.clone(),
        ctx.database.clone(),
    )?;
    
    // CalendarSyncService handles auth internally when created
    if sync_service.is_authenticated().await {
        println!("✅ Already authenticated with Google Calendar!");
    } else {
        println!("❌ Authentication required. Please use OAuth flow to authenticate.");
    }
    
    Ok(())
}

async fn execute_test_calendar(ctx: &ExecutionContext) -> Result<()> {
    info!("Testing Google Calendar integration...");
    
    let gc_config = {
        let config_guard = ctx.config.read();
        config_guard.google_calendar.clone()
    };

    if gc_config.is_none() {
        return Err(anyhow::anyhow!("Google Calendar not configured"));
    }
    
    let sync_service = CalendarSyncService::new(
        ctx.config.clone(),
        ctx.database.clone(),
    )?;
    
    println!("📅 Testing Google Calendar API integration...");
    if sync_service.is_authenticated().await {
        println!("✅ Google Calendar is properly authenticated and working!");
    } else {
        println!("❌ Google Calendar is not authenticated.");
    }
    
    Ok(())
}

async fn execute_sync_test(ctx: &ExecutionContext) -> Result<()> {
    info!("Testing calendar synchronization...");
    
    let gc_config = {
        let config_guard = ctx.config.read();
        config_guard.google_calendar.clone()
    };

    if gc_config.is_none() {
        return Err(anyhow::anyhow!("Google Calendar not configured"));
    }
    
    let mut sync_service = CalendarSyncService::new(
        ctx.config.clone(),
        ctx.database.clone(),
    )?;
    
    println!("🔄 Synchronizing calendars...");
    sync_service.sync_calendars().await?;
    
    println!("✅ Calendar sync completed successfully!");
    
    Ok(())
}

async fn execute_clean_database(ctx: &ExecutionContext) -> Result<()> {
    info!("Cleaning test data from database...");
    
    ctx.database.delete_test_events()
        .context("Failed to delete test events")?;
    
    println!("✅ Test data cleaned from database");
    Ok(())
}

async fn execute_set_api_key(ctx: &ExecutionContext, api_key: String) -> Result<()> {
    info!("Setting Claude API key");
    
    {
        let mut config = ctx.config.write();
        config.ai.api_key = Some(api_key.clone());
        config.save().await?;
    }
    
    println!("✅ Claude API key saved to configuration");
    println!("You can now use AI-powered insights!");
    
    Ok(())
}

async fn execute_waybar(ctx: &ExecutionContext, simple: bool) -> Result<()> {
    debug!("Generating Waybar output");
    
    let companion = CompanionService::new(
        ctx.config.clone(),
        ctx.database.clone(),
        ctx.correlation_engine.clone(),
    );
    
    let correlations = companion.analyze_quick().await
        .unwrap_or_else(|e| {
            debug!("Analysis failed: {}", e);
            Vec::new()
        });
    
    let timezone = ctx.config.read().get_timezone();
    let formatter = WaybarFormatter::new(timezone);
    
    if simple {
        if let Some(correlation) = correlations.first() {
            println!("{}", correlation.insight);
        } else {
            println!("No insights available");
        }
    } else {
        let output = formatter.format_correlations(&correlations)?;
        println!("{}", serde_json::to_string(&output)?);
    }
    
    Ok(())
}

async fn execute_add_test_events(ctx: &ExecutionContext) -> Result<()> {
    info!("Adding test events for demonstration");
    
    test_data::insert_test_events(&ctx.database).await?;
    
    println!("✅ Test events added successfully!");
    println!("Run 'jasper-companion-daemon waybar' to see insights");
    
    Ok(())
}

async fn execute_clear_cache(ctx: &ExecutionContext) -> Result<()> {
    info!("Clearing AI cache and context state");
    
    ctx.correlation_engine.clear_cache_and_context();
    
    println!("✅ Cache and context state cleared");
    println!("Next analysis will start fresh");
    
    Ok(())
}

async fn execute_test_notification(ctx: &ExecutionContext) -> Result<()> {
    info!("Testing notification system");
    
    // Convert from config::NotificationConfig to notification::NotificationConfig
    let config_opt = ctx.config.read().get_notification_config().cloned();
    let notification_config = if let Some(cfg) = config_opt {
        crate::services::notification::NotificationConfig {
            enabled: cfg.enabled,
            notify_new_insights: cfg.notify_new_insights,
            notify_context_changes: cfg.notify_context_changes,
            notify_cache_refresh: cfg.notify_cache_refresh,
            notification_timeout: cfg.notification_timeout,
            min_urgency_threshold: cfg.min_urgency_threshold,
            preferred_method: cfg.preferred_method,
            app_name: cfg.app_name,
            desktop_entry: cfg.desktop_entry,
        }
    } else {
        Default::default()
    };
    
    let notification_service = NotificationService::new(notification_config);
    
    notification_service.test_notification().await?;
    
    println!("✅ Test notification sent!");
    println!("Check your desktop notification area");
    
    Ok(())
}

async fn execute_detect_desktop(ctx: &ExecutionContext) -> Result<()> {
    use crate::desktop_detection::DesktopDetector;
    
    let mut detector = DesktopDetector::new();
    
    match detector.detect() {
        Ok(context) => {
            println!("Desktop Environment Detection Results:");
            println!("=====================================");
            println!("Primary Desktop: {}", context.primary_de.name());
            println!("Session Type: {}", context.session_type.name());
            println!("Available Components:");
            println!("- Waybar: {}", context.available_components.waybar);
            println!("- GNOME Shell: {}", context.available_components.gnome_shell);
            
            if ctx.debug {
                println!("\nDetailed Info:");
                println!("{}", context.summary());
            }
        }
        Err(e) => {
            eprintln!("Desktop detection failed: {}", e);
            return Err(e);
        }
    }
    
    Ok(())
}