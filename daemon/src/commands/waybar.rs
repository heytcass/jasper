use anyhow::Result;
use async_trait::async_trait;
use zbus::Connection;
use tracing::{debug, warn};

use crate::waybar_formatter::WaybarFormatter;
use super::{Command, CommandContext};

/// Command to output insights in Waybar JSON format
pub struct WaybarCommand {
    pub simple: bool,
}

#[async_trait]
impl Command for WaybarCommand {
    async fn execute(&mut self, context: &CommandContext) -> Result<()> {
        // Try to get waybar JSON from D-Bus daemon first
        match self.try_dbus_waybar_json().await {
            Ok(json) => {
                if self.simple {
                    // For simple mode, we need to parse and reformat
                    // This is a fallback - ideally simple mode would have its own D-Bus method
                    debug!("Simple mode not yet optimized for D-Bus, using formatted JSON");
                    println!("{}", json);
                } else {
                    // Direct JSON output from daemon
                    println!("{}", json);
                }
                Ok(())
            }
            Err(e) => {
                warn!("D-Bus waybar query failed, falling back to direct analysis: {}", e);
                self.fallback_direct_analysis(context).await
            }
        }
    }
}

impl WaybarCommand {
    /// Try to get waybar JSON from the D-Bus daemon service
    async fn try_dbus_waybar_json(&self) -> Result<String> {
        debug!("Attempting to query D-Bus daemon for waybar data");
        
        // Connect to the session bus
        let connection = Connection::session().await?;
        
        // Create a proxy to the Jasper daemon
        let proxy = zbus::Proxy::new(
            &connection,
            "org.personal.CompanionAI",
            "/org/personal/CompanionAI/Companion",
            "org.personal.CompanionAI.Companion1",
        ).await?;
        
        // Call the GetWaybarJson method (zbus uses PascalCase for D-Bus method names)
        let json: String = proxy.call("GetWaybarJson", &()).await?;
        
        debug!("Successfully retrieved waybar JSON from D-Bus daemon");
        Ok(json)
    }
    
    /// Fallback when D-Bus is unavailable - return empty insights to avoid AI calls
    async fn fallback_direct_analysis(&self, context: &CommandContext) -> Result<()> {
        debug!("D-Bus daemon unavailable - waybar will show 'daemon offline' message");
        
        // Don't trigger AI analysis from frontend - let daemon handle this
        // Return empty correlations so waybar shows daemon offline state
        let correlations = Vec::new();
        
        // Get timezone from config
        let timezone = context.config.read().get_timezone();
        
        let formatter = WaybarFormatter::new(timezone);
        
        if self.simple {
            // Simple text output for basic status bars
            println!("{}", formatter.format_simple(&correlations));
        } else {
            // Full JSON output for Waybar
            let output = formatter.format_correlations(&correlations)?;
            println!("{}", serde_json::to_string(&output)?);
        }
        
        Ok(())
    }
}