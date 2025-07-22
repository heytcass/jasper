use anyhow::Result;
use async_trait::async_trait;

use crate::waybar_formatter::WaybarFormatter;
use super::{Command, CommandContext};

/// Command to output insights in Waybar JSON format
pub struct WaybarCommand {
    pub simple: bool,
}

#[async_trait]
impl Command for WaybarCommand {
    async fn execute(&mut self, context: &CommandContext) -> Result<()> {
        // Run correlation analysis with timeout for Waybar
        let correlations = match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            context.correlation_engine.analyze()
        ).await {
            Ok(Ok(correlations)) => correlations,
            Ok(Err(e)) => {
                // Analysis failed, return empty correlations for clean fallback
                tracing::warn!("Correlation analysis failed for Waybar: {}", e);
                Vec::new()
            }
            Err(_) => {
                // Timeout occurred, return empty correlations
                tracing::warn!("Correlation analysis timed out for Waybar (30s limit)");
                Vec::new()
            }
        };
        
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