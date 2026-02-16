use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

use crate::context_sources;

/// Represents a snapshot of context at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub calendar_events: Vec<CalendarEventSummary>,
    pub weather: Option<WeatherSummary>,
    pub tasks: Vec<TaskSummary>,
    /// Full notes context (projects, relationships, focus areas) — passed through to AI prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_context: Option<context_sources::NotesContext>,
    /// Full weather context (forecasts, alerts) — passed through to AI prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weather_context: Option<context_sources::WeatherContext>,
    pub timestamp: DateTime<Utc>,
    pub context_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
pub struct CalendarEventSummary {
    pub id: String,
    pub title: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub location: Option<String>,
    pub is_all_day: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
pub struct WeatherSummary {
    pub condition: String,
    pub temperature: i32, // Use integer to avoid floating point hash issues
    pub feels_like: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash)]
pub struct TaskSummary {
    pub id: String,
    pub title: String,
    pub due: Option<DateTime<Utc>>,
    pub completed: bool,
}

/// Types of significant changes that warrant an AI call
#[derive(Debug, Clone, PartialEq)]
pub enum SignificantChange {
    NewCalendarEvent(String),
    CancelledCalendarEvent(String),
    EventTimeChanged { event_id: String, time_diff_hours: f64 },
    EventLocationChanged { event_id: String },
    WeatherConditionChanged { from: String, to: String },
    WeatherTemperatureChanged { diff: i32 },
    NewTask(String),
    TaskCompleted(String),
    TaskDueChanged { task_id: String, time_diff_hours: f64 },
    InitialContext,
}

/// Engine that determines if context changes are significant enough to warrant an AI call
pub struct SignificanceEngine {
    last_snapshot: parking_lot::Mutex<Option<ContextSnapshot>>,
    last_ai_call: parking_lot::Mutex<Option<DateTime<Utc>>>,
    min_time_between_calls: Duration,
}

impl SignificanceEngine {
    pub fn new() -> Self {
        Self {
            last_snapshot: parking_lot::Mutex::new(None),
            last_ai_call: parking_lot::Mutex::new(None),
            min_time_between_calls: Duration::minutes(5), // Don't call AI more than once per 5 minutes
        }
    }

    /// Analyze a new context snapshot and determine if changes are significant
    pub fn analyze_context(&self, new_snapshot: ContextSnapshot) -> (bool, Vec<SignificantChange>) {
        // Clone the previous snapshot (if any) and release the lock immediately
        let previous = self.last_snapshot.lock().clone();

        let Some(ref last) = previous else {
            info!("Initial context detected - significant by default");
            *self.last_snapshot.lock() = Some(new_snapshot);
            return (true, vec![SignificantChange::InitialContext]);
        };

        // Check minimum time between AI calls
        {
            let last_ai_call = self.last_ai_call.lock();
            if let Some(last_call) = *last_ai_call {
                let time_since_last = Utc::now() - last_call;
                if time_since_last < self.min_time_between_calls {
                    debug!("Skipping analysis - too soon since last AI call ({} seconds ago)",
                        time_since_last.num_seconds());
                    return (false, vec![]);
                }
            }
        }

        let mut changes = Vec::new();

        // Check calendar changes
        changes.extend(self.check_calendar_changes(&last.calendar_events, &new_snapshot.calendar_events));

        // Check weather changes
        if let (Some(ref old_weather), Some(ref new_weather)) = (&last.weather, &new_snapshot.weather) {
            changes.extend(self.check_weather_changes(old_weather, new_weather));
        }

        // Check task changes
        changes.extend(self.check_task_changes(&last.tasks, &new_snapshot.tasks));

        // Determine if any changes are significant
        let is_significant = !changes.is_empty();

        // Always update the snapshot to track incremental changes
        *self.last_snapshot.lock() = Some(new_snapshot);

        if is_significant {
            info!("Significant changes detected: {:?}", changes);
            *self.last_ai_call.lock() = Some(Utc::now());
        } else {
            debug!("No significant changes detected");
        }

        (is_significant, changes)
    }

    fn check_calendar_changes(&self, old: &[CalendarEventSummary], new: &[CalendarEventSummary]) -> Vec<SignificantChange> {
        let mut changes = Vec::new();
        
        // Create maps for easier comparison
        let old_map: HashMap<&str, &CalendarEventSummary> = old.iter()
            .map(|e| (e.id.as_str(), e))
            .collect();
        let new_map: HashMap<&str, &CalendarEventSummary> = new.iter()
            .map(|e| (e.id.as_str(), e))
            .collect();

        // Check for new events
        for (id, event) in &new_map {
            if !old_map.contains_key(id) {
                changes.push(SignificantChange::NewCalendarEvent(event.title.clone()));
            }
        }

        // Check for cancelled events
        for (id, event) in &old_map {
            if !new_map.contains_key(id) {
                changes.push(SignificantChange::CancelledCalendarEvent(event.title.clone()));
            }
        }

        // Check for changed events
        for (id, new_event) in &new_map {
            if let Some(old_event) = old_map.get(id) {
                // Check time changes
                let time_diff = (new_event.start_time - old_event.start_time).num_minutes() as f64 / 60.0;
                if time_diff.abs() > 1.0 { // More than 1 hour difference
                    changes.push(SignificantChange::EventTimeChanged {
                        event_id: (*id).to_string(),
                        time_diff_hours: time_diff,
                    });
                }

                // Check location changes
                if old_event.location != new_event.location {
                    changes.push(SignificantChange::EventLocationChanged {
                        event_id: (*id).to_string(),
                    });
                }
            }
        }

        changes
    }

    fn check_weather_changes(&self, old: &WeatherSummary, new: &WeatherSummary) -> Vec<SignificantChange> {
        let mut changes = Vec::new();

        // Check condition changes (sunny to rainy, etc)
        if old.condition != new.condition {
            changes.push(SignificantChange::WeatherConditionChanged {
                from: old.condition.clone(),
                to: new.condition.clone(),
            });
        }

        // Check significant temperature changes (>5 degrees)
        let temp_diff = (new.temperature - old.temperature).abs();
        if temp_diff > 5 {
            changes.push(SignificantChange::WeatherTemperatureChanged {
                diff: new.temperature - old.temperature,
            });
        }

        changes
    }

    fn check_task_changes(&self, old: &[TaskSummary], new: &[TaskSummary]) -> Vec<SignificantChange> {
        let mut changes = Vec::new();
        
        let old_map: HashMap<&str, &TaskSummary> = old.iter()
            .map(|t| (t.id.as_str(), t))
            .collect();
        let new_map: HashMap<&str, &TaskSummary> = new.iter()
            .map(|t| (t.id.as_str(), t))
            .collect();

        // Check for new tasks
        for (id, task) in &new_map {
            if !old_map.contains_key(id) {
                changes.push(SignificantChange::NewTask(task.title.clone()));
            }
        }

        // Check for completed tasks
        for (id, new_task) in &new_map {
            if let Some(old_task) = old_map.get(id) {
                if !old_task.completed && new_task.completed {
                    changes.push(SignificantChange::TaskCompleted(new_task.title.clone()));
                }

                // Check due date changes
                if let (Some(old_due), Some(new_due)) = (old_task.due, new_task.due) {
                    let time_diff = (new_due - old_due).num_minutes() as f64 / 60.0;
                    if time_diff.abs() > 1.0 {
                        changes.push(SignificantChange::TaskDueChanged {
                            task_id: (*id).to_string(),
                            time_diff_hours: time_diff,
                        });
                    }
                }
            }
        }

        changes
    }

    /// Record that an AI call was made (used by heartbeat triggers to respect cooldown)
    pub fn record_ai_call(&self) {
        *self.last_ai_call.lock() = Some(Utc::now());
    }

    /// Force the next context to be considered significant (useful after cache clear)
    pub fn reset(&self) {
        *self.last_snapshot.lock() = None;
        *self.last_ai_call.lock() = None;
        info!("Significance engine reset - next context will be considered significant");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_context_is_significant() {
        let engine = SignificanceEngine::new();
        let snapshot = ContextSnapshot {
            calendar_events: vec![],
            weather: None,
            tasks: vec![],
            notes_context: None,
            weather_context: None,
            timestamp: Utc::now(),
            context_hash: "test".to_string(),
        };

        let (is_significant, changes) = engine.analyze_context(snapshot);
        assert!(is_significant);
        assert_eq!(changes, vec![SignificantChange::InitialContext]);
    }

    #[test]
    fn test_new_calendar_event_is_significant() {
        let engine = SignificanceEngine::new();
        
        let snapshot1 = ContextSnapshot {
            calendar_events: vec![],
            weather: None,
            tasks: vec![],
            notes_context: None,
            weather_context: None,
            timestamp: Utc::now(),
            context_hash: "test1".to_string(),
        };
        engine.analyze_context(snapshot1);

        let snapshot2 = ContextSnapshot {
            calendar_events: vec![CalendarEventSummary {
                id: "1".to_string(),
                title: "Meeting".to_string(),
                start_time: Utc::now() + Duration::hours(1),
                end_time: None,
                location: None,
                is_all_day: false,
            }],
            weather: None,
            tasks: vec![],
            notes_context: None,
            weather_context: None,
            timestamp: Utc::now(),
            context_hash: "test2".to_string(),
        };

        // Bypass time restriction for testing
        *engine.last_ai_call.lock() = None;
        
        let (is_significant, changes) = engine.analyze_context(snapshot2);
        assert!(is_significant);
        assert!(changes.iter().any(|c| matches!(c, SignificantChange::NewCalendarEvent(_))));
    }

    #[test]
    fn test_small_time_change_not_significant() {
        let engine = SignificanceEngine::new();
        
        let event = CalendarEventSummary {
            id: "1".to_string(),
            title: "Meeting".to_string(),
            start_time: Utc::now() + Duration::hours(2),
            end_time: None,
            location: None,
            is_all_day: false,
        };

        let snapshot1 = ContextSnapshot {
            calendar_events: vec![event.clone()],
            weather: None,
            tasks: vec![],
            notes_context: None,
            weather_context: None,
            timestamp: Utc::now(),
            context_hash: "test1".to_string(),
        };
        engine.analyze_context(snapshot1);

        let mut event2 = event;
        event2.start_time = event2.start_time + Duration::minutes(30); // Only 30 min change

        let snapshot2 = ContextSnapshot {
            calendar_events: vec![event2],
            weather: None,
            tasks: vec![],
            notes_context: None,
            weather_context: None,
            timestamp: Utc::now(),
            context_hash: "test2".to_string(),
        };

        // Bypass time restriction for testing
        *engine.last_ai_call.lock() = None;
        
        let (is_significant, changes) = engine.analyze_context(snapshot2);
        assert!(!is_significant);
        assert!(changes.is_empty());
    }
}