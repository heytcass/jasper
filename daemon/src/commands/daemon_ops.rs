use anyhow::Result;
use async_trait::async_trait;
use tracing::info;

use crate::dbus_service::CompanionService;
use crate::test_data;
use super::{Command, CommandContext};

/// Command to start the daemon
pub struct StartCommand;

/// Command to check daemon status
pub struct StatusCommand;

/// Command to stop the daemon
pub struct StopCommand;

/// Command to clear AI cache and context
pub struct ClearCacheCommand;

/// Command to test the notification system
pub struct TestNotificationCommand;

#[async_trait]
impl Command for StartCommand {
    async fn execute(&mut self, context: &CommandContext) -> Result<()> {
        if context.test_mode {
            info!("Running in test mode");
            self.test_correlations(context).await?;
            info!("Starting D-Bus service for testing...");
            tokio::select! {
                result = CompanionService::new(context.database.clone(), context.correlation_engine.clone()) => {
                    match result {
                        Ok(_) => info!("D-Bus service ended normally"),
                        Err(e) => info!("D-Bus service error: {}", e),
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("Test mode stopped");
                }
            };
        } else {
            self.start_daemon(context).await?;
        }
        Ok(())
    }
}

impl StartCommand {
    async fn start_daemon(&self, context: &CommandContext) -> Result<()> {
        info!("Starting Jasper Companion daemon");
        info!("Jasper Companion is now observing your digital life...");

        // Start D-Bus service and wait for shutdown
        tokio::select! {
            result = CompanionService::new(context.database.clone(), context.correlation_engine.clone()) => {
                match result {
                    Ok(_) => info!("D-Bus service ended normally"),
                    Err(e) => info!("D-Bus service error: {}", e),
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Shutdown signal received, stopping daemon");
            }
        };
        
        Ok(())
    }

    async fn test_correlations(&self, context: &CommandContext) -> Result<()> {
        info!("Testing correlation engine...");
        
        // Insert test data
        info!("Inserting test events to demonstrate correlations...");
        test_data::insert_test_events(&context.database).await?;
        
        info!("Starting correlation analysis...");
        let correlations = context.correlation_engine.analyze().await?;
        info!("Correlation analysis completed, processing results...");
        println!("✅ Analysis complete - found {} correlations", correlations.len());
        
        if correlations.is_empty() {
            info!("No correlations found");
        } else {
            println!("🎯 Jasper found {} correlations:", correlations.len());
            for (i, correlation) in correlations.iter().enumerate() {
                let urgency_emoji = match correlation.urgency_score {
                    8..=10 => "🚨",
                    5..=7 => "⚠️",
                    _ => "💡",
                };
                println!("  {}. {} {}", i + 1, urgency_emoji, correlation.insight);
                println!("     Action: {}", correlation.action_needed);
                println!("     Urgency: {}/10", correlation.urgency_score);
                println!("");
            }
            println!("✅ Test completed successfully!");
        }
        
        Ok(())
    }
}

#[async_trait]
impl Command for StatusCommand {
    async fn execute(&mut self, _context: &CommandContext) -> Result<()> {
        info!("Checking daemon status...");
        // TODO: Implement status check via D-Bus
        println!("Status check not yet implemented");
        Ok(())
    }
}

#[async_trait]
impl Command for StopCommand {
    async fn execute(&mut self, _context: &CommandContext) -> Result<()> {
        info!("Stopping daemon...");
        // TODO: Implement graceful shutdown via D-Bus
        println!("Stop command not yet implemented");
        Ok(())
    }
}

#[async_trait]
impl Command for ClearCacheCommand {
    async fn execute(&mut self, context: &CommandContext) -> Result<()> {
        context.correlation_engine.clear_cache_and_context();
        info!("Cache and context state cleared");
        Ok(())
    }
}

#[async_trait]
impl Command for TestNotificationCommand {
    async fn execute(&mut self, context: &CommandContext) -> Result<()> {
        info!("Testing notification system...");
        
        let notification_service = context.correlation_engine.notification_service();
        
        // Check if notification system is available
        if !notification_service.is_notification_system_available() {
            println!("❌ Notification system not available (notify-send not found)");
            println!("Install libnotify-bin or similar package for your distribution");
            return Ok(());
        }
        
        println!("✅ Notification system available");
        
        // Send a test notification
        match notification_service.test_notification().await {
            Ok(_) => {
                println!("✅ Test notification sent successfully!");
                println!("If you see a desktop notification, the system is working correctly.");
            }
            Err(e) => {
                println!("❌ Failed to send test notification: {}", e);
            }
        }
        
        // Show system info
        let system_info = notification_service.get_system_info();
        println!("\n📊 Notification System Status:");
        println!("  • Notifications available: {}", system_info.notifications_available);
        println!("  • Enabled: {}", system_info.config.enabled);
        println!("  • Notify on new insights: {}", system_info.config.notify_new_insights);
        println!("  • Notify on context changes: {}", system_info.config.notify_context_changes);
        println!("  • Notify on cache refresh: {}", system_info.config.notify_cache_refresh);
        println!("  • Timeout: {}ms", system_info.config.notification_timeout);
        println!("  • Cooldown active: {}", system_info.cooldown_active);
        
        if let Some(last_notification) = system_info.last_notification {
            println!("  • Last notification: {}", last_notification.format("%Y-%m-%d %H:%M:%S UTC"));
        } else {
            println!("  • Last notification: Never");
        }
        
        Ok(())
    }
}