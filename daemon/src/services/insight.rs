use anyhow::Result;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, debug};

use crate::correlation_engine::CorrelationEngine;
use crate::database::Correlation;

/// Service for AI insight generation and analysis
#[allow(dead_code)]
pub struct InsightService {
    correlation_engine: CorrelationEngine,
    last_analysis: Arc<RwLock<Option<chrono::DateTime<chrono::Utc>>>>,
}

impl InsightService {
    pub fn new(correlation_engine: CorrelationEngine) -> Self {
        Self {
            correlation_engine,
            last_analysis: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Run full correlation analysis and generate insights
    pub async fn analyze(&self) -> Result<Vec<Correlation>> {
        debug!("Starting insight analysis");
        
        let correlations = self.correlation_engine.analyze().await?;
        
        // Update last analysis time
        {
            let mut last_analysis = self.last_analysis.write();
            *last_analysis = Some(chrono::Utc::now());
        }
        
        info!("Insight analysis completed with {} correlations", correlations.len());
        
        // Note: Notifications are handled by the CorrelationEngine itself to avoid duplicates
        
        Ok(correlations)
    }
    
    /// Get the most urgent insight for display
    pub async fn get_most_urgent(&self) -> Result<Option<Correlation>> {
        let correlations = self.analyze().await?;
        
        // Find correlation with highest urgency score
        let most_urgent = correlations
            .into_iter()
            .max_by_key(|c| c.urgency_score);
        
        Ok(most_urgent)
    }
    
    /// Get insights filtered by urgency level
    pub async fn get_insights_by_urgency(&self, min_urgency: i32) -> Result<Vec<Correlation>> {
        let correlations = self.analyze().await?;
        
        let filtered: Vec<Correlation> = correlations
            .into_iter()
            .filter(|c| c.urgency_score >= min_urgency)
            .collect();
        
        Ok(filtered)
    }
    
    /// Get time of last analysis
    pub fn get_last_analysis_time(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        *self.last_analysis.read()
    }
    
    /// Clear analysis cache and context
    pub fn clear_cache(&self) {
        debug!("Clearing insight analysis cache");
        self.correlation_engine.clear_cache_and_context();
        
        // Reset last analysis time
        {
            let mut last_analysis = self.last_analysis.write();
            *last_analysis = None;
        }
    }
    
    /// Check if analysis is stale and needs refresh
    pub fn needs_refresh(&self, max_age_minutes: i64) -> bool {
        let last_analysis = self.last_analysis.read();
        
        match *last_analysis {
            Some(last_time) => {
                let now = chrono::Utc::now();
                let age = now.signed_duration_since(last_time);
                age.num_minutes() > max_age_minutes
            }
            None => true, // Never analyzed
        }
    }
    
    /// Force refresh of insights (bypasses cache)
    pub async fn force_refresh(&self) -> Result<Vec<Correlation>> {
        debug!("Forcing insight analysis refresh");
        self.clear_cache();
        
        let correlations = self.analyze().await?;
        
        // Note: The cache refresh notification is already sent by clear_cache()
        // and analyze() will send its completion notification
        
        Ok(correlations)
    }
}