use anyhow::{Result, Context};
use zbus::ConnectionBuilder;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, debug, warn};

#[cfg(feature = "new-config")]
use crate::config_v2;
use crate::config::Config;
use crate::database::Database;
use crate::correlation_engine::CorrelationEngine;
use crate::frontend_framework::InsightData;
use crate::frontend_manager::FrontendManager;

pub struct CompanionService {
    database: Database,
    correlation_engine: CorrelationEngine,
    #[cfg(not(feature = "new-config"))]
    config: Arc<RwLock<Config>>,
    frontend_manager: FrontendManager,
}

impl CompanionService {
    pub async fn new(database: Database, correlation_engine: CorrelationEngine, config: Arc<parking_lot::RwLock<Config>>) -> Result<()> {
        let service = CompanionService {
            database: database.clone(),
            correlation_engine: correlation_engine.clone(),
            #[cfg(not(feature = "new-config"))]
            config: config.clone(),
            frontend_manager: FrontendManager::new(),
        };

        // Initial analysis on startup
        service.refresh_insights().await
            .context("Failed to refresh insights on D-Bus service startup")?;

        let _connection = ConnectionBuilder::session()?
            .name("org.personal.CompanionAI")?
            .serve_at("/org/personal/CompanionAI/Companion", service)?
            .build()
            .await
            .context("Failed to build D-Bus connection")?;

        info!("D-Bus service registered at org.personal.CompanionAI");

        // Keep the service running
        std::future::pending::<()>().await;
        Ok(())
    }

    async fn refresh_insights(&self) -> Result<()> {
        debug!("Refreshing insights...");
        
        // Use correlation engine directly - simpler and avoids locks across awaits
        let correlations = self.correlation_engine.analyze().await
            .context("Failed to analyze correlations for insights")?;
        
        info!("Refreshed insights: {} correlations found", correlations.len());
        Ok(())
    }
}

#[zbus::interface(name = "org.personal.CompanionAI.Companion1")]
impl CompanionService {
    /// Get current insight using "THE Insight" model
    async fn get_current_insight(&self) -> (String, String, String, i32, String) {
        // Use correlation engine directly - no locks needed!
        match self.correlation_engine.analyze().await {
            Ok(insights) => {
                if let Some(correlation) = insights.first() {
                    // Generate insight ID from correlation data
                    let insight_id = format!("insight_{}", correlation.id);
                    
                    // For now, use simplified extraction to avoid crashes
                    let emoji = "🎯".to_string(); // Use a fixed emoji
                    let action = correlation.insight.clone(); // Use full insight for now
                    
                    (emoji, action, "".to_string(), correlation.urgency_score, insight_id)
                } else {
                    ("🔍".to_string(), "Analyzing your digital patterns...".to_string(), "No insights available right now".to_string(), 0, "".to_string())
                }
            }
            Err(e) => {
                warn!("Failed to get current insight: {}", e);
                ("⚠️".to_string(), "Analysis temporarily unavailable".to_string(), "".to_string(), 0, "".to_string())
            }
        }
    }

    /// Get detailed insight information for progressive disclosure
    async fn get_insight_details(&self, insight_id: String) -> (String, String) {
        // Get current insights directly from correlation engine
        match self.correlation_engine.analyze().await {
            Ok(insights) => {
                // Find the correlation by ID
                if let Some(correlation) = insights.iter().find(|c| format!("insight_{}", c.id) == insight_id) {
                    let reasoning = format!("This insight is based on patterns in your calendar data. Urgency level: {}/10", correlation.urgency_score);
                    let full_data = format!("Action needed: {}\nFull insight: {}", correlation.action_needed, correlation.insight);
                    (reasoning, full_data)
                } else {
                    ("No details available".to_string(), "Insight not found".to_string())
                }
            }
            Err(e) => {
                warn!("Failed to get insight details: {}", e);
                ("Analysis unavailable".to_string(), "Unable to retrieve insight details".to_string())
            }
        }
    }

    /// Acknowledge insight and provide user action
    async fn acknowledge_insight(&self, insight_id: String, action_taken: String) {
        info!("Insight {} acknowledged with action: {}", insight_id, action_taken);
        // TODO: Store acknowledgment in database for learning
    }

    /// Provide explicit feedback for ambient learning
    async fn provide_feedback(&self, insight_id: String, feedback_type: String) {
        info!("Feedback for insight {}: {}", insight_id, feedback_type);
        // TODO: Store feedback for AI learning
    }

    /// Request refresh of insights
    async fn request_refresh(&self) -> zbus::fdo::Result<()> {
        match self.refresh_insights().await {
            Ok(_) => {
                info!("Insights refreshed via D-Bus request");
                Ok(())
            }
            Err(e) => {
                info!("Failed to refresh insights: {}", e);
                Err(zbus::fdo::Error::Failed(format!("Refresh failed: {}", e)))
            }
        }
    }


    /// Get daemon status  
    async fn get_status(&self) -> String {
        // For now, return a simple status - can be enhanced later
        "D-Bus Service Active".to_string()
    }

    /// Get insights formatted for any supported frontend
    async fn get_formatted_insights(&self, frontend_id: String) -> String {
        // Get correlations directly from correlation engine
        let correlations = match self.correlation_engine.analyze().await {
            Ok(correlations) => correlations,
            Err(e) => {
                warn!("Failed to get correlations for formatting: {}", e);
                Vec::new()
            }
        };
        
        // Get timezone from config
        #[cfg(not(feature = "new-config"))]
        let timezone = self.config.read().get_timezone();
        
        #[cfg(feature = "new-config")]
        let timezone = config_v2::config().get_timezone();
        
        // Convert correlations to InsightData format
        let insights: Vec<InsightData> = correlations.iter()
            .map(InsightData::from_correlation)
            .collect();
        
        // Use the frontend manager to format for the requested frontend
        match self.frontend_manager.format(&frontend_id, &insights, timezone) {
            Ok(formatted) => formatted,
            Err(e) => {
                warn!("Failed to format insights for frontend '{}': {}", frontend_id, e);
                
                // Try to return an error format for the requested frontend
                match self.frontend_manager.format_error(&frontend_id, &format!("Formatting error: {}", e), timezone) {
                    Ok(error_formatted) => error_formatted,
                    Err(_) => {
                        // Final fallback - return a generic error message
                        format!(r#"{{"error": "Unknown frontend '{}' or formatting failed"}}"#, frontend_id)
                    }
                }
            }
        }
    }

    /// List all available frontends
    async fn list_frontends(&self) -> Vec<(String, String)> {
        self.frontend_manager.list_frontends()
    }

    /// Get insights formatted for Waybar JSON output (backwards compatibility)
    async fn get_waybar_json(&self) -> String {
        // Use the new frontend system for consistency, but maintain backwards compatibility
        self.get_formatted_insights("waybar".to_string()).await
    }
}