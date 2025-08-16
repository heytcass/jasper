use anyhow::Result;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, debug};

use crate::config::Config;
use crate::database::Database;
use crate::correlation_engine::CorrelationEngine;
use super::{CalendarService, InsightService};

/// Main companion service that orchestrates all business logic
#[allow(dead_code)]
pub struct CompanionService {
    config: Arc<RwLock<Config>>,
    database: Database,
    correlation_engine: CorrelationEngine,
    calendar_service: CalendarService,
    insight_service: InsightService,
}

impl CompanionService {
    /// Create a new companion service with all dependencies
    pub fn new(
        config: Arc<RwLock<Config>>,
        database: Database,
        correlation_engine: CorrelationEngine,
    ) -> Self {
        let calendar_service = CalendarService::new(config.clone(), database.clone());
        let insight_service = InsightService::new(correlation_engine.clone());
        
        Self {
            config,
            database,
            correlation_engine,
            calendar_service,
            insight_service,
        }
    }
    
    /// Perform a full analysis cycle: sync calendars + analyze + generate insights
    pub async fn analyze_full(&mut self) -> Result<Vec<crate::database::Correlation>> {
        info!("Starting full analysis cycle");
        
        // Step 1: Sync calendar data
        debug!("Syncing calendar data");
        if let Err(e) = self.calendar_service.sync_calendars().await {
            tracing::warn!("Calendar sync failed: {}. Proceeding with existing data.", e);
        }
        
        // Step 2: Run correlation analysis
        debug!("Running correlation analysis");
        let correlations = self.insight_service.analyze().await?;
        
        info!("Full analysis cycle completed with {} insights", correlations.len());
        Ok(correlations)
    }
    
    /// Quick analysis using only existing data (for Waybar)
    pub async fn analyze_quick(&self) -> Result<Vec<crate::database::Correlation>> {
        debug!("Running quick analysis with existing data");
        self.insight_service.analyze().await
    }
    
    /// Get current system status
    pub async fn get_status(&self) -> CompanionStatus {
        let is_calendar_authenticated = self.calendar_service.is_authenticated().await;
        let has_api_key = {
            let config = self.config.read();
            config.ai.api_key.is_some()
        };
        
        // Count events in planning horizon
        let planning_horizon = self.config.read().get_planning_horizon();
        let now = chrono::Utc::now();
        let future_window = now + planning_horizon;
        
        let event_count = self.database.get_events_in_range(now, future_window)
            .map(|events| events.len())
            .unwrap_or(0);
        
        CompanionStatus {
            is_calendar_authenticated,
            has_api_key,
            event_count,
            last_analysis: self.insight_service.get_last_analysis_time(),
        }
    }
    
    /// Clear all caches and reset state
    pub fn clear_caches(&self) {
        info!("Clearing all caches and resetting state");
        self.correlation_engine.clear_cache_and_context();
    }
}

/// System status information
#[derive(Debug)]
#[allow(dead_code)]
pub struct CompanionStatus {
    pub is_calendar_authenticated: bool,
    pub has_api_key: bool,
    pub event_count: usize,
    pub last_analysis: Option<chrono::DateTime<chrono::Utc>>,
}