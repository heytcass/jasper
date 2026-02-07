use crate::errors::JasperResult;
use crate::database::{Database, Insight};
use crate::significance_engine::{SignificanceEngine, ContextSnapshot as ContextSnapshotSummary};
use crate::context_sources::ContextSourceManager;
use crate::api_manager::ApiManager;
use crate::config::Config;
use crate::google_calendar::GoogleCalendarService;
use crate::new_dbus_service::DbusSignalEmitter;

use std::sync::Arc;
use parking_lot::RwLock;
use tokio::time::{Duration, interval};
use tracing::{info, debug, warn, error};
use chrono::{DateTime, Utc};
use serde_json;

// Trait to detect emoji characters
trait EmojiChar {
    fn is_emoji_char(&self) -> bool;
}

impl EmojiChar for char {
    fn is_emoji_char(&self) -> bool {
        // Simple emoji detection - Unicode ranges for common emojis
        matches!(*self,
            '\u{1F600}'..='\u{1F64F}' | // Emoticons
            '\u{1F300}'..='\u{1F5FF}' | // Misc Symbols and Pictographs
            '\u{1F680}'..='\u{1F6FF}' | // Transport and Map
            '\u{1F700}'..='\u{1F77F}' | // Alchemical Symbols
            '\u{1F780}'..='\u{1F7FF}' | // Geometric Shapes Extended
            '\u{1F800}'..='\u{1F8FF}' | // Supplemental Arrows-C
            '\u{1F900}'..='\u{1F9FF}' | // Supplemental Symbols and Pictographs
            '\u{1FA00}'..='\u{1FA6F}' | // Chess Symbols
            '\u{1FA70}'..='\u{1FAFF}' | // Symbols and Pictographs Extended-A
            '\u{2600}'..='\u{26FF}' |   // Misc symbols
            '\u{2700}'..='\u{27BF}'     // Dingbats
        )
    }
}

/// The new simplified daemon core that only handles backend processing
pub struct SimplifiedDaemonCore {
    database: Database,
    significance_engine: SignificanceEngine,
    context_manager: Arc<tokio::sync::RwLock<ContextSourceManager>>,
    api_manager: ApiManager,
    config: Arc<parking_lot::RwLock<Config>>,

    // Google Calendar sync
    calendar_service: Option<Arc<GoogleCalendarService>>,
    last_calendar_sync: Arc<RwLock<Option<DateTime<Utc>>>>,
    calendar_sync_interval: Duration,

    // Configuration
    check_interval: Duration,

    // State
    is_running: Arc<RwLock<bool>>,

    // D-Bus signal emitter (initialized when D-Bus connection is ready)
    signal_emitter: Arc<tokio::sync::RwLock<Option<DbusSignalEmitter>>>,
}

impl SimplifiedDaemonCore {
    pub fn new(
        database: Database,
        context_manager: ContextSourceManager,
        api_manager: ApiManager,
        config: Arc<parking_lot::RwLock<Config>>,
        calendar_service: Option<GoogleCalendarService>,
    ) -> Self {
        let calendar_sync_interval = {
            let cfg = config.read();
            let minutes = cfg.google_calendar.as_ref()
                .map(|gc| gc.sync_interval_minutes)
                .unwrap_or(15);
            Duration::from_secs(minutes as u64 * 60)
        };

        Self {
            database,
            significance_engine: SignificanceEngine::new(),
            context_manager: Arc::new(tokio::sync::RwLock::new(context_manager)),
            api_manager,
            config,
            calendar_service: calendar_service.map(Arc::new),
            last_calendar_sync: Arc::new(RwLock::new(None)),
            calendar_sync_interval,
            check_interval: Duration::from_secs(60), // Check every minute
            is_running: Arc::new(RwLock::new(false)),
            signal_emitter: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    /// Initialize the D-Bus signal emitter (call after D-Bus connection is established)
    pub async fn init_signal_emitter(&self) -> JasperResult<()> {
        match DbusSignalEmitter::new().await {
            Ok(emitter) => {
                *self.signal_emitter.write().await = Some(emitter);
                info!("D-Bus signal emitter initialized");
                Ok(())
            }
            Err(e) => {
                warn!("Failed to initialize D-Bus signal emitter: {}", e);
                Err(e)
            }
        }
    }

    /// Emit an insight updated signal
    async fn emit_insight_signal(&self, insight_id: i64, emoji: &str, preview: &str) {
        if let Some(ref emitter) = *self.signal_emitter.read().await {
            if let Err(e) = emitter.emit_insight_updated(insight_id, emoji, preview).await {
                warn!("Failed to emit InsightUpdated signal: {}", e);
            }
        } else {
            debug!("Signal emitter not initialized, skipping signal emission");
        }
    }

    /// Start the daemon main loop
    /// Takes an Arc to self so it can release locks between iterations
    pub async fn start_with_arc(daemon: Arc<tokio::sync::RwLock<Self>>) -> JasperResult<()> {
        // Check and set running state
        {
            let d = daemon.read().await;
            let mut running = d.is_running.write();
            if *running {
                warn!("Daemon is already running");
                return Ok(());
            }
            *running = true;
        }

        info!("Starting simplified daemon core");

        // Give initial grace period for frontends to connect
        info!("Waiting for frontends to connect...");
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        // Get check interval from daemon (briefly acquire lock)
        let check_interval = daemon.read().await.check_interval;
        let mut ticker = interval(check_interval);

        loop {
            // Check if we should still be running (briefly acquire lock)
            {
                let d = daemon.read().await;
                if !*d.is_running.read() {
                    info!("Daemon stop requested");
                    break;
                }
            }

            // Check if any frontends are still active
            let has_frontends = {
                let d = daemon.read().await;
                d.database.has_active_frontends().unwrap_or(true)
            };

            if !has_frontends {
                info!("No active frontends detected - stopping daemon");
                // Give a grace period to allow reconnection
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

                // Recheck after grace period
                let still_no_frontends = {
                    let d = daemon.read().await;
                    !d.database.has_active_frontends().unwrap_or(false)
                };
                if still_no_frontends {
                    break;
                }
            }

            ticker.tick().await;

            // Sync Google Calendar events if interval has elapsed
            {
                let d = daemon.read().await;
                d.sync_calendar_if_needed().await;
            }

            // Perform context check and analysis (this acquires locks internally as needed)
            {
                let d = daemon.read().await;
                if let Err(e) = d.check_and_analyze().await {
                    error!("Error during context check and analysis: {}", e);
                }
            }
        }

        // Clear running state
        {
            let d = daemon.read().await;
            let mut running = d.is_running.write();
            *running = false;
        }

        info!("Simplified daemon core stopped");
        Ok(())
    }

    /// Stop the daemon (available for graceful shutdown)
    #[allow(dead_code)]
    pub fn stop(&self) {
        info!("Stopping daemon");
        let mut running = self.is_running.write();
        *running = false;
    }

    /// Sync Google Calendar events if the sync interval has elapsed
    async fn sync_calendar_if_needed(&self) {
        let calendar_service = match &self.calendar_service {
            Some(svc) => svc.clone(),
            None => return,
        };

        // Check if sync interval has elapsed
        {
            let last_sync = self.last_calendar_sync.read();
            if let Some(last) = *last_sync {
                if Utc::now() - last < chrono::Duration::from_std(self.calendar_sync_interval).unwrap_or(chrono::Duration::minutes(15)) {
                    return;
                }
            }
        }

        // Check if authenticated
        if !calendar_service.is_authenticated().await {
            debug!("Google Calendar not authenticated, skipping sync");
            return;
        }

        info!("Starting Google Calendar sync");

        let now = Utc::now();
        let planning_horizon = self.config.read().get_planning_horizon();
        let end_time = now + planning_horizon;

        match calendar_service.fetch_events(now, end_time).await {
            Ok(events_by_calendar) => {
                let mut total_events = 0usize;

                for (google_calendar_id, events) in &events_by_calendar {
                    // Ensure calendar exists in DB and get its DB id
                    let calendar_name = google_calendar_id.clone();
                    let db_calendar_id = match self.database.create_or_update_calendar(
                        google_calendar_id,
                        &calendar_name,
                        Some("google_calendar"),
                    ) {
                        Ok(id) => id,
                        Err(e) => {
                            warn!("Failed to create/update calendar {}: {}", google_calendar_id, e);
                            continue;
                        }
                    };

                    // Delete old events for this calendar then bulk-insert fresh ones
                    if let Err(e) = self.database.delete_events_for_calendar(db_calendar_id) {
                        warn!("Failed to delete old events for calendar {}: {}", google_calendar_id, e);
                        continue;
                    }

                    // Set the calendar_id on each event before inserting
                    let events_with_cal_id: Vec<_> = events.iter().map(|e| {
                        let mut ev = e.clone();
                        ev.calendar_id = db_calendar_id;
                        ev
                    }).collect();

                    match self.database.create_events_bulk(&events_with_cal_id) {
                        Ok(ids) => {
                            total_events += ids.len();
                            debug!("Synced {} events for calendar {}", ids.len(), google_calendar_id);
                        }
                        Err(e) => {
                            warn!("Failed to insert events for calendar {}: {}", google_calendar_id, e);
                        }
                    }
                }

                info!("Google Calendar sync complete: {} events", total_events);

                // Update last sync timestamp
                *self.last_calendar_sync.write() = Some(Utc::now());
            }
            Err(e) => {
                warn!("Google Calendar sync failed: {}", e);
            }
        }
    }

    /// Check context for changes and analyze if significant
    async fn check_and_analyze(&self) -> JasperResult<()> {
        debug!("Checking context for significant changes");

        // Collect current context from all sources
        let current_context = self.collect_current_context().await?;
        
        // Check if changes are significant
        let (is_significant, changes) = self.significance_engine.analyze_context(current_context.clone());

        if is_significant {
            info!("Significant changes detected: {:?}", changes);
            
            // Call AI for analysis
            match self.analyze_with_ai(&current_context).await {
                Ok(insight) => {
                    // Store the insight
                    match self.database.store_insight(&insight.emoji, &insight.text, Some(&insight.context_hash)) {
                        Ok(insight_id) => {
                            info!("Stored new insight with ID: {}", insight_id);
                            
                            // Store the context snapshot that triggered this insight
                            let snapshot_json = serde_json::to_string(&current_context)
                                .unwrap_or_else(|_| "{}".to_string());
                            
                            if let Err(e) = self.database.store_context_snapshot(
                                insight_id,
                                "combined",
                                &snapshot_json,
                                None,
                            ) {
                                warn!("Failed to store context snapshot: {}", e);
                            }

                            // Emit D-Bus signal to notify frontends of new insight
                            self.emit_insight_signal(insight_id, &insight.emoji, &insight.text).await;
                        }
                        Err(e) => {
                            error!("Failed to store insight: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("AI analysis failed: {}", e);
                }
            }
        } else {
            debug!("No significant changes detected");
        }

        Ok(())
    }

    /// Collect current context from all sources
    async fn collect_current_context(&self) -> JasperResult<ContextSnapshotSummary> {
        let now = Utc::now();
        let end_time = now + chrono::Duration::hours(24);

        // Get calendar events for next 24 hours from database
        let calendar_events: Vec<_> = self.database.get_events_in_range(now, end_time)?
            .into_iter()
            .map(|event| crate::significance_engine::CalendarEventSummary {
                id: event.source_id,
                title: event.title.unwrap_or_default(),
                start_time: DateTime::from_timestamp(event.start_time, 0).unwrap_or_default(),
                end_time: event.end_time.map(|ts| DateTime::from_timestamp(ts, 0).unwrap_or_default()),
                location: event.location,
            })
            .collect();

        // Collect additional context from all enabled context sources
        let context_data = match self.context_manager.read().await.fetch_all_context(now, end_time).await {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to fetch context from sources: {}", e);
                Vec::new()
            }
        };

        // Extract weather and tasks from context data
        let mut weather: Option<crate::significance_engine::WeatherSummary> = None;
        let mut tasks: Vec<crate::significance_engine::TaskSummary> = Vec::new();

        for ctx in &context_data {
            match &ctx.content {
                crate::context_sources::ContextContent::Weather(weather_ctx) => {
                    // Parse temperature from forecast or conditions
                    if let Some(forecast) = weather_ctx.forecast.first() {
                        weather = Some(crate::significance_engine::WeatherSummary {
                            condition: forecast.conditions.clone(),
                            temperature: forecast.temperature_high as i32,
                            feels_like: forecast.temperature_high as i32,
                        });
                    }
                }
                crate::context_sources::ContextContent::Tasks(task_ctx) => {
                    tasks.extend(task_ctx.tasks.iter().map(|t| {
                        crate::significance_engine::TaskSummary {
                            id: t.id.clone(),
                            title: t.title.clone(),
                            due: t.due_date,
                            completed: matches!(t.status, crate::context_sources::TaskStatus::Completed),
                        }
                    }));
                }
                crate::context_sources::ContextContent::Notes(notes_ctx) => {
                    // Extract tasks from notes as well
                    tasks.extend(notes_ctx.pending_tasks.iter().map(|t| {
                        crate::significance_engine::TaskSummary {
                            id: t.id.clone(),
                            title: t.title.clone(),
                            due: t.due_date,
                            completed: matches!(t.status, crate::context_sources::TaskStatus::Completed),
                        }
                    }));
                }
                _ => {}
            }
        }

        // Create context hash for comparison
        let context_hash = format!("{:x}", {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            calendar_events.hash(&mut hasher);
            weather.hash(&mut hasher);
            tasks.hash(&mut hasher);
            hasher.finish()
        });

        Ok(ContextSnapshotSummary {
            calendar_events,
            weather,
            tasks,
            timestamp: now,
            context_hash,
        })
    }

    /// Call AI service for analysis
    async fn analyze_with_ai(&self, context: &ContextSnapshotSummary) -> JasperResult<AiInsight> {
        debug!("Calling AI service for context analysis");
        
        // Check if we can make an API call
        match self.api_manager.can_make_api_call() {
            Ok(()) => {
                // Proceed with AI call
                match self.call_anthropic_api(context).await {
                    Ok(insight) => {
                        self.api_manager.record_api_success();
                        self.api_manager.record_api_call(500); // Estimate token usage
                        Ok(insight)
                    }
                    Err(e) => {
                        self.api_manager.record_api_failure(&e.to_string());
                        Err(e)
                    }
                }
            }
            Err(rate_limit_type) => {
                warn!("API call blocked due to rate limiting: {:?}", rate_limit_type);
                // Return a fallback insight
                Ok(AiInsight {
                    emoji: "⏳".to_string(),
                    text: "Rate limited - check back later for fresh insights".to_string(),
                    context_hash: context.context_hash.clone(),
                })
            }
        }
    }

    /// Make actual API call to Anthropic Claude
    async fn call_anthropic_api(&self, context: &ContextSnapshotSummary) -> JasperResult<AiInsight> {
        // Build context summary for AI
        let mut context_summary = String::new();
        
        // Add calendar events
        if !context.calendar_events.is_empty() {
            context_summary.push_str("Calendar Events (next 24h):\n");
            for event in &context.calendar_events {
                context_summary.push_str(&format!(
                    "- {} at {}{}\n",
                    event.title,
                    event.start_time.format("%I:%M %p"),
                    event.location.as_ref().map(|l| format!(" (at {})", l)).unwrap_or_default()
                ));
            }
            context_summary.push('\n');
        }
        
        // Add weather if available
        if let Some(weather) = &context.weather {
            context_summary.push_str(&format!("Weather: {} ({}°F)\n\n", weather.condition, weather.temperature));
        }
        
        // Add tasks if available
        if !context.tasks.is_empty() {
            context_summary.push_str("Upcoming Tasks:\n");
            for task in &context.tasks {
                context_summary.push_str(&format!("- {}\n", task.title));
            }
            context_summary.push('\n');
        }
        
        if context_summary.is_empty() {
            context_summary = "No significant context available.".to_string();
        }
        
        // Make API call to Anthropic
        let client = reqwest::Client::new();

        // Get API key from config (respects SOPS secrets and env var fallback)
        let api_key = self.config.read().get_api_key()
            .ok_or_else(|| crate::errors::JasperError::authentication("anthropic", "API key not configured. Set via config, SOPS secrets, or ANTHROPIC_API_KEY environment variable."))?;

        // Get model and max_tokens from config
        let (_, model, max_tokens, _) = self.config.read().get_api_config();

        let prompt = format!(
            "You are Jasper, a helpful AI assistant. Analyze this context and provide a brief, actionable insight with an appropriate emoji.

Context:
{}

Provide response in this format:
Emoji: [single emoji]
Insight: [brief actionable insight in 1-2 sentences]",
            context_summary
        );

        let request_body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens.min(200), // Cap at 200 for insights
            "messages": [{
                "role": "user",
                "content": prompt
            }]
        });
        
        let response = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| crate::errors::JasperError::Internal { message: format!("API request failed: {}", e) })?;
        
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::errors::JasperError::Internal {
                message: format!("API call failed with status {}: {}", status, error_text)
            });
        }
        
        let response_json: serde_json::Value = response.json().await
            .map_err(|e| crate::errors::JasperError::Internal { message: format!("Failed to parse response: {}", e) })?;
        
        let content = response_json
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("text"))
            .and_then(|text| text.as_str())
            .ok_or_else(|| crate::errors::JasperError::Internal { message: "Invalid API response format".to_string() })?;
        
        // Parse the response
        let (emoji, insight) = self.parse_ai_response(content);
        
        Ok(AiInsight {
            emoji,
            text: insight,
            context_hash: context.context_hash.clone(),
        })
    }
    
    /// Parse AI response to extract emoji and insight
    fn parse_ai_response(&self, content: &str) -> (String, String) {
        let lines: Vec<&str> = content.lines().collect();
        let mut emoji = "🤖".to_string();
        let mut insight = content.to_string();
        
        for line in &lines {
            if line.starts_with("Emoji:") {
                emoji = line.replace("Emoji:", "").trim().to_string();
                if emoji.is_empty() {
                    emoji = "🤖".to_string();
                }
            } else if line.starts_with("Insight:") {
                insight = line.replace("Insight:", "").trim().to_string();
            }
        }
        
        // If we couldn't parse the format, use the full content as insight
        if insight == content && !content.is_empty() {
            // Try to extract the first emoji from the content
            let chars: Vec<char> = content.chars().collect();
            for ch in &chars {
                if ch.is_emoji_char() {
                    emoji = ch.to_string();
                    break;
                }
            }
            // Remove the emoji from the insight if found
            insight = content.replace(&emoji, "").trim().to_string();
        }
        
        if insight.is_empty() {
            insight = "AI analysis complete - check your schedule and priorities".to_string();
        }
        
        (emoji, insight)
    }

    /// Get the latest insight from database
    pub fn get_latest_insight(&self) -> JasperResult<Option<Insight>> {
        self.database.get_latest_insight()
    }

    /// Get insight by ID
    pub fn get_insight_by_id(&self, insight_id: i64) -> JasperResult<Option<Insight>> {
        self.database.get_insight_by_id(insight_id)
    }

    /// Register a frontend as active
    pub fn register_frontend(&self, frontend_id: &str, pid: Option<i32>) -> JasperResult<()> {
        info!("Registering frontend: {}", frontend_id);
        self.database.register_frontend(frontend_id, pid)
    }

    /// Unregister a frontend
    pub fn unregister_frontend(&self, frontend_id: &str) -> JasperResult<()> {
        info!("Unregistering frontend: {}", frontend_id);
        self.database.unregister_frontend(frontend_id)
    }

    /// Update frontend heartbeat
    pub fn update_frontend_heartbeat(&self, frontend_id: &str) -> JasperResult<()> {
        self.database.update_frontend_heartbeat(frontend_id)
    }

    /// Force an immediate context check and analysis
    pub async fn force_refresh(&self) -> JasperResult<()> {
        info!("Forcing immediate context refresh");
        self.check_and_analyze().await
    }

    /// Reset significance engine (useful after cache clear)
    #[allow(dead_code)]
    pub fn reset_significance_engine(&self) {
        info!("Resetting significance engine");
        self.significance_engine.reset();
    }
}

/// Simplified AI insight result
#[derive(Debug, Clone)]
struct AiInsight {
    emoji: String,
    text: String,
    context_hash: String,
}

/// Daemon status information
#[derive(Debug)]
pub struct DaemonStatus {
    pub is_running: bool,
    pub active_frontends: usize,
    #[allow(dead_code)] // TODO: Track last analysis time
    pub last_analysis: Option<DateTime<Utc>>,
    pub insights_count: i64,
}

impl SimplifiedDaemonCore {
    /// Get daemon status
    pub async fn get_status(&self) -> JasperResult<DaemonStatus> {
        let is_running = *self.is_running.read();
        let active_frontends = self.database.get_active_frontends()?.len();
        
        // Get insights count from database
        let insights_count = 0; // TODO: Add method to count insights
        
        Ok(DaemonStatus {
            is_running,
            active_frontends,
            last_analysis: None, // TODO: Track last analysis time
            insights_count,
        })
    }
}