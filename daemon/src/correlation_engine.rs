use anyhow::Result;
use std::sync::Arc;
use std::collections::HashMap;
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
        
        // Auto-discover calendar owners if not already configured (runs once in a while)
        if self.config.read().calendar_owners.is_none() {
            if let Err(e) = self.auto_discover_calendar_owners().await {
                debug!("Auto-discovery failed, but continuing with analysis: {}", e);
            }
        }
        
        // Sanitize calendar data for AI analysis with config
        let sanitized_context = {
            let config = self.config.read();
            self.sanitizer.sanitize_enriched_events_with_config(&enriched_events, &config)?
        };
        
        // Fetch additional context from all sources
        let additional_context = self.fetch_additional_context(now, future_window).await;
        
        // Create a comprehensive hash of all context sources for caching
        
        // Check if context has changed meaningfully to avoid unnecessary API calls
        if !self.should_generate_new_insight("", &events, &additional_context) {
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
        
        
        // Check if we can make an API call
        if !self.api_manager.can_make_api_call() {
            warn!("Daily API limit reached. Using last cached insight.");
            if let Some(cached_insight) = self.api_manager.get_last_insight() {
                return Ok(vec![Correlation {
                    id: uuid::Uuid::new_v4().to_string(),
                    event_ids: events.iter().map(|e| e.id).collect(),
                    insight: cached_insight,
                    action_needed: "Review cached insights".to_string(),
                    urgency_score: 5,
                    discovered_at: Utc::now(),
                    recommended_glyph: Some("󰃭".to_string()), // Default calendar icon
                }]);
            } else {
                warn!("No cached insights available and API limit reached");
                return Ok(vec![]);
            }
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
                self.api_manager.cache_insight(ai_response.insight.message.clone());
                
                // Update context state with new insight
                self.update_context_state("", &ai_response.insight.message, &events, &additional_context);
                
                // Send notification for new insight
                if let Err(e) = self.notification_service.notify(NotificationType::NewInsight {
                    message: ai_response.insight.message.clone(),
                    icon: Some(ai_response.insight.icon.clone()),
                }).await {
                    warn!("Failed to send new insight notification: {}", e);
                }
                
                debug!("About to convert AI insight to correlation");
                let result = self.convert_single_insight_to_correlation(ai_response.insight, &events).await;
                debug!("Conversion completed with result: {:?}", result.is_ok());
                result
            }
            Err(e) => {
                warn!("AI analysis failed: {}. Using last cached insight.", e);
                if let Some(cached_insight) = self.api_manager.get_last_insight() {
                    Ok(vec![Correlation {
                        id: uuid::Uuid::new_v4().to_string(),
                        event_ids: events.iter().map(|e| e.id).collect(),
                        insight: cached_insight,
                        action_needed: "Review cached insights".to_string(),
                        urgency_score: 5,
                        discovered_at: Utc::now(),
                        recommended_glyph: Some("󰃭".to_string()), // Default calendar icon
                    }])
                } else {
                    warn!("No cached insights available and AI analysis failed");
                    Ok(vec![])
                }
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
                "immediate_needs".to_string(),
                "today_and_tomorrow".to_string(),
                "obvious_connections".to_string(),
                "actionable_awareness".to_string(),
                "personal_responsibilities".to_string(),
                "simple_reminders".to_string(),
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
        
        // Get personality configuration values from config
        let (personality_guidance, childcare_helper_term, persona_reference) = {
            let config_guard = self.config.read();
            let (personality_config, _) = config_guard.get_personality_config();
            
            let guidance = match prefs.formality.as_str() {
                "formal" => {
                    let persona_text = if let Some(ref persona) = personality_config.persona_reference {
                        format!("{} - personally invested, familiar with everyone, and proactively helpful.", persona)
                    } else {
                        "- personally invested, familiar with everyone, and proactively helpful.".to_string()
                    };
                    format!("You are a {} {}", personality_config.assistant_persona, persona_text)
                },
                "casual" => "You are a caring family companion who knows everyone personally and speaks naturally about their lives and needs.".to_string(),
                _ => {
                    let persona_text = if let Some(ref persona) = personality_config.persona_reference {
                        format!("{} - personally invested, familiar with everyone, and proactively helpful.", persona)
                    } else {
                        "- personally invested, familiar with everyone, and proactively helpful.".to_string()
                    };
                    format!("You are a {} {}", personality_config.assistant_persona, persona_text)
                },
            };
            
            (
                guidance,
                personality_config.childcare_helper_term.clone(),
                personality_config.persona_reference.as_deref().unwrap_or("").to_string()
            )
        };

        format!(r#"You are Jasper, a personal digital companion. {personality_guidance}

Analyze this calendar and identify THE most important insight that needs attention right now. ALWAYS CHECK WEATHER DATA FIRST - if there are weather alerts or conditions affecting events, prioritize those insights over logistics concerns. Look for meaningful patterns, conflicts, and relationships between events.

**IMPORTANT**: Keep your response short and punchy - 2-3 sentences maximum. Get straight to the point without being wordy.

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

**ALL-DAY EVENT HANDLING**: Events with "is_all_day": true are usually reminders, not time-consuming activities. Birthdays are reminders to send messages/calls, not parties to plan. "{}" type events are just tracking when childcare help is available. Don't treat these as events that need preparation or conflict with your schedule - they're background information.

CALENDAR EVENTS:
{}

ADDITIONAL CONTEXT:
The following additional context sources provide deeper insights into your life and work:
{}

**CONTEXT INTEGRATION GUIDELINES**:
- **Weather Conditions**: PRIORITIZE weather impacts on today and tomorrow's activities - storms affecting outdoor events, extreme temperatures, precipitation requiring plan changes, etc. Weather often matters more than logistics coordination
- **Obsidian Notes**: Use daily notes, project status, and relationship alerts for comprehensive insights
- **Task Management**: Consider overdue tasks, upcoming deadlines, and project dependencies
- **Relationship Alerts**: Prioritize communication needs based on last contact dates
- **Project Status**: Use project progress and deadlines for workload assessment

**ANALYSIS APPROACH**:
ALWAYS PRIORITIZE WEATHER CONCERNS FIRST. If there are weather alerts or conditions that could affect events, activities, or plans, make that your primary insight. Weather impacts (rain, storms, extreme temperatures) that could disrupt events should be the top priority over family logistics coordination. Only focus on logistics if there are no weather concerns affecting the user's plans.

Look for insights that are genuinely helpful and actionable. Focus on environmental factors and immediate practical impacts that THE USER needs to know about. 

**AVOID TRANSPORTATION ASSUMPTIONS**: Do not suggest how family members should get places or assume transportation needs. Adults and family members marked as "Coordination Needed" are capable of handling their own transportation, while those marked as "Parent Logistics" may need assistance. Simply state timing overlaps or conflicts as informational awareness without proposing specific logistics solutions.

**CREATIVE INSIGHT GENERATION**: Don't just follow example templates. Think creatively about what would be most helpful to know. Each situation is unique - craft insights that fit the specific circumstances rather than copying patterns from examples.

**PERSONAL TOUCH**: Act like a family assistant focused on helping THE USER specifically. When you see events for other family members, think about what that means for THE USER's responsibilities and schedule. Don't give advice about what the user's wife should do - focus on what THE USER needs to handle or be aware of.

**CALENDAR OWNERSHIP**: Check the "calendar_owner" field to understand whose events you're looking at:
- "Me (Family Coordination)" or "Me (Personal Tasks)" = Your personal events
- "Wife (Coordination Needed)" = Your wife's events  
- "Son/Daughter (Parent Logistics)" = Your children's events

**LOCATION & LOGIC**: Apply common sense about geography and logistics. If someone is traveling to another state, they obviously can't be in two places at once. Focus on what this means for the person staying behind.

**TEMPORAL RELEVANCE**: Prioritize events happening today and tomorrow over future events. A simple event tomorrow typically needs more immediate attention than a complex situation next week. Focus on what actually requires action or preparation in the near term rather than getting distracted by distant complexity.

**OBVIOUS CONTEXT RECOGNITION**: 
Use basic logic and common sense. If someone is traveling far away, they can't handle local responsibilities. If it's a child's activity and only one parent is available, it's obvious who handles it. Don't overcomplicate simple situations - just state the obvious implication directly.

Examples of the **types** of insights to look for (these are just examples of the categories, not templates to copy):
- Weather impacts on planned activities (storms affecting outdoor events, extreme temperatures, travel conditions)
- Preparation needs for immediate activities (deadlines approaching, items to gather)
- Simple responsibility changes when context obviously shifts
- Timing conflicts that need attention or backup plans
- Environmental factors affecting plans (weather, construction, etc.)

**IMPORTANT**: These are example **categories** only. Generate your own unique insights that fit these types of helpful observations, don't copy these patterns.

**TONE GUIDELINES:**
- Be personal and familiar, like a trusted family assistant focused on helping THE USER
- Show awareness of family dynamics but always from THE USER's perspective
- Speak like you understand what THE USER needs to know or handle
- Be proactive and caring, like Alfred from Batman - anticipating THE USER's needs
- Don't give advice about what others should do - focus on THE USER's responsibilities
- Create original insights that fit the situation - don't follow rigid templates
- Focus on what's genuinely helpful for THE USER specifically
- Keep it punchy and concise - 2-3 sentences maximum, no more
- Get to the point quickly, don't be wordy or over-explain

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
    "message": "Your direct, helpful insight about the most important thing to address (2-3 sentences max)",
    "icon": "📅",
    "context_hash": "placeholder",
    "generated_at": "2024-01-01T00:00:00Z"
  }}
}}

Focus on THE most important insight. Be specific to THIS calendar, not generic. Keep it concise and punchy."#,
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
            childcare_helper_term,
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

    async fn convert_single_insight_to_correlation(&self, insight: SingleInsight, _events: &[Event]) -> Result<Vec<Correlation>> {
        debug!("Converting single AI insight to correlation");
        
        // Reverse sanitize the insight to add back personal names
        let personalized_insight = self.reverse_sanitize_insight(&insight.message).await;
        
        let correlation = Correlation {
            id: uuid::Uuid::new_v4().to_string(),
            event_ids: vec![], // Simplified for now
            insight: personalized_insight,
            action_needed: "Review this insight".to_string(),
            urgency_score: 5, // Default medium urgency since we removed urgency scoring
            discovered_at: Utc::now(),
            recommended_glyph: Some(insight.icon),
        };
        
        debug!("Successfully converted to single correlation");
        Ok(vec![correlation])
    }

    async fn reverse_sanitize_insight(&self, insight: &str) -> String {
        let mut name_mappings = HashMap::new();
        
        // Strategy 1: Use explicit calendar_owners config if available
        let has_calendar_owners = {
            let config = self.config.read();
            if let Some(calendar_owners) = &config.calendar_owners {
                for (calendar_id, owner_name) in calendar_owners {
                    self.add_name_mappings_from_calendar(&mut name_mappings, calendar_id, owner_name);
                }
                true
            } else {
                false
            }
        };
        
        if !has_calendar_owners {
            // Strategy 2: Fallback - extract names from recent calendar events
            debug!("No calendar_owners config found, using event-based name extraction");
            if let Ok(name_mappings_from_events) = self.extract_names_from_recent_events().await {
                name_mappings.extend(name_mappings_from_events);
            }
        }
        
        // Apply the name replacements with intelligent word boundary matching
        let mut result = insight.to_string();
        for (generic_term, real_name) in &name_mappings {
            // Use whole word replacement to avoid partial matches
            result = self.replace_whole_words(&result, generic_term, real_name);
        }
        
        // Handle additional patterns that might appear in AI insights
        result = self.apply_additional_name_patterns(&result, &name_mappings);
        
        debug!("Reverse sanitized insight: {} mappings applied, original length {}, final length {}", 
               name_mappings.len(), insight.len(), result.len());
        result
    }
    
    /// Add name mappings based on calendar ID patterns
    fn add_name_mappings_from_calendar(&self, mappings: &mut HashMap<&'static str, String>, calendar_id: &str, owner_name: &str) {
        let calendar_lower = calendar_id.to_lowercase();
        
        // Map based on calendar naming patterns
        if calendar_lower.contains("wife") || calendar_lower.contains("spouse") {
            mappings.insert("your wife", owner_name.to_string());
            mappings.insert("Your wife", owner_name.to_string());
        } else if calendar_lower.contains("son") {
            mappings.insert("your son", owner_name.to_string());
            mappings.insert("Your son", owner_name.to_string());
        } else if calendar_lower.contains("daughter") {
            mappings.insert("your daughter", owner_name.to_string());
            mappings.insert("Your daughter", owner_name.to_string());
        } else if calendar_lower.contains("husband") || calendar_lower.contains("partner") {
            mappings.insert("your husband", owner_name.to_string());
            mappings.insert("Your husband", owner_name.to_string());
        }
        
        // Handle other family relationships that might appear in insights
        if calendar_lower.contains("child") {
            mappings.insert("your child", owner_name.to_string());
            mappings.insert("Your child", owner_name.to_string());
        }
    }
    
    /// Extract names from recent calendar events as a fallback
    async fn extract_names_from_recent_events(&self) -> Result<HashMap<&'static str, String>> {
        let mut mappings = HashMap::new();
        
        // Get recent events (last 30 days)
        let now = Utc::now();
        let start_time = now - chrono::Duration::days(30);
        
        match self.database.get_events_in_range(start_time, now) {
            Ok(events) => {
                // Collect calendar owners from recent enriched events
                let enriched_events = self.enrich_events_with_calendar_info(&events).await?;
                
                for enriched in &enriched_events {
                    if let Some(ref calendar_info) = enriched.calendar_info {
                        let owner_name = self.sanitizer.extract_calendar_owner_with_config(&calendar_info.calendar_id, None);
                        
                        // Only add if we got a meaningful name (not generic terms)
                        if !owner_name.contains("Unknown") && !owner_name.contains("Person") && owner_name.len() > 1 {
                            self.add_name_mappings_from_calendar(&mut mappings, &calendar_info.calendar_id, &owner_name);
                        }
                    }
                }
                
                debug!("Extracted {} name mappings from recent events", mappings.len());
            }
            Err(e) => {
                debug!("Failed to extract names from recent events: {}", e);
            }
        }
        
        Ok(mappings)
    }
    
    /// Replace whole words to avoid partial matches
    fn replace_whole_words(&self, text: &str, pattern: &str, replacement: &str) -> String {
        // Use regex for word boundary matching, but fallback to simple replace if regex fails
        match regex::Regex::new(&format!(r"\b{}\b", regex::escape(pattern))) {
            Ok(re) => re.replace_all(text, replacement).to_string(),
            Err(_) => text.replace(pattern, replacement),
        }
    }
    
    /// Apply additional name patterns that might appear in AI insights
    fn apply_additional_name_patterns(&self, text: &str, name_mappings: &HashMap<&'static str, String>) -> String {
        let mut result = text.to_string();
        
        // Handle possessive forms
        for (generic_term, real_name) in name_mappings {
            if generic_term.starts_with("your ") {
                let possessive_generic = format!("{}'s", generic_term);
                let possessive_real = format!("{}'s", real_name);
                result = self.replace_whole_words(&result, &possessive_generic, &possessive_real);
            }
        }
        
        // Handle additional relationship terms that might be inferred
        if let Some(wife_name) = name_mappings.get("your wife") {
            // Handle terms like "she" when referring to wife in context
            // This is more complex and would need context analysis, so keeping simple for now
            result = result.replace("your wife's", &format!("{}'s", wife_name));
        }
        
        if let Some(son_name) = name_mappings.get("your son") {
            result = result.replace("your son's", &format!("{}'s", son_name));
        }
        
        if let Some(daughter_name) = name_mappings.get("your daughter") {
            result = result.replace("your daughter's", &format!("{}'s", daughter_name));
        }
        
        result
    }
    
    /// Auto-discover and populate calendar owners from existing calendar data
    pub async fn auto_discover_calendar_owners(&self) -> Result<()> {
        debug!("Auto-discovering calendar owners from database");
        
        // Get recent events to analyze calendar patterns (last 90 days)
        let now = Utc::now();
        let start_time = now - chrono::Duration::days(90);
        
        let events = self.database.get_events_in_range(start_time, now)?;
        let enriched_events = self.enrich_events_with_calendar_info(&events).await?;
        
        let mut discovered_owners = HashMap::new();
        
        for enriched in &enriched_events {
            if let Some(ref calendar_info) = enriched.calendar_info {
                // Extract a meaningful name using the enhanced sanitizer
                let owner_name = self.sanitizer.extract_calendar_owner_with_config(&calendar_info.calendar_id, None);
                
                // Only store meaningful names (not generic fallbacks)
                if !owner_name.contains("Unknown") && !owner_name.contains("Person") && owner_name.len() > 1 {
                    discovered_owners.insert(calendar_info.calendar_id.clone(), owner_name);
                }
            }
        }
        
        if !discovered_owners.is_empty() {
            debug!("Discovered {} calendar owners: {:?}", discovered_owners.len(), discovered_owners);
            
            // Update the config with discovered owners
            {
                let mut config = self.config.write();
                config.update_calendar_owners(discovered_owners);
            }
            
            info!("Auto-discovery populated calendar owners configuration");
        } else {
            debug!("No meaningful calendar owners discovered");
        }
        
        Ok(())
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