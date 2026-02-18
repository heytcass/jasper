use crate::api_manager::ApiManager;
use crate::config::Config;
use crate::context_sources::{self, ContextSourceManager};
use crate::database::{Database, Insight};
use crate::errors::JasperResult;
use crate::google_calendar::GoogleCalendarService;
use crate::new_dbus_service::DbusSignalEmitter;
use crate::significance_engine::{
    ContextSnapshot as ContextSnapshotSummary, SignificanceEngine, SignificantChange,
};

use chrono::{DateTime, Timelike, Utc};
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

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
    http_client: reqwest::Client,

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
            let minutes = cfg
                .google_calendar
                .as_ref()
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
            http_client: reqwest::Client::new(),
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
            if let Err(e) = emitter
                .emit_insight_updated(insight_id, emoji, preview)
                .await
            {
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
                if Utc::now() - last
                    < chrono::Duration::from_std(self.calendar_sync_interval)
                        .unwrap_or(chrono::Duration::minutes(15))
                {
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
                            warn!(
                                "Failed to create/update calendar {}: {}",
                                google_calendar_id, e
                            );
                            continue;
                        }
                    };

                    // Delete old events for this calendar then bulk-insert fresh ones
                    if let Err(e) = self.database.delete_events_for_calendar(db_calendar_id) {
                        warn!(
                            "Failed to delete old events for calendar {}: {}",
                            google_calendar_id, e
                        );
                        continue;
                    }

                    // Set the calendar_id on each event before inserting
                    let events_with_cal_id: Vec<_> = events
                        .iter()
                        .map(|e| {
                            let mut ev = e.clone();
                            ev.calendar_id = db_calendar_id;
                            ev
                        })
                        .collect();

                    match self.database.create_events_bulk(&events_with_cal_id) {
                        Ok(ids) => {
                            total_events += ids.len();
                            debug!(
                                "Synced {} events for calendar {}",
                                ids.len(),
                                google_calendar_id
                            );
                        }
                        Err(e) => {
                            warn!(
                                "Failed to insert events for calendar {}: {}",
                                google_calendar_id, e
                            );
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

    /// Determine the current heartbeat phase based on time of day.
    /// Returns the phase name if we should fire a heartbeat, or None if one already fired this phase.
    fn should_fire_heartbeat(&self) -> Option<String> {
        let tz = self.config.read().get_timezone();
        let local_now = Utc::now().with_timezone(&tz);
        let hour = local_now.hour();

        // Define heartbeat phases: morning (7-8), midday (12-13), evening (18-19)
        let phase = match hour {
            7..=8 => Some("morning"),
            12..=13 => Some("midday"),
            18..=19 => Some("evening"),
            _ => None,
        };

        phase.map(|p| p.to_string())
    }

    /// Check context for changes and analyze if significant.
    /// Uses a dual trigger model: heartbeat (time-of-day phases) + event-driven (context changes).
    async fn check_and_analyze(&self) -> JasperResult<()> {
        debug!("Checking context for significant changes");

        // Collect current context from all sources
        let current_context = self.collect_current_context().await?;

        // Determine trigger: context change or heartbeat
        let (is_significant, changes) = self
            .significance_engine
            .analyze_context(current_context.clone());

        let trigger = if is_significant {
            info!("Significant changes detected: {:?}", changes);
            Some(InsightTrigger::ContextChange(changes))
        } else if let Some(phase) = self.should_fire_heartbeat() {
            // Check if we already fired a heartbeat this phase by looking at recent insights
            let dominated_by_recent = self
                .database
                .get_recent_insights(1)
                .unwrap_or_default()
                .first()
                .map(|i| {
                    let age_minutes = (Utc::now() - i.created_at).num_minutes();
                    // Don't fire heartbeat if we generated an insight within the last 90 minutes
                    age_minutes < 90
                })
                .unwrap_or(false);

            if dominated_by_recent {
                debug!("Skipping heartbeat — recent insight is still fresh");
                None
            } else {
                info!("Heartbeat trigger: {} phase", phase);
                // Record this as an AI call in the significance engine so it respects the cooldown
                self.significance_engine.record_ai_call();
                Some(InsightTrigger::Heartbeat(phase))
            }
        } else {
            None
        };

        if let Some(trigger) = trigger {
            // Call AI for analysis with full context and trigger info
            match self.analyze_with_ai(&current_context, &trigger).await {
                Ok(insight) => {
                    // Store the insight
                    match self.database.store_insight(
                        &insight.emoji,
                        &insight.text,
                        Some(&insight.context_hash),
                    ) {
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
                            self.emit_insight_signal(insight_id, &insight.emoji, &insight.text)
                                .await;
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
            debug!("No trigger fired — skipping AI call");
        }

        Ok(())
    }

    /// Collect current context from all sources
    async fn collect_current_context(&self) -> JasperResult<ContextSnapshotSummary> {
        let now = Utc::now();
        let end_time = now + chrono::Duration::hours(24);

        // Get calendar events for next 24 hours from database
        let calendar_events: Vec<_> = self
            .database
            .get_events_in_range(now, end_time)?
            .into_iter()
            .map(|event| crate::significance_engine::CalendarEventSummary {
                id: event.source_id,
                title: event.title.unwrap_or_default(),
                start_time: DateTime::from_timestamp(event.start_time, 0).unwrap_or_default(),
                end_time: event
                    .end_time
                    .map(|ts| DateTime::from_timestamp(ts, 0).unwrap_or_default()),
                location: event.location,
                is_all_day: event.is_all_day.unwrap_or(false),
            })
            .collect();

        // Collect additional context from all enabled context sources
        let context_data = match self
            .context_manager
            .read()
            .await
            .fetch_all_context(now, end_time)
            .await
        {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to fetch context from sources: {}", e);
                Vec::new()
            }
        };

        // Extract weather, tasks, and full notes context from context data
        let mut weather: Option<crate::significance_engine::WeatherSummary> = None;
        let mut tasks: Vec<crate::significance_engine::TaskSummary> = Vec::new();
        let mut notes_context: Option<context_sources::NotesContext> = None;
        let mut weather_context: Option<context_sources::WeatherContext> = None;

        for ctx in &context_data {
            match &ctx.content {
                context_sources::ContextContent::Weather(weather_ctx) => {
                    weather_context = Some(weather_ctx.clone());
                    // Parse temperature from forecast or conditions
                    if let Some(forecast) = weather_ctx.forecast.first() {
                        weather = Some(crate::significance_engine::WeatherSummary {
                            condition: forecast.conditions.clone(),
                            temperature: forecast.temperature_high as i32,
                            feels_like: forecast.temperature_high as i32,
                        });
                    }
                }
                context_sources::ContextContent::Tasks(task_ctx) => {
                    tasks.extend(task_ctx.tasks.iter().map(|t| {
                        crate::significance_engine::TaskSummary {
                            id: t.id.clone(),
                            title: t.title.clone(),
                            due: t.due_date,
                            completed: matches!(t.status, context_sources::TaskStatus::Completed),
                        }
                    }));
                }
                context_sources::ContextContent::Notes(notes_ctx) => {
                    notes_context = Some(notes_ctx.clone());
                    // Also extract tasks from notes for the significance engine
                    tasks.extend(notes_ctx.pending_tasks.iter().map(|t| {
                        crate::significance_engine::TaskSummary {
                            id: t.id.clone(),
                            title: t.title.clone(),
                            due: t.due_date,
                            completed: matches!(t.status, context_sources::TaskStatus::Completed),
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
            notes_context,
            weather_context,
            timestamp: now,
            context_hash,
        })
    }

    /// Call AI service for analysis, with automatic retry on transient failures
    async fn analyze_with_ai(
        &self,
        context: &ContextSnapshotSummary,
        trigger: &InsightTrigger,
    ) -> JasperResult<AiInsight> {
        debug!("Calling AI service for context analysis");

        // Build the request body once so retries reuse it
        let request_body = self.build_anthropic_request(context, trigger)?;

        match self
            .api_manager
            .execute_with_retry(|| {
                let body = request_body.clone();
                async move {
                    self.send_anthropic_request(&body)
                        .await
                        .map_err(|e| anyhow::anyhow!("{}", e))
                }
            })
            .await
        {
            Ok((insight, tokens_used)) => {
                self.api_manager.record_api_call(tokens_used);
                Ok(insight)
            }
            Err(e) => {
                // If rate-limited / circuit-broken, return fallback
                if e.to_string().contains("Daily API limit")
                    || e.to_string().contains("Circuit breaker")
                {
                    warn!("API call blocked: {}", e);
                    Ok(AiInsight {
                        emoji: "⏳".to_string(),
                        text: "Rate limited - check back later for fresh insights".to_string(),
                        context_hash: context.context_hash.clone(),
                    })
                } else {
                    Err(crate::errors::JasperError::Internal {
                        message: format!("AI analysis failed: {}", e),
                    })
                }
            }
        }
    }

    /// Determine the current time-of-day phase for the user's timezone
    fn get_time_of_day_phase(&self) -> (&'static str, DateTime<chrono::FixedOffset>) {
        let tz = self.config.read().get_timezone();
        let local_now = Utc::now().with_timezone(&tz);
        // Convert to FixedOffset for storage
        let fixed_now = local_now.fixed_offset();
        let hour = fixed_now.hour();
        let phase = match hour {
            5..=8 => "early morning — you're starting your day",
            9..=11 => "morning — your day is underway",
            12..=13 => "midday",
            14..=16 => "afternoon",
            17..=19 => "evening — winding down the day",
            20..=22 => "night — wrapping up",
            _ => "late night",
        };
        (phase, fixed_now)
    }

    /// Format a datetime as a human-relative time string (e.g., "in 45 minutes", "tomorrow at 3pm")
    fn format_relative_time(now: &DateTime<chrono::FixedOffset>, target: &DateTime<Utc>) -> String {
        let target_local = target.with_timezone(&now.timezone());
        let diff = target_local.signed_duration_since(*now);
        let minutes = diff.num_minutes();
        let hours = diff.num_hours();

        if minutes < 0 {
            let abs_min = minutes.abs();
            if abs_min < 60 {
                format!("{} minutes ago", abs_min)
            } else if abs_min < 1440 {
                format!("{} hours ago", (abs_min / 60))
            } else {
                format!("{} days ago", (abs_min / 1440))
            }
        } else if minutes < 60 {
            format!("in {} minutes", minutes)
        } else if hours < 24 {
            if hours == 1 {
                format!("in about an hour ({})", target_local.format("%I:%M %p"))
            } else {
                format!("in {} hours ({})", hours, target_local.format("%I:%M %p"))
            }
        } else {
            let days = hours / 24;
            if days == 1 {
                format!("tomorrow at {}", target_local.format("%I:%M %p"))
            } else {
                format!("in {} days ({})", days, target_local.format("%a %I:%M %p"))
            }
        }
    }

    /// Format a relative deadline for tasks (e.g., "due in 2 days", "OVERDUE by 3 days")
    fn format_relative_deadline(
        now: &DateTime<chrono::FixedOffset>,
        due: &DateTime<Utc>,
    ) -> String {
        let due_local = due.with_timezone(&now.timezone());
        let diff = due_local.signed_duration_since(*now);
        let hours = diff.num_hours();
        let days = diff.num_days();

        if hours < 0 {
            let abs_days = days.abs();
            if abs_days == 0 {
                "OVERDUE (due today)".to_string()
            } else if abs_days == 1 {
                "OVERDUE by 1 day".to_string()
            } else {
                format!("OVERDUE by {} days", abs_days)
            }
        } else if hours < 24 {
            "due today".to_string()
        } else if days == 1 {
            "due tomorrow".to_string()
        } else if days <= 7 {
            format!("due in {} days", days)
        } else {
            format!("due in {} weeks", days / 7)
        }
    }

    /// Build the Anthropic API request body from context (no I/O, can be reused for retries)
    fn build_anthropic_request(
        &self,
        context: &ContextSnapshotSummary,
        trigger: &InsightTrigger,
    ) -> JasperResult<serde_json::Value> {
        let (time_phase, local_now) = self.get_time_of_day_phase();

        // --- Build the system message with personality and guidance ---
        let (personality, _timezone_str) = {
            let cfg = self.config.read();
            let (p, tz) = cfg.get_personality_config();
            (p.clone(), tz.to_string())
        };

        let persona_desc = personality
            .persona_reference
            .as_deref()
            .map(|r| format!(" ({})", r))
            .unwrap_or_default();

        // Get recent insights for deduplication
        let recent_insights = self.database.get_recent_insights(5).unwrap_or_default();
        let recent_insights_text = if recent_insights.is_empty() {
            "None yet — this is your first insight of the session.".to_string()
        } else {
            recent_insights
                .iter()
                .map(|i| {
                    format!(
                        "- {} {} ({})",
                        i.emoji,
                        i.insight,
                        Self::format_relative_time(&local_now, &i.created_at)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        let system_message = format!(
            "You are Jasper, a {persona}{persona_ref}. \
You provide a single glanceable insight for {title}'s status bar — like Android's At a Glance widget, but smarter.\n\n\
Current time: {now} ({phase}).\n\n\
Your job: Surface the ONE most useful thing {title} needs to know or be reminded of right now. \
Think about what a thoughtful person who knows their whole life would tap them on the shoulder about.\n\n\
Prioritize (in rough order):\n\
1. Things that need action or preparation in the next 1-2 hours\n\
2. Tasks or deadlines that are creeping up and easy to forget (the kind assigned weeks ago that slip through the cracks)\n\
3. Scheduling conflicts or logistical issues they haven't noticed\n\
4. Cross-domain connections (a task relates to an upcoming event, weather affects plans)\n\n\
Do NOT:\n\
- Simply restate a calendar entry (\"You have a meeting at 3pm\") — add value beyond what a calendar shows\n\
- Focus on weather unless it meaningfully impacts plans or activities\n\
- Repeat something you've already surfaced recently (see recent insights below)\n\
- Be robotic or generic — write like someone who knows {title} personally, with warmth\n\
- Use any name or title other than \"{title}\" when addressing the user — always call them \"{title}\", never \"Sir\", \"Ma'am\", or any other title\n\
- NEVER invent, fabricate, or assume events, tasks, or appointments that are not listed in the context below — if the schedule is empty, it's empty\n\n\
Tone: {formality}. Keep it to ONE concise sentence. Warm and familiar, not stiff.\n\n\
Recent insights (DO NOT repeat these):\n{recent_insights}",
            persona = personality.assistant_persona,
            persona_ref = persona_desc,
            title = personality.user_title,
            now = local_now.format("%A, %B %-d at %-I:%M %p"),
            phase = time_phase,
            formality = personality.formality,
            recent_insights = recent_insights_text,
        );

        // --- Build the context (user message) with full data and relative times ---
        let mut context_parts: Vec<String> = Vec::new();

        // Trigger reason
        let trigger_text = match trigger {
            InsightTrigger::Heartbeat(phase) => format!("Trigger: Regular {} check-in.", phase),
            InsightTrigger::ContextChange(changes) => {
                let change_descriptions: Vec<String> = changes
                    .iter()
                    .map(|c| match c {
                        SignificantChange::NewCalendarEvent(title) => {
                            format!("New event added: \"{}\"", title)
                        }
                        SignificantChange::CancelledCalendarEvent(title) => {
                            format!("Event cancelled: \"{}\"", title)
                        }
                        SignificantChange::EventTimeChanged {
                            event_id,
                            time_diff_hours,
                        } => format!("Event {} moved by {:.1} hours", event_id, time_diff_hours),
                        SignificantChange::EventLocationChanged { event_id } => {
                            format!("Event {} location changed", event_id)
                        }
                        SignificantChange::WeatherConditionChanged { from, to } => {
                            format!("Weather changed: {} → {}", from, to)
                        }
                        SignificantChange::WeatherTemperatureChanged { diff } => {
                            format!("Temperature shifted by {}°F", diff)
                        }
                        SignificantChange::NewTask(title) => format!("New task: \"{}\"", title),
                        SignificantChange::TaskCompleted(title) => {
                            format!("Task completed: \"{}\"", title)
                        }
                        SignificantChange::TaskDueChanged {
                            task_id,
                            time_diff_hours,
                        } => format!(
                            "Task {} due date moved by {:.1} hours",
                            task_id, time_diff_hours
                        ),
                        SignificantChange::InitialContext => {
                            "Initial startup — first look at the day.".to_string()
                        }
                    })
                    .collect();
                format!(
                    "Trigger: Context changed — {}",
                    change_descriptions.join("; ")
                )
            }
        };
        context_parts.push(trigger_text);

        // Detect when there is no real data at all
        let has_calendar = !context.calendar_events.is_empty();
        let has_tasks = !context.tasks.is_empty();
        let has_weather = context.weather.is_some() || context.weather_context.is_some();
        let has_notes = context.notes_context.is_some();

        if !has_calendar && !has_tasks && !has_weather && !has_notes {
            context_parts.push(
                "\nNo calendar events, tasks, weather, or notes are available. \
                The schedule is completely clear. Do NOT invent or assume any events — \
                provide a genuine observation about having a clear schedule."
                    .to_string(),
            );
        }

        // Calendar events with relative times
        if !context.calendar_events.is_empty() {
            let mut cal_section = String::from("\nCalendar (next 24h):");
            for event in &context.calendar_events {
                let timing = if event.is_all_day {
                    "all day".to_string()
                } else {
                    Self::format_relative_time(&local_now, &event.start_time)
                };
                let location = event
                    .location
                    .as_ref()
                    .map(|l| format!(", at {}", l))
                    .unwrap_or_default();
                cal_section.push_str(&format!("\n- \"{}\" — {}{}", event.title, timing, location));
            }
            context_parts.push(cal_section);
        }

        // Tasks with relative deadlines
        if !context.tasks.is_empty() {
            let mut task_section = String::from("\nTasks:");
            for task in &context.tasks {
                if task.completed {
                    continue;
                }
                let deadline = task
                    .due
                    .as_ref()
                    .map(|d| format!(" ({})", Self::format_relative_deadline(&local_now, d)))
                    .unwrap_or_else(|| " (no due date)".to_string());
                task_section.push_str(&format!("\n- {}{}", task.title, deadline));
            }
            context_parts.push(task_section);
        }

        // Full weather context (if available)
        if let Some(weather_ctx) = &context.weather_context {
            let mut weather_section = format!("\nWeather: {}", weather_ctx.current_conditions);
            if !weather_ctx.forecast.is_empty() {
                let today = &weather_ctx.forecast[0];
                weather_section.push_str(&format!(
                    " (High: {:.0}°F, Low: {:.0}°F, {}% chance of precipitation)",
                    today.temperature_high,
                    today.temperature_low,
                    (today.precipitation_chance * 100.0) as i32
                ));
            }
            if !weather_ctx.alerts.is_empty() {
                weather_section.push_str(&format!(
                    "\nWeather alerts: {}",
                    weather_ctx.alerts.join(", ")
                ));
            }
            context_parts.push(weather_section);
        } else if let Some(weather) = &context.weather {
            context_parts.push(format!(
                "\nWeather: {} ({}°F)",
                weather.condition, weather.temperature
            ));
        }

        // Notes context: projects, relationships, focus areas
        if let Some(notes) = &context.notes_context {
            // Active projects with deadlines
            let active_projects: Vec<_> = notes
                .active_projects
                .iter()
                .filter(|p| {
                    matches!(
                        p.status,
                        context_sources::ProjectStatus::Active
                            | context_sources::ProjectStatus::Pending
                    )
                })
                .collect();
            if !active_projects.is_empty() {
                let mut proj_section = String::from("\nActive Projects:");
                for project in &active_projects {
                    let deadline = project
                        .due_date
                        .as_ref()
                        .map(|d| format!(" ({})", Self::format_relative_deadline(&local_now, d)))
                        .unwrap_or_default();
                    let progress = if project.progress > 0.0 {
                        format!(", {:.0}% complete", project.progress * 100.0)
                    } else {
                        String::new()
                    };
                    proj_section.push_str(&format!("\n- {}{}{}", project.name, deadline, progress));
                }
                context_parts.push(proj_section);
            }

            // Today's focus areas from daily notes
            let focus_areas: Vec<_> = notes
                .daily_notes
                .iter()
                .flat_map(|n| n.focus_areas.iter())
                .collect();
            if !focus_areas.is_empty() {
                let mut focus_section = String::from("\nToday's focus areas:");
                for area in &focus_areas {
                    focus_section.push_str(&format!("\n- {}", area));
                }
                context_parts.push(focus_section);
            }
        }

        let user_message = context_parts.join("\n");

        let model = self.config.read().ai.model.clone();

        Ok(serde_json::json!({
            "model": model,
            "max_tokens": 300,
            "system": system_message,
            "messages": [{
                "role": "user",
                "content": user_message
            }],
            "_context_hash": context.context_hash
        }))
    }

    /// Send the pre-built request body to the Anthropic API. Returns insight and tokens used.
    async fn send_anthropic_request(
        &self,
        request_body: &serde_json::Value,
    ) -> JasperResult<(AiInsight, u64)> {
        let api_key = self.config.read().get_api_key()
            .ok_or_else(|| crate::errors::JasperError::Authentication { service: "anthropic".into(), message: "API key not configured. Set via config, SOPS secrets, or ANTHROPIC_API_KEY environment variable.".into() })?;

        // Strip our internal field before sending
        let mut body = request_body.clone();
        let context_hash = body
            .get("_context_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        body.as_object_mut().map(|o| o.remove("_context_hash"));

        let response = self
            .http_client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| crate::errors::JasperError::Internal {
                message: format!("API request failed: {}", e),
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::errors::JasperError::Internal {
                message: format!("API call failed with status {}: {}", status, error_text),
            });
        }

        let response_json: serde_json::Value =
            response
                .json()
                .await
                .map_err(|e| crate::errors::JasperError::Internal {
                    message: format!("Failed to parse response: {}", e),
                })?;

        let content = response_json
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("text"))
            .and_then(|text| text.as_str())
            .ok_or_else(|| crate::errors::JasperError::Internal {
                message: "Invalid API response format".to_string(),
            })?;

        let tokens_used = response_json
            .get("usage")
            .map(|u| {
                let input = u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let output = u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                input + output
            })
            .unwrap_or(0);

        let (emoji, insight) = self.parse_ai_response(content);

        Ok((
            AiInsight {
                emoji,
                text: insight,
                context_hash,
            },
            tokens_used,
        ))
    }

    /// Parse AI response to extract emoji and insight.
    /// Supports both "Emoji:/Insight:" format and freeform "emoji text" format.
    fn parse_ai_response(&self, content: &str) -> (String, String) {
        let content = content.trim();
        let lines: Vec<&str> = content.lines().collect();
        let mut emoji = Option::<String>::None;
        let mut insight = Option::<String>::None;

        // Try structured format first
        for line in &lines {
            if line.starts_with("Emoji:") {
                let e = line.replace("Emoji:", "").trim().to_string();
                if !e.is_empty() {
                    emoji = Some(e);
                }
            } else if line.starts_with("Insight:") {
                let i = line.replace("Insight:", "").trim().to_string();
                if !i.is_empty() {
                    insight = Some(i);
                }
            }
        }

        // If structured parsing worked, return
        if let (Some(e), Some(i)) = (emoji.clone(), insight.clone()) {
            return (e, i);
        }

        // Freeform: expect the response to start with an emoji followed by text
        let mut chars = content.chars().peekable();
        let mut leading_emoji = String::new();

        // Collect leading emoji characters (may be multi-codepoint)
        while let Some(&ch) = chars.peek() {
            if ch.is_emoji_char() || ch == '\u{FE0F}' || ch == '\u{200D}' {
                leading_emoji.push(ch);
                chars.next();
            } else {
                break;
            }
        }

        let remaining: String = chars.collect();
        let remaining = remaining.trim().to_string();

        if !leading_emoji.is_empty() && !remaining.is_empty() {
            return (leading_emoji, remaining);
        }

        // Last resort: use full content as insight
        (
            emoji.unwrap_or_else(|| "🤖".to_string()),
            insight.unwrap_or_else(|| {
                if content.is_empty() {
                    "AI analysis complete - check your schedule and priorities".to_string()
                } else {
                    content.to_string()
                }
            }),
        )
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

/// What triggered the insight generation
#[derive(Debug, Clone)]
enum InsightTrigger {
    /// Regular time-of-day heartbeat (morning, afternoon, evening)
    Heartbeat(String),
    /// Detected context change from significance engine
    ContextChange(Vec<SignificantChange>),
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
