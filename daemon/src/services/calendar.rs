use anyhow::Result;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, debug, warn};

use crate::config::Config;
use crate::database::Database;
use crate::calendar_sync::CalendarSyncService;

/// Service for managing calendar operations
pub struct CalendarService {
    config: Arc<RwLock<Config>>,
    database: Database,
    calendar_sync: Option<CalendarSyncService>,
}

impl CalendarService {
    pub fn new(config: Arc<RwLock<Config>>, database: Database) -> Self {
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
            calendar_sync,
        }
    }
    
    /// Sync all configured calendars
    pub async fn sync_calendars(&mut self) -> Result<()> {
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
    pub async fn is_authenticated(&self) -> bool {
        if let Some(ref sync) = self.calendar_sync {
            sync.is_authenticated().await
        } else {
            false
        }
    }
    
    /// Get authentication URL for setup
    pub fn get_auth_url(&self) -> Result<Option<(String, String)>> {
        if let Some(ref sync) = self.calendar_sync {
            sync.get_auth_url()
        } else {
            Ok(None)
        }
    }
    
    /// Complete authentication with authorization code
    pub async fn authenticate_with_code(&mut self, auth_code: &str, csrf_token: &str) -> Result<()> {
        if let Some(ref mut sync) = self.calendar_sync {
            sync.authenticate_with_code(auth_code, csrf_token).await
        } else {
            Err(anyhow::anyhow!("Calendar sync not configured"))
        }
    }
    
    /// List available calendars
    pub async fn list_calendars(&mut self) -> Result<Vec<(String, String)>> {
        if let Some(ref mut sync) = self.calendar_sync {
            sync.list_calendars().await
        } else {
            Ok(Vec::new())
        }
    }
    
    /// Get events in a time range
    pub fn get_events_in_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<crate::database::Event>> {
        Ok(self.database.get_events_in_range(start, end)?)
    }
    
    /// Get current planning horizon events
    pub fn get_planning_horizon_events(&self) -> Result<Vec<crate::database::Event>> {
        let planning_horizon = self.config.read().get_planning_horizon();
        let now = chrono::Utc::now();
        let future_window = now + planning_horizon;
        
        self.get_events_in_range(now, future_window)
    }
}