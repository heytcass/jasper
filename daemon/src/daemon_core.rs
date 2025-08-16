use crate::errors::{JasperError, JasperResult};
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, debug, warn};

#[cfg(feature = "new-config")]
use crate::config_v2;
use crate::config::Config;
use crate::database::{Database, Event, Correlation};
use crate::correlation_engine::CorrelationEngine;
use crate::calendar_sync::CalendarSyncService;

/// Single flattened daemon that combines all business logic
/// No service layers, no indirection - direct implementation as Linus suggested
#[allow(dead_code)]
pub struct DaemonCore {
    // Core dependencies
    #[cfg(not(feature = "new-config"))]
    config: Arc<RwLock<Config>>,
    database: Database,
    correlation_engine: CorrelationEngine,
    
    // Calendar functionality (previously CalendarService)
    calendar_sync: Option<CalendarSyncService>,
    
    // Insight functionality (previously InsightService)
    last_analysis: Arc<RwLock<Option<chrono::DateTime<chrono::Utc>>>>,
}

impl DaemonCore {
    /// Create new daemon with all functionality flattened
    #[cfg(not(feature = "new-config"))]
    pub fn new(
        config: Arc<RwLock<Config>>,
        database: Database,
        correlation_engine: CorrelationEngine,
    ) -> Self {
        // Initialize calendar sync if Google Calendar is configured
        let calendar_sync = match CalendarSyncService::new(config.clone(), database.clone()) {
            Ok(sync) => Some(sync),
            Err(e) => {
                debug!("Calendar sync not available: {}", e);
                None
            }
        };
        
        Self {
            config,
            database,
            correlation_engine,
            calendar_sync,
            last_analysis: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Create new daemon with static config
    #[cfg(feature = "new-config")]
    pub fn new(
        _config: Arc<RwLock<Config>>, // Ignored for API compatibility
        database: Database,
        correlation_engine: CorrelationEngine,
    ) -> Self {
        // Initialize calendar sync with static config
        let config_arc = Arc::new(RwLock::new(config_v2::config().as_ref().clone()));
        let calendar_sync = match CalendarSyncService::new(config_arc, database.clone()) {
            Ok(sync) => Some(sync),
            Err(e) => {
                debug!("Calendar sync not available: {}", e);
                None
            }
        };
        
        Self {
            database,
            correlation_engine,
            calendar_sync,
            last_analysis: Arc::new(RwLock::new(None)),
        }
    }

    // ============================================================================
    // CORE ORCHESTRATION METHODS (previously CompanionService)
    // ============================================================================
    
    /// Perform a full analysis cycle: sync calendars + analyze + generate insights
    pub async fn analyze_full(&mut self) -> JasperResult<Vec<Correlation>> {
        info!("Starting full analysis cycle");
        
        // Step 1: Sync calendar data
        debug!("Syncing calendar data");
        if let Err(e) = self.sync_calendars().await {
            warn!("Calendar sync failed: {}. Proceeding with existing data.", e);
        }
        
        // Step 2: Run correlation analysis
        debug!("Running correlation analysis");
        let correlations = self.analyze_insights().await?;
        
        info!("Full analysis cycle completed with {} insights", correlations.len());
        Ok(correlations)
    }
    
    /// Quick analysis using only existing data (for Waybar)
    pub async fn analyze_quick(&self) -> JasperResult<Vec<Correlation>> {
        debug!("Running quick analysis with existing data");
        self.analyze_insights().await
    }
    
    /// Get current system status
    pub async fn get_status(&self) -> DaemonStatus {
        let is_calendar_authenticated = self.is_calendar_authenticated().await;
        let has_api_key = self.has_api_key();
        
        // Count events in planning horizon
        let planning_horizon = self.get_planning_horizon();
        let now = chrono::Utc::now();
        let future_window = now + planning_horizon;
        
        let event_count = self.database.get_events_in_range(now, future_window)
            .map(|events| events.len())
            .unwrap_or(0);
        
        DaemonStatus {
            is_calendar_authenticated,
            has_api_key,
            event_count,
            last_analysis: self.get_last_analysis_time(),
        }
    }
    
    /// Clear all caches and reset state
    pub fn clear_caches(&self) {
        info!("Clearing all caches and resetting state");
        self.correlation_engine.clear_cache_and_context();
        
        // Reset last analysis time
        {
            let mut last_analysis = self.last_analysis.write();
            *last_analysis = None;
        }
    }

    // ============================================================================
    // CALENDAR METHODS (previously CalendarService)
    // ============================================================================
    
    /// Sync all configured calendars
    pub async fn sync_calendars(&mut self) -> JasperResult<()> {
        if let Some(ref mut sync) = self.calendar_sync {
            info!("Syncing calendars");
            sync.sync_calendars().await?;
            info!("Calendar sync completed");
        } else {
            warn!("Calendar sync not configured");
        }
        Ok(())
    }
    
    /// Check if calendar authentication is valid
    pub async fn is_calendar_authenticated(&self) -> bool {
        if let Some(ref sync) = self.calendar_sync {
            sync.is_authenticated().await
        } else {
            false
        }
    }
    
    /// Get authentication URL for setup
    pub fn get_auth_url(&self) -> JasperResult<Option<(String, String)>> {
        if let Some(ref sync) = self.calendar_sync {
            Ok(sync.get_auth_url()?)
        } else {
            Ok(None)
        }
    }
    
    /// Complete authentication with authorization code
    pub async fn authenticate_with_code(&mut self, auth_code: &str, csrf_token: &str) -> JasperResult<()> {
        if let Some(ref mut sync) = self.calendar_sync {
            Ok(sync.authenticate_with_code(auth_code, csrf_token).await?)
        } else {
            Err(JasperError::calendar_sync("Calendar sync not configured"))
        }
    }
    
    /// List available calendars
    pub async fn list_calendars(&mut self) -> JasperResult<Vec<(String, String)>> {
        if let Some(ref mut sync) = self.calendar_sync {
            Ok(sync.list_calendars().await?)
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Get events in a time range
    pub fn get_events_in_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> JasperResult<Vec<Event>> {
        Ok(self.database.get_events_in_range(start, end)?)
    }
    
    /// Get current planning horizon events
    pub fn get_planning_horizon_events(&self) -> JasperResult<Vec<Event>> {
        let planning_horizon = self.get_planning_horizon();
        let now = chrono::Utc::now();
        let future_window = now + planning_horizon;
        
        self.get_events_in_range(now, future_window)
    }

    // ============================================================================
    // INSIGHT METHODS (previously InsightService)
    // ============================================================================
    
    /// Run full correlation analysis and generate insights
    pub async fn analyze_insights(&self) -> JasperResult<Vec<Correlation>> {
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
    pub async fn get_most_urgent_insight(&self) -> JasperResult<Option<Correlation>> {
        let correlations = self.analyze_insights().await?;
        
        // Find correlation with highest urgency score
        let most_urgent = correlations
            .into_iter()
            .max_by_key(|c| c.urgency_score);
        
        Ok(most_urgent)
    }
    
    /// Get insights filtered by urgency level
    pub async fn get_insights_by_urgency(&self, min_urgency: i32) -> JasperResult<Vec<Correlation>> {
        let correlations = self.analyze_insights().await?;
        
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
    
    /// Force refresh of insights (bypassing cache)
    pub async fn force_refresh(&self) -> JasperResult<Vec<Correlation>> {
        debug!("Forcing insight analysis refresh");
        self.clear_caches();
        self.analyze_insights().await
    }

    // ============================================================================
    // HELPER METHODS
    // ============================================================================
    
    /// Get planning horizon duration from config
    fn get_planning_horizon(&self) -> chrono::Duration {
        #[cfg(not(feature = "new-config"))]
        {
            self.config.read().get_planning_horizon()
        }
        
        #[cfg(feature = "new-config")]
        {
            config_v2::config().get_planning_horizon()
        }
    }
    
    /// Check if API key is configured
    fn has_api_key(&self) -> bool {
        #[cfg(not(feature = "new-config"))]
        {
            self.config.read().ai.api_key.is_some()
        }
        
        #[cfg(feature = "new-config")]
        {
            config_v2::config().ai.api_key.is_some()
        }
    }
    
    /// Get timezone from config
    pub fn get_timezone(&self) -> chrono_tz::Tz {
        #[cfg(not(feature = "new-config"))]
        {
            self.config.read().get_timezone()
        }
        
        #[cfg(feature = "new-config")]
        {
            config_v2::config().get_timezone()
        }
    }
}

/// System status information - renamed from CompanionStatus
#[derive(Debug)]
#[allow(dead_code)]
pub struct DaemonStatus {
    pub is_calendar_authenticated: bool,
    pub has_api_key: bool,
    pub event_count: usize,
    pub last_analysis: Option<chrono::DateTime<chrono::Utc>>,
}