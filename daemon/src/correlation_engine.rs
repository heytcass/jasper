use anyhow::Result;
use std::sync::Arc;
use parking_lot::RwLock;
use chrono::{Utc, DateTime};
use chrono_tz;
use tracing::{info, debug, warn};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::database::{Database, Event, Correlation, EnrichedEvent};
use crate::data_sanitizer::{DataSanitizer, SanitizationLevel, SanitizedCalendarContext};
use crate::api_manager::ApiManager;
use crate::services::notification::{NotificationService, NotificationType, NotificationConfig};
use crate::context_sources::{
    ContextSourceManager, ContextData, ContextSource,
    obsidian::{ObsidianVaultSource, ObsidianConfig},
    calendar::CalendarContextSource,
    weather::WeatherContextSource,
    tasks::{TasksContextSource, TaskSourceType, TasksConfig},
};

#[derive(Debug, Serialize, Deserialize)]
struct CalendarAnalysisRequest {
    calendar_context: SanitizedCalendarContext,
    additional_context: Vec<ContextData>,
    user_preferences: UserPreferences,
    analysis_focus: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserPreferences {
    title: String,
    formality: String,
    timezone: String,
    working_hours: WorkingHours,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkingHours {
    start: String,
    end: String,
    days: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AiAnalysisResponse {
    insight: SingleInsight,
}

#[derive(Debug, Serialize, Deserialize)]
struct SingleInsight {
    message: String,
    icon: String,
    context_hash: String,
    generated_at: String, // ISO format timestamp
}

#[derive(Clone)]
pub struct CorrelationEngine {
    pub database: Database,
    config: Arc<RwLock<Config>>,
    sanitizer: DataSanitizer,
    http_client: reqwest::Client,
    api_manager: ApiManager,
    last_context_state: Arc<RwLock<Option<ContextState>>>,
    context_manager: Arc<RwLock<ContextSourceManager>>,
    notification_service: Arc<NotificationService>,
}

#[derive(Debug, Clone)]
struct ContextState {
    full_context_hash: String,
    context_source_states: std::collections::HashMap<String, String>,
    last_insight: Option<String>,
    last_generated: DateTime<Utc>,
    events_completed_since_last: u32,
    significant_changes: Vec<String>,
    last_significant_change_type: Vec<String>,
}

impl CorrelationEngine {
    pub fn new(database: Database, config: Arc<RwLock<Config>>) -> Self {
        // Default to moderate sanitization for now
        // TODO: Add privacy config section to Config
        let sanitization_level = SanitizationLevel::Moderate;
        
        // Initialize context source manager
        let context_manager = Self::initialize_context_sources(database.clone(), config.clone());
        
        // Initialize notification service with config from main config
        let notification_config = config.read().get_notification_config()
            .cloned()
            .map(|cfg| NotificationConfig {
                enabled: cfg.enabled,
                notify_new_insights: cfg.notify_new_insights,
                notify_context_changes: cfg.notify_context_changes,
                notify_cache_refresh: cfg.notify_cache_refresh,
                notification_timeout: cfg.notification_timeout,
                min_urgency_threshold: cfg.min_urgency_threshold,
            })
            .unwrap_or_else(|| NotificationConfig::default());
        let notification_service = Arc::new(NotificationService::new(notification_config));

        Self { 
            database, 
            config,
            sanitizer: DataSanitizer::new(sanitization_level),
            http_client: reqwest::Client::new(),
            api_manager: ApiManager::new(),
            last_context_state: Arc::new(RwLock::new(None)),
            context_manager: Arc::new(RwLock::new(context_manager)),
            notification_service,
        }
    }
    
    /// Initialize context sources based on configuration
    fn initialize_context_sources(database: Database, config: Arc<RwLock<Config>>) -> ContextSourceManager {
        let mut manager = ContextSourceManager::new();
        
        // Always add calendar context source
        manager.add_source(Box::new(CalendarContextSource::new(config.clone(), database.clone())));
        
        // Add Obsidian source if configured
        if let Some(obsidian_config) = config.read().get_obsidian_config() {
            if obsidian_config.enabled {
                match ObsidianVaultSource::new(ObsidianConfig {
                    vault_path: obsidian_config.vault_path.clone(),
                    daily_notes_folder: obsidian_config.daily_notes_folder.clone(),
                    daily_notes_format: obsidian_config.daily_notes_format.clone(),
                    templates_folder: obsidian_config.templates_folder.clone(),
                    people_folder: obsidian_config.people_folder.clone(),
                    projects_folder: obsidian_config.projects_folder.clone(),
                    parse_dataview: obsidian_config.parse_dataview,
                    parse_tasks: obsidian_config.parse_tasks,
                    parse_frontmatter: obsidian_config.parse_frontmatter,
                    relationship_alert_days: obsidian_config.relationship_alert_days,
                    ignored_folders: obsidian_config.ignored_folders.clone(),
                    ignored_files: obsidian_config.ignored_files.clone(),
                }) {
                    Ok(source) => {
                        info!("Obsidian context source initialized");
                        manager.add_source(Box::new(source));
                    }
                    Err(e) => {
                        warn!("Failed to initialize Obsidian context source: {}", e);
                    }
                }
            }
        }
        
        // Add weather source if configured
        if let Some(weather_config) = config.read().get_weather_config() {
            if weather_config.enabled && !weather_config.api_key.is_empty() {
                let weather_source = WeatherContextSource::with_config(
                    Some(weather_config.api_key.clone()),
                    weather_config.location.clone(),
                    weather_config.units.clone(),
                    weather_config.cache_duration_minutes,
                );
                manager.add_source(Box::new(weather_source));
                info!("Weather context source initialized");
            }
        }
        
        // Add tasks source if configured
        if let Some(tasks_config) = config.read().get_tasks_config() {
            if tasks_config.enabled {
                let source_type = match tasks_config.source_type.as_str() {
                    "todoist" => TaskSourceType::Todoist,
                    "local_file" => TaskSourceType::LocalFile,
                    "obsidian" => TaskSourceType::Obsidian,
                    _ => TaskSourceType::Todoist,
                };
                
                let tasks_source = TasksContextSource::new(
                    source_type,
                    TasksConfig {
                        api_key: tasks_config.api_key.clone(),
                        file_path: tasks_config.file_path.clone(),
                        sync_completed: tasks_config.sync_completed,
                        max_tasks: tasks_config.max_tasks,
                    },
                );
                manager.add_source(Box::new(tasks_source));
                info!("Tasks context source initialized");
            }
        }
        
        manager
    }

    /// Fetch additional context from all sources
    async fn fetch_additional_context(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<ContextData> {
        let mut context_data = Vec::new();
        
        // Check if Obsidian is enabled and fetch context
        if let Some(obsidian_context) = self.fetch_obsidian_context_simple(start, end).await {
            context_data.push(obsidian_context);
        }
        
        // Check if Weather is enabled and fetch context
        if let Some(weather_context) = self.fetch_weather_context_simple(start, end).await {
            context_data.push(weather_context);
        }
        
        // Check if Tasks is enabled and fetch context
        if let Some(tasks_context) = self.fetch_tasks_context_simple(start, end).await {
            context_data.push(tasks_context);
        }
        
        // Also check for Obsidian tasks (if different from main task source)
        if let Some(obsidian_tasks_context) = self.fetch_obsidian_tasks_context_simple(start, end).await {
            context_data.push(obsidian_tasks_context);
        }
        
        // Add other context sources here as they're implemented
        // TODO: Add tasks, etc.
        
        info!("Fetched {} context sources", context_data.len());
        context_data
    }
    
    /// Simple Obsidian context fetch without complex async management
    async fn fetch_obsidian_context_simple(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Option<ContextData> {
        // Get config synchronously
        let vault_path = {
            let config = self.config.read();
            if let Some(obsidian_config) = config.get_obsidian_config() {
                if obsidian_config.enabled {
                    obsidian_config.vault_path.clone()
                } else {
                    debug!("Obsidian context source not enabled");
                    return None;
                }
            } else {
                debug!("Obsidian config not found");
                return None;
            }
        };
        
        // Simple direct implementation without complex trait system
        match self.fetch_obsidian_data_direct(&vault_path, start, end).await {
            Ok(context) => {
                info!("Successfully fetched Obsidian context from {}", vault_path);
                Some(context)
            }
            Err(e) => {
                warn!("Failed to fetch Obsidian context: {}", e);
                None
            }
        }
    }
    
    /// Simple Weather context fetch without complex async management
    async fn fetch_weather_context_simple(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Option<ContextData> {
        // Get config synchronously
        let (api_key, location, units, cache_duration) = {
            let config = self.config.read();
            if let Some(weather_config) = config.get_weather_config() {
                if weather_config.enabled {
                    (
                        Some(weather_config.api_key.clone()),
                        weather_config.location.clone(),
                        weather_config.units.clone(),
                        weather_config.cache_duration_minutes
                    )
                } else {
                    debug!("Weather config disabled");
                    return None;
                }
            } else {
                debug!("Weather config not found");
                return None;
            }
        };
        
        // Create weather source and fetch data
        let weather_source = WeatherContextSource::with_config(
            api_key,
            location,
            units,
            cache_duration,
        );
        
        if !weather_source.is_enabled() {
            debug!("Weather source not enabled");
            return None;
        }
        
        match weather_source.fetch_context(start, end).await {
            Ok(context) => {
                info!("Successfully fetched weather context for {}", weather_source.display_name());
                Some(context)
            }
            Err(e) => {
                warn!("Failed to fetch weather context: {}", e);
                None
            }
        }
    }
    
    /// Simple Tasks context fetch without complex async management
    async fn fetch_tasks_context_simple(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Option<ContextData> {
        // Get config synchronously
        let (source_type, api_key, file_path, sync_completed, max_tasks) = {
            let config = self.config.read();
            if let Some(tasks_config) = config.get_tasks_config() {
                if tasks_config.enabled {
                    let source_type = match tasks_config.source_type.as_str() {
                        "todoist" => TaskSourceType::Todoist,
                        "local_file" => TaskSourceType::LocalFile,
                        "obsidian" => TaskSourceType::Obsidian,
                        _ => TaskSourceType::LocalFile,
                    };
                    
                    (
                        source_type,
                        tasks_config.api_key.clone(),
                        tasks_config.file_path.clone(),
                        tasks_config.sync_completed,
                        tasks_config.max_tasks
                    )
                } else {
                    debug!("Tasks config disabled");
                    return None;
                }
            } else {
                debug!("Tasks config not found");
                return None;
            }
        };
        
        // Create tasks source and fetch data
        let tasks_config = TasksConfig {
            api_key,
            file_path,
            sync_completed,
            max_tasks,
        };
        
        let tasks_source = TasksContextSource::new(source_type, tasks_config);
        
        if !tasks_source.is_enabled() {
            debug!("Tasks source not enabled");
            return None;
        }
        
        match tasks_source.fetch_context(start, end).await {
            Ok(context) => {
                info!("Successfully fetched tasks context for {}", tasks_source.display_name());
                Some(context)
            }
            Err(e) => {
                warn!("Failed to fetch tasks context: {}", e);
                None
            }
        }
    }
    
    /// Fetch Obsidian tasks specifically (separate from main task source)
    async fn fetch_obsidian_tasks_context_simple(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Option<ContextData> {
        // Only fetch if main task source is NOT already using Obsidian
        let main_task_source = {
            let config = self.config.read();
            if let Some(tasks_config) = config.get_tasks_config() {
                if tasks_config.enabled {
                    Some(tasks_config.source_type.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };
        
        // Skip if main task source is already Obsidian
        if let Some(ref source_type) = main_task_source {
            if source_type == "obsidian" {
                return None;
            }
        }
        
        // Get Obsidian config
        let vault_path = {
            let config = self.config.read();
            if let Some(obsidian_config) = config.get_obsidian_config() {
                if obsidian_config.enabled && obsidian_config.parse_tasks {
                    obsidian_config.vault_path.clone()
                } else {
                    return None;
                }
            } else {
                return None;
            }
        };
        
        // Create an Obsidian task source
        let tasks_config = TasksConfig {
            api_key: None,
            file_path: Some(vault_path),
            sync_completed: false,
            max_tasks: 25, // Limit for supplementary tasks
        };
        
        let obsidian_tasks_source = TasksContextSource::new(TaskSourceType::Obsidian, tasks_config);
        
        if !obsidian_tasks_source.is_enabled() {
            return None;
        }
        
        match obsidian_tasks_source.fetch_context(start, end).await {
            Ok(mut context) => {
                // Modify the source ID to differentiate from main task source
                context.source_id = "obsidian_tasks".to_string();
                info!("Successfully fetched Obsidian tasks context (supplementary)");
                Some(context)
            }
            Err(e) => {
                warn!("Failed to fetch Obsidian tasks context: {}", e);
                None
            }
        }
    }
    
    /// Direct Obsidian data fetch without complex abstractions
    async fn fetch_obsidian_data_direct(&self, vault_path: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<ContextData> {
        use std::path::Path;
        use tokio::fs;
        
        let vault_path = Path::new(vault_path);
        if !vault_path.exists() {
            return Err(anyhow::anyhow!("Vault path does not exist: {}", vault_path.display()));
        }
        
        // Simple implementation: just read some daily notes and create context
        let people_folder = vault_path.join("Work/People");
        let mut relationship_alerts = Vec::new();
        
        if people_folder.exists() {
            let mut entries = fs::read_dir(&people_folder).await?;
            while let Some(entry) = entries.next_entry().await? {
                if entry.path().extension().map_or(false, |ext| ext == "md") {
                    if let Ok(content) = fs::read_to_string(entry.path()).await {
                        // Simple frontmatter parsing for last_contact
                        if let Some(last_contact) = self.extract_last_contact(&content) {
                            let days_ago = (Utc::now() - last_contact).num_days();
                            if days_ago > 21 {
                                relationship_alerts.push(format!("Haven't contacted {} in {} days", 
                                    entry.file_name().to_string_lossy().replace(".md", ""), days_ago));
                            }
                        }
                    }
                }
            }
        }
        
        // Create simple context data
        let context = ContextData {
            source_id: "obsidian".to_string(),
            timestamp: Utc::now(),
            data_type: crate::context_sources::ContextDataType::Notes,
            priority: 200,
            content: crate::context_sources::ContextContent::Generic(
                crate::context_sources::GenericContext {
                    data: {
                        let mut data = std::collections::HashMap::new();
                        data.insert("relationship_alerts".to_string(), 
                            serde_json::to_value(&relationship_alerts).unwrap_or_default());
                        data.insert("source".to_string(), 
                            serde_json::Value::String("obsidian_vault".to_string()));
                        data
                    },
                    summary: format!("Obsidian vault analysis: {} relationship alerts", relationship_alerts.len()),
                    insights: relationship_alerts,
                }
            ),
            metadata: {
                let mut metadata = std::collections::HashMap::new();
                metadata.insert("vault_path".to_string(), vault_path.to_string_lossy().to_string());
                metadata
            },
        };
        
        Ok(context)
    }
    
    /// Extract last_contact date from frontmatter
    fn extract_last_contact(&self, content: &str) -> Option<DateTime<Utc>> {
        use regex::Regex;
        let re = Regex::new(r"last_contact:\s*(\d{4}-\d{2}-\d{2})").ok()?;
        let caps = re.captures(content)?;
        let date_str = caps.get(1)?.as_str();
        chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?
            .and_hms_opt(12, 0, 0)?.and_utc().into()
    }
    

    pub async fn analyze(&self) -> Result<Vec<Correlation>> {
        let planning_horizon = self.config.read().get_planning_horizon();

        let now = Utc::now();
        let future_window = now + planning_horizon;

        debug!("Analyzing calendar with AI from {} to {}", now, future_window);

        // Get all events in the planning horizon
        let events = self.database.get_events_in_range(now, future_window)?;
        
        if events.is_empty() {
            debug!("No events found in planning horizon");
            return Ok(Vec::new());
        }

        info!("Found {} events to analyze with AI", events.len());

        // Enrich events with calendar information before sanitization
        let enriched_events = self.enrich_events_with_calendar_info(&events).await?;
        
        // Sanitize calendar data for AI analysis with config
        let sanitized_context = {
            let config = self.config.read();
            self.sanitizer.sanitize_enriched_events_with_config(&enriched_events, &config)?
        };
        
        // Fetch additional context from all sources
        let additional_context = self.fetch_additional_context(now, future_window).await;
        
        // Create a comprehensive hash of all context sources for caching
        let full_context_hash = self.api_manager.create_context_hash(&events, &additional_context);
        
        // Check if context has changed meaningfully
        if !self.should_generate_new_insight(&full_context_hash, &events, &additional_context) {
            // Return the last insight if context hasn't changed significantly
            if let Some(last_state) = self.last_context_state.read().as_ref() {
                if let Some(last_insight) = &last_state.last_insight {
                    debug!("Context hasn't changed meaningfully, returning last insight");
                    return Ok(vec![Correlation {
                        id: uuid::Uuid::new_v4().to_string(),
                        event_ids: events.iter().map(|e| e.id).collect(),
                        insight: last_insight.clone(),
                        action_needed: "Review this insight".to_string(),
                        urgency_score: 5,
                        discovered_at: last_state.last_generated,
                        recommended_glyph: Some("󰃭".to_string()), // Default calendar icon
                    }]);
                }
            }
        }
        
        // Check cache first for new insights
        if let Some(cached_insight) = self.api_manager.get_cached_insight(&full_context_hash) {
            info!("Using cached analysis for full context");
            self.update_context_state(&full_context_hash, &cached_insight, &events, &additional_context);
            
            // Send notification for cached insight if it's still relevant
            if let Err(e) = self.notification_service.notify(NotificationType::NewInsight {
                message: cached_insight.clone(),
                icon: Some("󰃭".to_string()),
            }).await {
                warn!("Failed to send cached insight notification: {}", e);
            }
            
            return Ok(vec![Correlation {
                id: uuid::Uuid::new_v4().to_string(),
                event_ids: events.iter().map(|e| e.id).collect(),
                insight: cached_insight,
                action_needed: "Review cached insights".to_string(),
                urgency_score: 5,
                discovered_at: Utc::now(),
                recommended_glyph: Some("󰃭".to_string()), // Default calendar icon
            }]);
        }
        
        // Check if we can make an API call
        if !self.api_manager.can_make_api_call() {
            warn!("Daily API limit reached. Using emergency fallback.");
            return self.emergency_fallback_analysis(&events);
        }
        
        // Prepare AI analysis request
        let analysis_request = self.prepare_analysis_request(sanitized_context, additional_context.clone())?;
        
        // Send to Claude Sonnet 4 for analysis
        match self.analyze_with_claude(&analysis_request).await {
            Ok(ai_response) => {
                info!("AI analysis completed successfully with single insight");
                
                // Record the API call
                self.api_manager.record_api_call(1500); // Estimate 1500 tokens
                
                // Cache the result
                self.api_manager.cache_insight(full_context_hash.clone(), ai_response.insight.message.clone());
                
                // Update context state with new insight
                self.update_context_state(&full_context_hash, &ai_response.insight.message, &events, &additional_context);
                
                // Send notification for new insight
                if let Err(e) = self.notification_service.notify(NotificationType::NewInsight {
                    message: ai_response.insight.message.clone(),
                    icon: Some(ai_response.insight.icon.clone()),
                }).await {
                    warn!("Failed to send new insight notification: {}", e);
                }
                
                debug!("About to convert AI insight to correlation");
                let result = self.convert_single_insight_to_correlation(ai_response.insight, &events);
                debug!("Conversion completed with result: {:?}", result.is_ok());
                result
            }
            Err(e) => {
                warn!("AI analysis failed: {}. Using emergency fallback.", e);
                self.emergency_fallback_analysis(&events)
            }
        }
    }

    fn prepare_analysis_request(&self, sanitized_context: SanitizedCalendarContext, additional_context: Vec<ContextData>) -> Result<CalendarAnalysisRequest> {
        let (user_title, formality, timezone) = self.config.read().get_personality_info();

        let user_preferences = UserPreferences {
            title: user_title,
            formality,
            timezone,
            working_hours: WorkingHours {
                start: "09:00".to_string(),
                end: "17:00".to_string(),
                days: vec!["Monday".to_string(), "Tuesday".to_string(), "Wednesday".to_string(), 
                         "Thursday".to_string(), "Friday".to_string()],
            },
        };

        Ok(CalendarAnalysisRequest {
            calendar_context: sanitized_context,
            additional_context,
            user_preferences,
            analysis_focus: vec![
                "scheduling_conflicts".to_string(),
                "preparation_time".to_string(),
                "overcommitment".to_string(),
                "travel_logistics".to_string(),
                "wellness_balance".to_string(),
                "opportunities".to_string(),
                "relationship_maintenance".to_string(),
                "project_deadlines".to_string(),
                "personal_development".to_string(),
                "context_awareness".to_string(),
            ],
        })
    }

    async fn analyze_with_claude(&self, request: &CalendarAnalysisRequest) -> Result<AiAnalysisResponse> {
        // Read config values using accessor methods (extract before async operations)
        let (api_key, model, max_tokens, temperature) = {
            let config = self.config.read();
            let api_key = config.get_api_key()
                .ok_or_else(|| anyhow::anyhow!("API key not found. Set either config.ai.api_key or ANTHROPIC_API_KEY environment variable."))?;
            let (_, model, max_tokens, temperature) = config.get_api_config();
            (api_key, model, max_tokens, temperature)
        };

        let prompt = self.create_claude_analysis_prompt(request);
        debug!("Prompt length: {} characters", prompt.len());
        debug!("Full prompt being sent to AI:\n{}", prompt);

        let request_body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "temperature": temperature,
            "messages": [{
                "role": "user",
                "content": prompt
            }]
        });
        
        debug!("Making Claude API request...");
        let response = self.http_client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &api_key)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Claude API error: {}", error_text));
        }

        let json: serde_json::Value = response.json().await?;
        let content = json["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid Claude response format"))?;

        self.parse_claude_response(content)
    }

    fn create_claude_analysis_prompt(&self, request: &CalendarAnalysisRequest) -> String {
        let ctx = &request.calendar_context;
        let prefs = &request.user_preferences;
        let additional_ctx = &request.additional_context;
        
        let events_json = serde_json::to_string_pretty(&ctx.events).unwrap_or_default();
        let additional_context_json = serde_json::to_string_pretty(additional_ctx).unwrap_or_default();
        
        // Get current time in both UTC and local timezone
        let now_utc = chrono::Utc::now();
        let timezone: chrono_tz::Tz = prefs.timezone.parse().unwrap_or(chrono_tz::UTC);
        let now_local = now_utc.with_timezone(&timezone);
        
        let current_datetime = now_local.format("%Y-%m-%d %I:%M %p").to_string();
        let current_day = now_local.format("%A, %B %d, %Y").to_string();
        
        let personality_guidance = match prefs.formality.as_str() {
            "formal" => "You are a thoughtful personal assistant who notices important details and provides direct, helpful guidance.",
            "casual" => "You are a caring, observant friend who notices important details and speaks naturally.",
            _ => "You are a thoughtful personal assistant who notices important details and provides direct, helpful guidance.",
        };

        format!(r#"You are Jasper, a personal digital companion. {personality_guidance}

Analyze this calendar and identify THE most important insight that needs attention right now. Look for meaningful patterns, conflicts, and relationships between events.

CURRENT CONTEXT:
- Current date and time: {} ({})
- Today is: {}

USER PREFERENCES:
- Address as: "{}"
- Communication style: {}
- Timezone: {}
- Working hours: {} to {} on weekdays

CALENDAR CONTEXT:
Time range: {}
Total events: {}
Pattern summary: {}

IMPORTANT: All event times below are in UTC format (ending with Z). When referencing times in your insights, convert them to the user's timezone ({}) and use 12-hour format with AM/PM. For example: "2025-07-16T15:00:00Z" should be referenced as "11:00 AM" in your response.

**ALL-DAY EVENT HANDLING**: Some events have "is_all_day": true. These are all-day events that span entire days. Do NOT treat all-day events as happening at specific times like "tonight" or "this evening". Instead, refer to them by their date (e.g., "Sunday's task" or "this Sunday").

CALENDAR EVENTS:
{}

ADDITIONAL CONTEXT:
The following additional context sources provide deeper insights into your life and work:
{}

**CONTEXT INTEGRATION GUIDELINES**:
- **Obsidian Notes**: Use daily notes, project status, and relationship alerts for comprehensive insights
- **Task Management**: Consider overdue tasks, upcoming deadlines, and project dependencies
- **Weather**: Factor in weather conditions for travel planning and outdoor events
- **Relationship Alerts**: Prioritize communication needs based on last contact dates
- **Project Status**: Use project progress and deadlines for workload assessment

**MANDATORY PRIORITY CHECK**: Before analyzing anything else, scan for:
1. **YOUR PERSONAL EVENTS** from calendars labeled "Me (Family Coordination)" or "Me (Personal Tasks)"
2. **EVENTS HAPPENING TOMORROW** (within 24 hours from current time)
3. **EVENTS HAPPENING TODAY** (if any remain)

**CRITICAL RULE**: If you find ANY events from "Me (Family Coordination)" or "Me (Personal Tasks)" calendars happening in the next 1-3 days, you MUST prioritize them over all other events, regardless of complexity of other situations.

**TEMPORAL PRIORITY HIERARCHY** (MANDATORY ORDER):
1. **YOUR events happening TODAY** (absolute top priority)
2. **YOUR events happening TOMORROW** (second priority)
3. **YOUR events happening THIS WEEK** (third priority)
4. **Other family events happening TODAY/TOMORROW** (fourth priority)
5. **Other family events happening THIS WEEK** (fifth priority)
6. **Future events beyond this week** (lowest priority)

**CALENDAR OWNER DETECTION** (MANDATORY STEP):
Before analyzing any events, you MUST check the "calendar_owner" field for EVERY event. This tells you WHO the event belongs to:
- "Me (Family Coordination)" = YOUR events (highest priority)
- "Me (Personal Tasks)" = YOUR events (highest priority)  
- "Wife (Coordination Needed)" = Your wife's events (mention as "Your wife...")
- "Son (Parent Logistics)" = Your son's events (mention as "Your son...")
- "Daughter (Parent Logistics)" = Your daughter's events (mention as "Your daughter...")

**EXAMPLES OF PROPER CALENDAR OWNER USAGE**:
- Event on "Wife (Coordination Needed)" calendar going to Boston → "Your wife is traveling to Boston"
- Event on "Me (Family Coordination)" calendar for haircut → "You have a haircut appointment"
- Event on "Son (Parent Logistics)" calendar for haircut → "Your son has a haircut appointment"

**TEMPORAL PRIORITY RULE**: Events happening TOMORROW (1 day away) are MORE IMPORTANT than events happening in 4+ days, regardless of complexity. A simple appointment tomorrow beats a complex situation later in the week.

**EXAMPLE SCENARIOS**:
- If YOU have a haircut Wednesday AND there's a family event Saturday → Focus on YOUR Wednesday haircut
- If YOU have a meeting tomorrow AND there's a complex weekend situation → Focus on YOUR tomorrow meeting
- If YOU have any appointment in the next 2 days AND family has events later → Focus on YOUR appointment

**CALENDAR OWNERSHIP RULES** (EXTREMELY IMPORTANT):
- **"Me (Family Coordination)" calendar** = YOUR events, say "You have..."
- **"Me (Personal Tasks)" calendar** = YOUR events, say "You have..."
- **"Wife (Coordination Needed)" calendar** = Your wife's events, say "Your wife has..." or "Your wife is going to..."
- **"Son (Parent Logistics)" calendar** = Your son's events, say "Your son has..."
- **"Daughter (Parent Logistics)" calendar** = Your daughter's events, say "Your daughter has..."
- **All other calendars** = Family/shared events, use appropriate language

**CROSS-CALENDAR INTELLIGENCE**: Pay attention to calendar ownership and locations:
- Events from different family members' calendars are for different people
- Same event type at same time/place but different calendars = different people doing the same thing
- Example: "Haircut at 3pm on Me calendar + Haircut at 3pm on Son calendar = Both you AND your son have haircuts"
- Don't assume conflicts unless it's the same person or affects the same resources
- **CRITICAL**: Always check the calendar owner field in each event and use appropriate pronouns

**TONE GUIDELINES:**
- Be direct and helpful, not formal
- Use natural, conversational language
- Provide actionable guidance
- Example: "You have two appointments scheduled close to each other, make sure you have a plan to be able to attend both."
- Example: "You won't be home from your trip before the cleaning crew comes. Make sure you straighten up before you leave."

**GLYPH SELECTION GUIDE:**
Choose Unicode characters that represent the content/topic. Examples:
- Racing/Formula 1: 󰄶 (race car)
- Travel/flights: ✈️ (airplane)  
- Meetings/work: 󰍥 (people/meeting)
- Weather events: 🌤️ (weather)
- Sports: ⚽ (sport-specific)
- Entertainment: 🎵 (music/entertainment)
- Technology: 💻 (computer/tech)
- Conflicts/warnings: ⚠️ (warning)
- Default: 󰃭 (calendar)

Respond in this JSON format:
{{
  "insight": {{
    "message": "Your direct, helpful insight about the most important thing to address",
    "icon": "📅",
    "context_hash": "placeholder",
    "generated_at": "2024-01-01T00:00:00Z"
  }}
}}

Focus on THE most important insight. Be specific to THIS calendar, not generic."#,
            current_datetime,
            prefs.timezone,
            current_day,
            prefs.title,
            prefs.formality,
            prefs.timezone,
            prefs.working_hours.start,
            prefs.working_hours.end,
            ctx.time_range,
            ctx.total_events,
            ctx.pattern_summary,
            prefs.timezone,
            events_json,
            additional_context_json
        )
    }

    fn parse_claude_response(&self, content: &str) -> Result<AiAnalysisResponse> {
        debug!("Raw Claude response: {}", content);
        
        // Extract JSON from Claude's response
        let json_start = content.find('{').unwrap_or(0);
        let json_end = content.rfind('}').map(|i| i + 1).unwrap_or(content.len());
        let json_str = &content[json_start..json_end];
        
        debug!("Extracted JSON: {}", json_str);

        match serde_json::from_str::<AiAnalysisResponse>(json_str) {
            Ok(response) => {
                debug!("Successfully parsed Claude analysis with single insight");
                Ok(response)
            }
            Err(e) => {
                warn!("Failed to parse Claude response as JSON: {}. Raw: {}", e, content);
                // Create a fallback response from the raw text
                Ok(AiAnalysisResponse {
                    insight: SingleInsight {
                        message: content.trim().to_string(),
                        icon: "󰃭".to_string(), // Default calendar icon
                        context_hash: "fallback".to_string(),
                        generated_at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                    },
                })
            }
        }
    }

    fn convert_single_insight_to_correlation(&self, insight: SingleInsight, _events: &[Event]) -> Result<Vec<Correlation>> {
        debug!("Converting single AI insight to correlation");
        
        let correlation = Correlation {
            id: uuid::Uuid::new_v4().to_string(),
            event_ids: vec![], // Simplified for now
            insight: insight.message,
            action_needed: "Review this insight".to_string(),
            urgency_score: 5, // Default medium urgency since we removed urgency scoring
            discovered_at: Utc::now(),
            recommended_glyph: Some(insight.icon),
        };
        
        debug!("Successfully converted to single correlation");
        Ok(vec![correlation])
    }

    fn emergency_fallback_analysis(&self, events: &[Event]) -> Result<Vec<Correlation>> {
        warn!("Using emergency fallback analysis - Claude API unavailable");
        
        // Provide basic conflict detection as emergency fallback
        let mut correlations = Vec::new();
        
        // Optimized O(n log n) overlap detection using sweep line algorithm
        correlations.extend(self.detect_overlaps_optimized(events));

        if correlations.is_empty() {
            info!("No conflicts detected in emergency fallback analysis");
        } else {
            info!("Emergency fallback found {} potential conflicts", correlations.len());
        }

        Ok(correlations)
    }

    fn events_overlap(&self, event1: &Event, event2: &Event) -> bool {
        let event1_start = event1.start_time;
        let event1_end = event1.end_time.unwrap_or(event1_start + 3600); // Default 1 hour

        let event2_start = event2.start_time;
        let event2_end = event2.end_time.unwrap_or(event2_start + 3600);

        // Check if events overlap
        event1_start < event2_end && event2_start < event1_end
    }

    /// Optimized O(n log n) overlap detection using sweep line algorithm
    fn detect_overlaps_optimized(&self, events: &[Event]) -> Vec<Correlation> {
        let mut correlations = Vec::new();
        
        if events.len() < 2 {
            return correlations;
        }

        // Create intervals with event references
        let mut intervals: Vec<(i64, i64, &Event)> = events.iter()
            .map(|event| {
                let start = event.start_time;
                let end = event.end_time.unwrap_or(start + 3600); // Default 1 hour
                (start, end, event)
            })
            .collect();

        // Sort by start time for sweep line algorithm
        intervals.sort_by_key(|&(start, _, _)| start);

        // Use sweep line to detect overlaps
        for i in 0..intervals.len() {
            let (start_i, end_i, event_i) = intervals[i];
            
            // Check against all events that could potentially overlap
            for j in (i + 1)..intervals.len() {
                let (start_j, end_j, event_j) = intervals[j];
                
                // If the next event starts after current event ends, no more overlaps possible
                if start_j >= end_i {
                    break;
                }
                
                // Check if they actually overlap
                if start_i < end_j && start_j < end_i {
                    correlations.push(Correlation {
                        id: uuid::Uuid::new_v4().to_string(),
                        event_ids: vec![event_i.id, event_j.id],
                        insight: format!("You have overlapping events: '{}' and '{}'. Check your schedule.", 
                            event_i.title.as_deref().unwrap_or("Event"),
                            event_j.title.as_deref().unwrap_or("Event")),
                        action_needed: "Please review these overlapping events".to_string(),
                        urgency_score: 5,
                        discovered_at: Utc::now(),
                        recommended_glyph: Some("⚠️".to_string()), // Warning icon
                    });
                }
            }
        }

        correlations
    }
    
    fn should_generate_new_insight(&self, full_context_hash: &str, events: &[Event], additional_context: &[crate::context_sources::ContextData]) -> bool {
        let last_state = self.last_context_state.read();
        
        // If no previous state, we should generate a new insight
        let Some(ref state) = *last_state else {
            debug!("No previous context state, generating new insight");
            return true;
        };
        
        // If full context hash changed, we should generate a new insight
        if state.full_context_hash != full_context_hash {
            debug!("Full context hash changed, checking if semantic similarity allows reuse");
            
            // Before generating a new insight, check if the previous one is still semantically relevant
            if let Some(ref last_insight) = state.last_insight {
                if self.is_context_semantically_similar(&state, additional_context) {
                    debug!("Context changed but semantically similar, keeping existing insight");
                    return false;
                }
            }
            
            debug!("Context changed meaningfully, generating new insight");
            return true;
        }
        
        // Check if significant time has passed (more than 4 hours)
        let now = Utc::now();
        if now.signed_duration_since(state.last_generated).num_hours() > 4 {
            debug!("Significant time passed (>4 hours), generating new insight");
            return true;
        }
        
        // Check if events have been completed (ended)
        let completed_events = self.count_completed_events(events);
        if completed_events > state.events_completed_since_last {
            debug!("Events completed since last insight, generating new insight");
            return true;
        }
        
        // Check for significant changes in event structure or context sources
        if self.has_significant_context_changes(events, additional_context, state) {
            debug!("Significant context changes detected, generating new insight");
            return true;
        }
        
        debug!("No significant context changes, keeping current insight");
        false
    }
    
    fn count_completed_events(&self, events: &[Event]) -> u32 {
        let now = Utc::now().timestamp();
        events.iter()
            .filter(|event| {
                let end_time = event.end_time.unwrap_or(event.start_time + 3600);
                end_time < now
            })
            .count() as u32
    }
    
    fn has_significant_context_changes(&self, events: &[Event], additional_context: &[crate::context_sources::ContextData], state: &ContextState) -> bool {
        // Check for significant calendar event changes
        let current_event_count = events.len();
        
        // Check if there are events starting soon (within next 2 hours)
        let now = Utc::now().timestamp();
        let soon_threshold = now + 2 * 3600; // 2 hours
        
        let upcoming_events = events.iter()
            .filter(|event| event.start_time > now && event.start_time <= soon_threshold)
            .count();
        
        // If there are upcoming events, we should consider regenerating
        if upcoming_events > 0 {
            debug!("Found {} upcoming events in next 2 hours", upcoming_events);
            return true;
        }
        
        // Check for significant changes in context sources
        for context in additional_context {
            let current_source_hash = format!("{:x}", md5::compute(format!("{:?}", context).as_bytes()));
            
            if let Some(previous_hash) = state.context_source_states.get(&context.source_id) {
                if previous_hash != &current_source_hash {
                    // Check if this is a meaningful change for this source type
                    if self.is_meaningful_context_change(&context, previous_hash, &current_source_hash) {
                        debug!("Meaningful change detected in context source: {}", context.source_id);
                        return true;
                    }
                }
            } else {
                // New context source appeared
                debug!("New context source detected: {}", context.source_id);
                return true;
            }
        }
        
        false
    }
    
    fn is_meaningful_context_change(&self, context: &crate::context_sources::ContextData, _previous_hash: &str, _current_hash: &str) -> bool {
        use crate::context_sources::ContextContent;
        
        // For now, consider all context changes meaningful
        // In the future, we could implement more sophisticated logic here
        match &context.content {
            ContextContent::Tasks(task_ctx) => {
                // Tasks changes are meaningful if there are overdue tasks or upcoming deadlines
                task_ctx.overdue_count > 0 || task_ctx.upcoming_count > 0
            },
            ContextContent::Notes(notes_ctx) => {
                // Notes changes are meaningful if there are relationship alerts or project changes
                !notes_ctx.relationship_alerts.is_empty() || !notes_ctx.active_projects.is_empty()
            },
            ContextContent::Weather(weather_ctx) => {
                // Weather changes are meaningful if there are alerts
                !weather_ctx.alerts.is_empty()
            },
            _ => true, // For other types, consider all changes meaningful for now
        }
    }
    
    fn is_context_semantically_similar(&self, previous_state: &ContextState, current_context: &[crate::context_sources::ContextData]) -> bool {
        use crate::context_sources::ContextContent;
        
        // Compare key metrics between previous and current context to determine if they're semantically similar
        // This helps prevent regenerating insights that would be essentially the same
        
        let mut similar_sources = 0;
        let mut total_sources = 0;
        
        for context in current_context {
            total_sources += 1;
            
            // Check if this context source existed before and has similar characteristics
            if previous_state.context_source_states.contains_key(&context.source_id) {
                match &context.content {
                    ContextContent::Calendar(cal_ctx) => {
                        // Calendar context is similar if event count and conflicts are similar
                        // (Minor changes like descriptions don't affect semantic meaning)
                        similar_sources += 1;
                    },
                    ContextContent::Tasks(task_ctx) => {
                        // Task context is similar if urgency counts haven't changed dramatically
                        if task_ctx.overdue_count <= 2 && task_ctx.upcoming_count <= 5 {
                            similar_sources += 1;
                        }
                    },
                    ContextContent::Notes(notes_ctx) => {
                        // Notes context is similar if no critical relationship alerts
                        let critical_alerts = notes_ctx.relationship_alerts.iter()
                            .filter(|alert| alert.urgency >= 8)
                            .count();
                        if critical_alerts == 0 {
                            similar_sources += 1;
                        }
                    },
                    ContextContent::Weather(weather_ctx) => {
                        // Weather context is similar if no severe alerts
                        if weather_ctx.alerts.is_empty() {
                            similar_sources += 1;
                        }
                    },
                    _ => {
                        // For other context types, assume similar for now
                        similar_sources += 1;
                    }
                }
            }
        }
        
        // Consider contexts semantically similar if 80% or more of sources are similar
        if total_sources == 0 {
            return true;
        }
        
        let similarity_ratio = similar_sources as f32 / total_sources as f32;
        let is_similar = similarity_ratio >= 0.8;
        
        debug!("Semantic similarity check: {}/{} sources similar ({}), threshold: 80%", 
               similar_sources, total_sources, 
               if is_similar { "SIMILAR" } else { "DIFFERENT" });
        
        is_similar
    }
    
    fn update_context_state(&self, full_context_hash: &str, insight: &str, events: &[Event], additional_context: &[crate::context_sources::ContextData]) {
        let mut state = self.last_context_state.write();
        let completed_events = self.count_completed_events(events);
        
        // Create individual source states for tracking
        let mut context_source_states = std::collections::HashMap::new();
        for context in additional_context {
            let source_hash = format!("{:x}", md5::compute(format!("{:?}", context).as_bytes()));
            context_source_states.insert(context.source_id.clone(), source_hash);
        }
        
        *state = Some(ContextState {
            full_context_hash: full_context_hash.to_string(),
            context_source_states,
            last_insight: Some(insight.to_string()),
            last_generated: Utc::now(),
            events_completed_since_last: completed_events,
            significant_changes: Vec::new(),
            last_significant_change_type: Vec::new(),
        });
        
        debug!("Updated context state with new insight from all sources");
    }

    pub fn clear_cache_and_context(&self) {
        self.api_manager.clear_cache();
        let mut state = self.last_context_state.write();
        *state = None;
        debug!("Cleared all cache and context state");
        
        // Send notification about cache refresh
        let notification_service = self.notification_service.clone();
        tokio::spawn(async move {
            if let Err(e) = notification_service.notify(NotificationType::CacheRefreshed).await {
                warn!("Failed to send cache refresh notification: {}", e);
            }
        });
    }
    
    /// Reinitialize context sources (useful for config changes)
    pub fn reinitialize_context_sources(&self) {
        let new_manager = Self::initialize_context_sources(self.database.clone(), self.config.clone());
        let mut context_manager = self.context_manager.write();
        *context_manager = new_manager;
        info!("Context sources reinitialized");
    }

    /// Get access to the notification service
    pub fn notification_service(&self) -> Arc<NotificationService> {
        self.notification_service.clone()
    }

    async fn enrich_events_with_calendar_info(&self, events: &[Event]) -> Result<Vec<EnrichedEvent>> {
        let mut enriched_events = Vec::new();
        
        for event in events {
            let calendar_info = self.database.get_calendar_info(event.calendar_id)?;
            enriched_events.push(EnrichedEvent {
                event: event.clone(),
                calendar_info,
            });
        }
        
        Ok(enriched_events)
    }
}