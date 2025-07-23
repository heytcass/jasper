use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{debug, warn, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SimpleInsightCache {
    last_insight: Option<String>,
    cached_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApiCallStats {
    calls_today: u32,
    daily_limit: u32,
    last_reset: DateTime<Utc>,
    total_calls: u64,
    total_tokens_used: u64,
}

#[derive(Clone)]
pub struct ApiManager {
    cache: Arc<RwLock<SimpleInsightCache>>,
    stats: Arc<RwLock<ApiCallStats>>,
}

impl ApiManager {
    pub fn new() -> Self {
        let stats = ApiCallStats {
            calls_today: 0,
            daily_limit: 200, // 200 calls per day to stay within budget
            last_reset: Utc::now(),
            total_calls: 0,
            total_tokens_used: 0,
        };

        Self {
            cache: Arc::new(RwLock::new(SimpleInsightCache {
                last_insight: None,
                cached_at: None,
            })),
            stats: Arc::new(RwLock::new(stats)),
        }
    }

    pub fn can_make_api_call(&self) -> bool {
        let mut stats = self.stats.write();
        
        // Reset daily counter if it's a new day
        let now = Utc::now();
        if now.date_naive() != stats.last_reset.date_naive() {
            stats.calls_today = 0;
            stats.last_reset = now;
            info!("Daily API call counter reset. Used {} calls yesterday.", stats.calls_today);
        }
        
        let can_call = stats.calls_today < stats.daily_limit;
        if !can_call {
            warn!("Daily API call limit reached ({}/{}). Using cached or fallback responses.", 
                  stats.calls_today, stats.daily_limit);
        }
        
        can_call
    }

    pub fn record_api_call(&self, tokens_used: u64) {
        let mut stats = self.stats.write();
        stats.calls_today += 1;
        stats.total_calls += 1;
        stats.total_tokens_used += tokens_used;
        
        debug!("API call recorded. Today: {}/{}, Total: {}, Tokens: {}", 
               stats.calls_today, stats.daily_limit, stats.total_calls, stats.total_tokens_used);
        
        // Warn when approaching daily limit
        if stats.calls_today >= (stats.daily_limit as f32 * 0.8) as u32 {
            warn!("Approaching daily API limit: {}/{}", stats.calls_today, stats.daily_limit);
        }
    }

    pub fn get_last_insight(&self) -> Option<String> {
        let cache = self.cache.read();
        
        if let Some(ref insight) = cache.last_insight {
            debug!("Using last cached insight");
            Some(insight.clone())
        } else {
            debug!("No cached insight available");
            None
        }
    }

    pub fn cache_insight(&self, insight: String) {
        debug!("Caching latest insight");
        let mut cache = self.cache.write();
        cache.last_insight = Some(insight);
        cache.cached_at = Some(Utc::now());
        debug!("Cached latest insight");
    }


    pub fn create_calendar_hash(&self, events: &[impl std::fmt::Debug]) -> String {
        // Create a simple hash of the calendar events
        // In practice, you'd want a proper hash function
        let events_string = format!("{:?}", events);
        format!("{:x}", md5::compute(events_string.as_bytes()))
    }

    pub fn create_context_hash(&self, events: &[impl std::fmt::Debug], additional_context: &[crate::context_sources::ContextData]) -> String {
        use crate::context_sources::ContextContent;
        
        // Create semantically meaningful hash that focuses on important changes
        let mut hash_components = Vec::new();
        
        // Hash calendar events (existing logic for compatibility)
        let events_string = format!("{:?}", events);
        hash_components.push(events_string);
        
        // Hash each context source focusing on meaningful fields only
        for context in additional_context {
            let context_summary = match &context.content {
                ContextContent::Calendar(cal_ctx) => {
                    // Focus on event times, titles, conflicts - ignore metadata
                    format!("cal:{}:{}:{}",
                        cal_ctx.events.len(),
                        cal_ctx.conflicts.join("|"),
                        cal_ctx.upcoming_deadlines.join("|")
                    )
                },
                ContextContent::Tasks(task_ctx) => {
                    // Focus on task counts and urgency - ignore full descriptions
                    format!("tasks:{}:{}:{}",
                        task_ctx.tasks.len(),
                        task_ctx.overdue_count,
                        task_ctx.upcoming_count
                    )
                },
                ContextContent::Notes(notes_ctx) => {
                    // Focus on project status and relationship alerts - ignore full content
                    let project_statuses: Vec<String> = notes_ctx.active_projects.iter()
                        .map(|p| format!("{}:{:?}:{}", p.name, p.status, p.progress))
                        .collect();
                    let alert_urgencies: Vec<String> = notes_ctx.relationship_alerts.iter()
                        .map(|a| format!("{}:{}", a.person_name, a.urgency))
                        .collect();
                    format!("notes:{}:{}:{}:{}",
                        notes_ctx.daily_notes.len(),
                        project_statuses.join("|"),
                        notes_ctx.pending_tasks.len(),
                        alert_urgencies.join("|")
                    )
                },
                ContextContent::Weather(weather_ctx) => {
                    // Focus on significant conditions and alerts - ignore minor temp changes
                    format!("weather:{}:{}",
                        weather_ctx.current_conditions,
                        weather_ctx.alerts.join("|")
                    )
                },
                ContextContent::Generic(generic_ctx) => {
                    // Use summary for generic context
                    format!("generic:{}:{}", context.source_id, generic_ctx.summary)
                }
            };
            
            hash_components.push(format!("{}:{}", context.source_id, context_summary));
        }
        
        // Create final hash
        let combined_string = hash_components.join(":");
        format!("{:x}", md5::compute(combined_string.as_bytes()))
    }

    pub fn clear_cache(&self) {
        let mut cache = self.cache.write();
        cache.last_insight = None;
        cache.cached_at = None;
        debug!("Cache cleared");
    }

    // API stats and cost estimation methods removed - not currently used
}

impl Default for ApiManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daily_limit_enforcement() {
        let manager = ApiManager::new();
        
        // Should allow calls initially
        assert!(manager.can_make_api_call());
        
        // Exhaust daily limit
        {
            let mut stats = manager.stats.write();
            stats.calls_today = stats.daily_limit;
        }
        
        // Should deny further calls
        assert!(!manager.can_make_api_call());
    }

    #[test]
    fn test_caching() {
        let manager = ApiManager::new();
        let hash = "test_hash".to_string();
        let insight = "Test insight".to_string();
        
        // Should return None initially
        assert!(manager.get_last_insight().is_none());
        
        // Cache an insight
        manager.cache_insight(insight.clone());
        
        // Should return cached insight
        assert_eq!(manager.get_last_insight(), Some(insight));
    }
}