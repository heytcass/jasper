use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{debug, warn, info, error};
use anyhow::{Result, anyhow};
use std::time::{Duration as StdDuration};
use tokio::time::sleep;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SimpleInsightCache {
    last_insight: Option<String>,
    cached_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCallStats {
    calls_today: u32,
    daily_limit: u32,
    last_reset: DateTime<Utc>,
    total_calls: u64,
    total_tokens_used: u64,
    // Rate limiting fields
    calls_this_minute: u32,
    minute_reset: DateTime<Utc>,
    per_minute_limit: u32,
    // Retry/backoff fields
    consecutive_failures: u32,
    last_failure: Option<DateTime<Utc>>,
    next_allowed_attempt: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub enum RateLimitType {
    Daily,
    PerMinute,
    Backoff,
    CircuitBreaker,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub daily_limit: u32,
    pub per_minute_limit: u32,
    pub max_retry_attempts: u32,
    pub base_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub circuit_failure_threshold: u32,
    pub circuit_recovery_timeout_minutes: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            daily_limit: 200,
            per_minute_limit: 10,
            max_retry_attempts: 3,
            base_backoff_ms: 1000,      // 1 second base
            max_backoff_ms: 60000,      // 60 seconds max
            circuit_failure_threshold: 5,
            circuit_recovery_timeout_minutes: 15,
        }
    }
}

#[derive(Clone)]
pub struct ApiManager {
    cache: Arc<RwLock<SimpleInsightCache>>,
    stats: Arc<RwLock<ApiCallStats>>,
    config: RateLimitConfig,
}

impl ApiManager {
    pub fn new() -> Self {
        Self::with_config(RateLimitConfig::default())
    }
    
    pub fn with_config(config: RateLimitConfig) -> Self {
        let now = Utc::now();
        let stats = ApiCallStats {
            calls_today: 0,
            daily_limit: config.daily_limit,
            last_reset: now,
            total_calls: 0,
            total_tokens_used: 0,
            calls_this_minute: 0,
            minute_reset: now,
            per_minute_limit: config.per_minute_limit,
            consecutive_failures: 0,
            last_failure: None,
            next_allowed_attempt: now,
        };

        Self {
            cache: Arc::new(RwLock::new(SimpleInsightCache {
                last_insight: None,
                cached_at: None,
            })),
            stats: Arc::new(RwLock::new(stats)),
            config,
        }
    }

    pub fn can_make_api_call(&self) -> Result<(), RateLimitType> {
        let mut stats = self.stats.write();
        let now = Utc::now();
        
        // Check if we're in backoff period due to failures
        if now < stats.next_allowed_attempt {
            let wait_seconds = (stats.next_allowed_attempt - now).num_seconds();
            debug!("API call blocked by backoff, wait {} seconds", wait_seconds);
            return Err(RateLimitType::Backoff);
        }
        
        // Check circuit breaker
        if stats.consecutive_failures >= self.config.circuit_failure_threshold {
            if let Some(last_failure) = stats.last_failure {
                let recovery_time = last_failure + Duration::minutes(self.config.circuit_recovery_timeout_minutes as i64);
                if now < recovery_time {
                    debug!("Circuit breaker open, blocking API calls");
                    return Err(RateLimitType::CircuitBreaker);
                } else {
                    info!("Circuit breaker recovery period expired, allowing test call");
                    // Reset for half-open state
                    stats.consecutive_failures = 0;
                }
            }
        }
        
        // Reset daily counter if it's a new day
        if now.date_naive() != stats.last_reset.date_naive() {
            info!("Daily API call counter reset. Used {} calls yesterday.", stats.calls_today);
            stats.calls_today = 0;
            stats.last_reset = now;
        }
        
        // Reset minute counter if it's a new minute
        if (now - stats.minute_reset).num_seconds() >= 60 {
            stats.calls_this_minute = 0;
            stats.minute_reset = now;
        }
        
        // Check daily limit
        if stats.calls_today >= stats.daily_limit {
            warn!("Daily API call limit reached ({}/{})", stats.calls_today, stats.daily_limit);
            return Err(RateLimitType::Daily);
        }
        
        // Check per-minute limit
        if stats.calls_this_minute >= stats.per_minute_limit {
            warn!("Per-minute API call limit reached ({}/{})", stats.calls_this_minute, stats.per_minute_limit);
            return Err(RateLimitType::PerMinute);
        }
        
        Ok(())
    }

    pub fn record_api_call(&self, tokens_used: u64) {
        let mut stats = self.stats.write();
        stats.calls_today += 1;
        stats.calls_this_minute += 1;
        stats.total_calls += 1;
        stats.total_tokens_used += tokens_used;
        
        debug!("API call recorded. Today: {}/{}, This minute: {}/{}, Total: {}, Tokens: {}", 
               stats.calls_today, stats.daily_limit, stats.calls_this_minute, stats.per_minute_limit,
               stats.total_calls, stats.total_tokens_used);
        
        // Warn when approaching limits
        if stats.calls_today >= (stats.daily_limit as f32 * 0.8) as u32 {
            warn!("Approaching daily API limit: {}/{}", stats.calls_today, stats.daily_limit);
        }
        if stats.calls_this_minute >= (stats.per_minute_limit as f32 * 0.8) as u32 {
            warn!("Approaching per-minute API limit: {}/{}", stats.calls_this_minute, stats.per_minute_limit);
        }
    }
    
    pub fn record_api_success(&self) {
        let mut stats = self.stats.write();
        if stats.consecutive_failures > 0 {
            info!("API call succeeded after {} failures, resetting backoff", stats.consecutive_failures);
            stats.consecutive_failures = 0;
            stats.next_allowed_attempt = Utc::now();
        }
    }
    
    pub fn record_api_failure(&self, error: &str) {
        let mut stats = self.stats.write();
        stats.consecutive_failures += 1;
        stats.last_failure = Some(Utc::now());
        
        // Calculate exponential backoff
        let backoff_ms = std::cmp::min(
            self.config.base_backoff_ms * (2_u64.pow(stats.consecutive_failures.saturating_sub(1))),
            self.config.max_backoff_ms,
        );
        
        stats.next_allowed_attempt = Utc::now() + Duration::milliseconds(backoff_ms as i64);
        
        warn!("API call failed (attempt {}): {}. Next attempt allowed in {}ms", 
              stats.consecutive_failures, error, backoff_ms);
        
        // Log circuit breaker activation
        if stats.consecutive_failures >= self.config.circuit_failure_threshold {
            error!("Circuit breaker activated after {} failures. Blocking API calls for {} minutes",
                  stats.consecutive_failures, self.config.circuit_recovery_timeout_minutes);
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

    /// Execute an API call with automatic retry and backoff
    pub async fn execute_with_retry<F, Fut, T>(&self, operation: F) -> Result<T>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<T>> + Send,
        T: Send,
    {
        let mut attempt = 0;
        let max_attempts = self.config.max_retry_attempts;
        
        loop {
            // Check rate limits before attempting
            match self.can_make_api_call() {
                Ok(()) => {
                    // Allowed to proceed
                }
                Err(RateLimitType::Daily) => {
                    return Err(anyhow!("Daily API limit exceeded"));
                }
                Err(RateLimitType::PerMinute) => {
                    let wait_time = 60 - (Utc::now() - self.stats.read().minute_reset).num_seconds() + 1;
                    info!("Per-minute limit reached, waiting {} seconds", wait_time);
                    sleep(StdDuration::from_secs(wait_time as u64)).await;
                    continue;
                }
                Err(RateLimitType::Backoff) => {
                    let wait_ms = (self.stats.read().next_allowed_attempt - Utc::now()).num_milliseconds().max(0);
                    info!("Backoff active, waiting {}ms", wait_ms);
                    sleep(StdDuration::from_millis(wait_ms as u64)).await;
                    continue;
                }
                Err(RateLimitType::CircuitBreaker) => {
                    return Err(anyhow!("Circuit breaker is open, API calls blocked"));
                }
            }
            
            attempt += 1;
            debug!("Attempting API call (attempt {}/{})", attempt, max_attempts);
            
            match operation().await {
                Ok(result) => {
                    self.record_api_success();
                    return Ok(result);
                }
                Err(e) => {
                    self.record_api_failure(&e.to_string());
                    
                    if attempt >= max_attempts {
                        error!("API call failed after {} attempts: {}", max_attempts, e);
                        return Err(e);
                    }
                    
                    // Wait with exponential backoff before retry
                    let backoff_ms = std::cmp::min(
                        self.config.base_backoff_ms * (2_u64.pow(attempt - 1)),
                        self.config.max_backoff_ms,
                    );
                    
                    info!("Retrying in {}ms (attempt {}/{})", backoff_ms, attempt, max_attempts);
                    sleep(StdDuration::from_millis(backoff_ms)).await;
                }
            }
        }
    }
    
    /// Get current API statistics
    pub fn get_stats(&self) -> ApiCallStats {
        self.stats.read().clone()
    }
    
    /// Get wait time for next allowed API call
    pub fn get_wait_time(&self) -> Option<StdDuration> {
        let stats = self.stats.read();
        let now = Utc::now();
        
        if now < stats.next_allowed_attempt {
            let wait_ms = (stats.next_allowed_attempt - now).num_milliseconds();
            Some(StdDuration::from_millis(wait_ms.max(0) as u64))
        } else {
            None
        }
    }
    
    /// Check if circuit breaker is open
    pub fn is_circuit_open(&self) -> bool {
        let stats = self.stats.read();
        if stats.consecutive_failures >= self.config.circuit_failure_threshold {
            if let Some(last_failure) = stats.last_failure {
                let recovery_time = last_failure + Duration::minutes(self.config.circuit_recovery_timeout_minutes as i64);
                return Utc::now() < recovery_time;
            }
        }
        false
    }
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
        assert!(manager.can_make_api_call().is_ok());
        
        // Exhaust daily limit
        {
            let mut stats = manager.stats.write();
            stats.calls_today = stats.daily_limit;
        }
        
        // Should deny further calls
        assert!(matches!(manager.can_make_api_call(), Err(RateLimitType::Daily)));
    }
    
    #[test]
    fn test_per_minute_limit_enforcement() {
        let manager = ApiManager::new();
        
        // Exhaust per-minute limit
        {
            let mut stats = manager.stats.write();
            stats.calls_this_minute = stats.per_minute_limit;
        }
        
        // Should deny further calls
        assert!(matches!(manager.can_make_api_call(), Err(RateLimitType::PerMinute)));
    }
    
    #[test]
    fn test_circuit_breaker() {
        let config = RateLimitConfig {
            circuit_failure_threshold: 2,
            base_backoff_ms: 0,  // Disable backoff to test circuit breaker specifically
            ..Default::default()
        };
        let manager = ApiManager::with_config(config);
        
        // Record failures
        manager.record_api_failure("test error 1");
        manager.record_api_failure("test error 2");
        
        // Circuit should be open
        assert!(manager.is_circuit_open());
        assert!(matches!(manager.can_make_api_call(), Err(RateLimitType::CircuitBreaker)));
    }
    
    #[test]
    fn test_exponential_backoff() {
        let manager = ApiManager::new();
        
        // Record a failure
        manager.record_api_failure("test error");
        
        // Should be in backoff period
        assert!(matches!(manager.can_make_api_call(), Err(RateLimitType::Backoff)));
        
        // Wait time should be available
        assert!(manager.get_wait_time().is_some());
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