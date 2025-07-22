use anyhow::Result;
use zbus::ConnectionBuilder;
use std::sync::Arc;
use parking_lot::Mutex;
use tracing::{info, debug};

use crate::database::Database;
use crate::correlation_engine::CorrelationEngine;

pub struct CompanionService {
    database: Database,
    correlation_engine: CorrelationEngine,
    current_insights: Arc<Mutex<Vec<crate::database::Correlation>>>,
}

impl CompanionService {
    pub async fn new(database: Database, correlation_engine: CorrelationEngine) -> Result<()> {
        let service = CompanionService {
            database: database.clone(),
            correlation_engine,
            current_insights: Arc::new(Mutex::new(Vec::new())),
        };

        // Analyze correlations on startup
        service.refresh_insights().await?;

        let _connection = ConnectionBuilder::session()?
            .name("org.personal.CompanionAI")?
            .serve_at("/org/personal/CompanionAI/Companion", service)?
            .build()
            .await?;

        info!("D-Bus service registered at org.personal.CompanionAI");

        // Keep the service running
        std::future::pending::<()>().await;
        Ok(())
    }

    async fn refresh_insights(&self) -> Result<()> {
        debug!("Refreshing insights...");
        let correlations = self.correlation_engine.analyze().await?;
        *self.current_insights.lock() = correlations;
        info!("Refreshed insights: {} correlations found", self.current_insights.lock().len());
        Ok(())
    }
}

#[zbus::interface(name = "org.personal.CompanionAI.Companion1")]
impl CompanionService {
    /// Get current insight using "THE Insight" model
    async fn get_current_insight(&self) -> (String, String, String, i32, String) {
        let insights = self.current_insights.lock();
        
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

    /// Get detailed insight information for progressive disclosure
    async fn get_insight_details(&self, insight_id: String) -> (String, String) {
        let insights = self.current_insights.lock();
        
        // Find the correlation by ID
        if let Some(correlation) = insights.iter().find(|c| format!("insight_{}", c.id) == insight_id) {
            let reasoning = format!("This insight is based on patterns in your calendar data. Urgency level: {}/10", correlation.urgency_score);
            let full_data = format!("Action needed: {}\nFull insight: {}", correlation.action_needed, correlation.insight);
            (reasoning, full_data)
        } else {
            ("No details available".to_string(), "Insight not found".to_string())
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
        "Observing".to_string()
    }
}