use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::info;

use crate::config::Config;
use crate::database::Database;
use crate::services::CalendarService;
use super::{
    ContextSource, ContextData, ContextDataType, ContextContent, CalendarContext
};

/// Calendar context source that wraps the existing calendar service
pub struct CalendarContextSource {
    calendar_service: CalendarService,
    config: Arc<RwLock<Config>>,
    database: Database,
    enabled: bool,
}

impl CalendarContextSource {
    /// Create a new calendar context source
    pub fn new(config: Arc<RwLock<Config>>, database: Database) -> Self {
        let calendar_service = CalendarService::new(config.clone(), database.clone());
        
        Self {
            calendar_service,
            config,
            database,
            enabled: true,
        }
    }
    
    /// Generate conflict information for events
    fn generate_conflicts(&self, events: &[crate::database::Event]) -> Vec<String> {
        let mut conflicts = Vec::new();
        
        // Simple overlap detection
        for i in 0..events.len() {
            for j in (i + 1)..events.len() {
                let event1 = &events[i];
                let event2 = &events[j];
                
                let event1_start = event1.start_time;
                let event1_end = event1.end_time.unwrap_or(event1_start + 3600);
                let event2_start = event2.start_time;
                let event2_end = event2.end_time.unwrap_or(event2_start + 3600);
                
                // Check if events overlap
                if event1_start < event2_end && event2_start < event1_end {
                    let conflict_msg = format!(
                        "Conflict: '{}' overlaps with '{}'",
                        event1.title.as_deref().unwrap_or("Untitled"),
                        event2.title.as_deref().unwrap_or("Untitled")
                    );
                    conflicts.push(conflict_msg);
                }
            }
        }
        
        conflicts
    }
    
    /// Generate upcoming deadline information
    fn generate_upcoming_deadlines(&self, events: &[crate::database::Event]) -> Vec<String> {
        let mut deadlines = Vec::new();
        let now = Utc::now().timestamp();
        let tomorrow = now + 24 * 3600;
        
        for event in events {
            // Check if event is within next 24 hours
            if event.start_time > now && event.start_time <= tomorrow {
                let deadline_msg = format!(
                    "Tomorrow: '{}' at {}",
                    event.title.as_deref().unwrap_or("Untitled"),
                    DateTime::from_timestamp(event.start_time, 0)
                        .unwrap_or_default()
                        .format("%I:%M %p")
                );
                deadlines.push(deadline_msg);
            }
        }
        
        deadlines.sort();
        deadlines
    }
}

#[async_trait]
impl ContextSource for CalendarContextSource {
    fn source_id(&self) -> &str {
        "calendar"
    }
    
    fn display_name(&self) -> &str {
        "Calendar Events"
    }
    
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    async fn fetch_context(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<ContextData> {
        info!("Fetching context from calendar events");
        
        // Get events from the existing calendar service
        let events = self.calendar_service.get_events_in_range(start, end)?;
        
        // Generate conflicts and deadlines
        let conflicts = self.generate_conflicts(&events);
        let upcoming_deadlines = self.generate_upcoming_deadlines(&events);
        
        let calendar_context = CalendarContext {
            events,
            conflicts,
            upcoming_deadlines,
        };
        
        Ok(ContextData {
            source_id: self.source_id().to_string(),
            timestamp: Utc::now(),
            data_type: ContextDataType::Calendar,
            priority: 150, // Medium-high priority
            content: ContextContent::Calendar(calendar_context),
            metadata: {
                let mut metadata = HashMap::new();
                metadata.insert("source_type".to_string(), "calendar".to_string());
                metadata
            },
        })
    }
    
    fn priority(&self) -> i32 {
        150 // Medium-high priority
    }
    
    fn required_config(&self) -> Vec<String> {
        vec![]
    }
}