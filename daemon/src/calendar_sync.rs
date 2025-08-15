use anyhow::{Result, anyhow, Context};
use chrono::{Utc, Duration};
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{info, debug, warn, error};

use crate::config::Config;
use crate::database::Database;
use crate::google_calendar::{GoogleCalendarService, GoogleCalendarConfig};

pub struct CalendarSyncService {
    config: Arc<RwLock<Config>>,
    database: Database,
    google_calendar: Option<GoogleCalendarService>,
}

impl CalendarSyncService {
    pub fn new(config: Arc<RwLock<Config>>, database: Database) -> Result<Self> {
        let google_calendar = {
            let config_guard = config.read();
            if let Some(gc_config) = &config_guard.google_calendar {
                if gc_config.enabled && !gc_config.client_id.is_empty() {
                    let data_dir = Config::get_data_dir()?;
                    let google_config = GoogleCalendarConfig {
                        client_id: gc_config.client_id.clone(),
                        client_secret: gc_config.client_secret.clone(),
                        redirect_uri: gc_config.redirect_uri.clone(),
                        calendar_ids: gc_config.calendar_ids.clone(),
                    };
                    Some(GoogleCalendarService::new(google_config, data_dir))
                } else {
                    None
                }
            } else {
                None
            }
        };

        Ok(Self {
            config,
            database,
            google_calendar,
        })
    }

    // Background sync loop removed - currently using manual sync only

    /// Perform a single calendar sync operation
    pub async fn sync_calendars(&mut self) -> Result<()> {
        debug!("Starting calendar sync operation");

        if let Some(ref mut google_calendar) = self.google_calendar {
            // Note: We don't check is_authenticated() here because fetch_events() 
            // will handle token refresh automatically via get_valid_token()

            // Get sync time window
            let planning_horizon = {
                let config = self.config.read();
                config.general.planning_horizon_days
            };

            let now = Utc::now();
            let future_window = now + Duration::days(planning_horizon as i64);

            info!("Syncing Google Calendar events from {} to {}", now, future_window);

            // Fetch events from Google Calendar (grouped by calendar)
            match google_calendar.fetch_events(now, future_window).await {
                Ok(events_by_calendar) => {
                    let total_events: usize = events_by_calendar.iter().map(|(_, events)| events.len()).sum();
                    info!("Fetched {} events from Google Calendar", total_events);
                    
                    // Store events in database with proper calendar records
                    self.store_calendar_events_with_metadata(events_by_calendar).await
                        .context("Failed to store calendar events with metadata")?;
                }
                Err(e) => {
                    error!("Failed to fetch Google Calendar events: {}", e);
                    return Err(e);
                }
            }
        } else {
            debug!("No calendar services configured");
        }

        Ok(())
    }

    /// Store fetched calendar events with proper calendar records
    async fn store_calendar_events_with_metadata(
        &mut self, 
        events_by_calendar: Vec<(String, Vec<crate::database::Event>)>
    ) -> Result<()> {
        for (google_calendar_id, events) in events_by_calendar {
            debug!("Processing calendar: {}", google_calendar_id);
            
            // Get calendar metadata from Google
            let (calendar_id, calendar_name, _color) = if let Some(ref mut google_calendar) = self.google_calendar {
                match google_calendar.get_calendar_metadata(&google_calendar_id).await {
                    Ok(metadata) => metadata,
                    Err(e) => {
                        warn!("Failed to get metadata for calendar {}: {}", google_calendar_id, e);
                        (google_calendar_id.clone(), google_calendar_id.clone(), None)
                    }
                }
            } else {
                (google_calendar_id.clone(), google_calendar_id.clone(), None)
            };
            
            // Infer calendar type from name and ID
            let calendar_type = self.infer_calendar_type(&calendar_id, &calendar_name);
            
            // Create or update calendar record in database
            let db_calendar_id = self.database.create_or_update_calendar(
                &calendar_id,
                &calendar_name,
                calendar_type.as_deref()
            )?;
            
            info!("Calendar '{}' (ID: {}) mapped to database ID: {}", calendar_name, calendar_id, db_calendar_id);
            
            // Now store events with correct calendar_id
            self.store_events_for_calendar(events, db_calendar_id).await
                .with_context(|| format!("Failed to store events for calendar: {}", calendar_name))?;
        }
        
        info!("Calendar sync completed");
        Ok(())
    }
    
    /// Store events for a specific calendar
    async fn store_events_for_calendar(&self, events: Vec<crate::database::Event>, db_calendar_id: i64) -> Result<()> {
        debug!("Storing {} events for calendar ID {} using bulk operations", events.len(), db_calendar_id);

        if events.is_empty() {
            return Ok(());
        }

        // Set the correct database calendar ID for all events
        let prepared_events: Vec<crate::database::Event> = events.into_iter()
            .map(|mut event| {
                event.calendar_id = db_calendar_id;
                event
            })
            .collect();

        // Use bulk insert with transaction handling
        match self.database.create_events_bulk(&prepared_events) {
            Ok(event_ids) => {
                debug!("Successfully stored {} events using bulk operation", event_ids.len());
                
                // Log a few examples for debugging
                for (i, event_id) in event_ids.iter().take(3).enumerate() {
                    if let Some(event) = prepared_events.get(i) {
                        debug!("Stored event '{}' with ID: {}", 
                               event.title.as_deref().unwrap_or("(no title)"), event_id);
                    }
                }
                
                if event_ids.len() != prepared_events.len() {
                    debug!("Note: {} events were skipped (already existed)", 
                           prepared_events.len() - event_ids.len());
                }
            }
            Err(e) => {
                warn!("Bulk event storage failed: {}. Falling back to individual inserts", e);
                
                // Fallback to individual inserts if bulk operation fails
                for event in prepared_events {
                    if self.database.get_event_by_source_id(&event.source_id)?.is_some() {
                        debug!("Event {} already exists, skipping", event.source_id);
                        continue;
                    }

                    match self.database.create_event(&event) {
                        Ok(event_id) => {
                            debug!("Stored calendar event '{}' with ID: {}", 
                                   event.title.as_deref().unwrap_or("(no title)"), event_id);
                        }
                        Err(e) => {
                            warn!("Failed to store calendar event '{}': {}", 
                                  event.title.as_deref().unwrap_or("(no title)"), e);
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Infer calendar type from ID and name patterns
    fn infer_calendar_type(&self, calendar_id: &str, calendar_name: &str) -> Option<String> {
        let id_lower = calendar_id.to_lowercase();
        let name_lower = calendar_name.to_lowercase();
        
        // Primary calendar
        if calendar_id == "primary" || name_lower.contains("personal") {
            return Some("personal".to_string());
        }
        
        // Family calendars
        if id_lower.contains("family") || name_lower.contains("family") {
            return Some("family".to_string());
        }
        
        // House/maintenance calendars
        if id_lower.contains("house") || name_lower.contains("house") || 
           name_lower.contains("maintenance") || name_lower.contains("home") {
            return Some("house".to_string());
        }
        
        // Work calendars
        if id_lower.contains("work") || name_lower.contains("work") || 
           name_lower.contains("office") || name_lower.contains("business") {
            return Some("work".to_string());
        }
        
        // Holiday calendars
        if id_lower.contains("holiday") || name_lower.contains("holiday") {
            return Some("holiday".to_string());
        }
        
        // Celebration calendars
        if name_lower.contains("celebration") || name_lower.contains("birthday") {
            return Some("celebration".to_string());
        }
        
        None // Unknown type
    }

    /// Manual sync trigger for testing/debugging
    pub async fn sync_now(&mut self) -> Result<()> {
        info!("Manual calendar sync triggered");
        self.sync_calendars().await
    }

    /// Check authentication status
    pub async fn is_authenticated(&self) -> bool {
        if let Some(ref google_calendar) = self.google_calendar {
            google_calendar.is_authenticated().await
        } else {
            false
        }
    }

    /// Get authentication URL for setup
    pub fn get_auth_url(&self) -> Result<Option<(String, String)>> {
        if let Some(ref google_calendar) = self.google_calendar {
            let (url, csrf_token) = google_calendar.get_auth_url()?;
            Ok(Some((url, csrf_token.secret().clone())))
        } else {
            Ok(None)
        }
    }

    /// Complete authentication with authorization code
    pub async fn authenticate_with_code(&mut self, auth_code: &str, csrf_token: &str) -> Result<()> {
        if let Some(ref mut google_calendar) = self.google_calendar {
            google_calendar.authenticate_with_code(auth_code, csrf_token).await
                .context("Failed to authenticate with Google Calendar using provided code")?;
            info!("Google Calendar authentication completed successfully");
            
            // Trigger immediate sync after authentication
            self.sync_calendars().await
                .context("Failed to sync calendars after authentication")?;
            
            Ok(())
        } else {
            Err(anyhow!("Google Calendar service not configured"))
        }
    }

    /// List available calendars (for configuration)
    pub async fn list_calendars(&mut self) -> Result<Vec<(String, String)>> {
        if let Some(ref mut google_calendar) = self.google_calendar {
            google_calendar.list_calendars().await
        } else {
            Ok(Vec::new())
        }
    }

    // Calendar configuration update removed - currently using static config only
}