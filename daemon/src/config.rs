use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::fs;
use tracing::{info, debug, warn};
use chrono;
use chrono_tz;

use crate::sops_integration::SopsSecrets;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub general: GeneralConfig,
    pub ai: AiConfig,
    pub insights: InsightsConfig,
    pub personality: PersonalityConfig,
    pub google_calendar: Option<GoogleCalendarConfig>,
    pub calendar_owners: Option<HashMap<String, String>>,
    pub context_sources: Option<ContextSourcesConfig>,
    pub notifications: Option<NotificationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleCalendarConfig {
    pub enabled: bool,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub calendar_ids: Vec<String>,
    pub sync_interval_minutes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// How far ahead should Jasper look?
    pub planning_horizon_days: u32,
    /// How often to check for new correlations (minutes)
    pub analysis_interval: u32,
    /// Daemon behavior
    pub auto_start: bool,
    /// Log level
    pub log_level: String,
    /// Timezone for displaying events
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// LLM provider: "openai", "anthropic"
    pub provider: String,
    /// Model name
    pub model: String,
    /// Maximum tokens for LLM responses
    pub max_tokens: u32,
    /// Temperature for LLM
    pub temperature: f32,
    /// API key for the AI provider (optional - falls back to environment variable)
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightsConfig {
    /// What types of insights do you want?
    pub enable_travel_prep: bool,
    pub enable_maintenance_conflicts: bool,
    pub enable_overcommitment_warnings: bool,
    pub enable_pattern_observations: bool,
    
    /// Urgency thresholds
    pub high_urgency_days: u32,
    pub medium_urgency_days: u32,
    
    /// Insight delivery
    pub max_insights_per_day: u32,
    pub quiet_hours_start: String,
    pub quiet_hours_end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalityConfig {
    /// How should Jasper address you?
    pub user_title: String,
    /// Formality level: "formal", "balanced", "casual"
    pub formality: String,
    /// Humor level: "none", "occasional", "frequent"
    pub humor_level: String,
    /// Assistant persona description
    pub assistant_persona: String,
    /// Optional persona reference for personality (e.g., "like Alfred from Batman")
    pub persona_reference: Option<String>,
    /// Term to use for childcare helper availability (e.g., "Helper Day", "Nanny Day")
    pub childcare_helper_term: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSourcesConfig {
    pub obsidian: Option<ObsidianConfig>,
    pub weather: Option<WeatherConfig>,
    pub tasks: Option<TasksConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObsidianConfig {
    pub enabled: bool,
    pub vault_path: String,
    pub daily_notes_folder: String,
    pub daily_notes_format: String,
    pub templates_folder: String,
    pub people_folder: String,
    pub projects_folder: String,
    pub parse_dataview: bool,
    pub parse_tasks: bool,
    pub parse_frontmatter: bool,
    pub relationship_alert_days: i64,
    pub ignored_folders: Vec<String>,
    pub ignored_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherConfig {
    pub enabled: bool,
    pub api_key: String,
    pub location: String,
    pub units: String, // "metric", "imperial", "kelvin"
    pub cache_duration_minutes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TasksConfig {
    pub enabled: bool,
    pub source_type: String, // "todoist", "local_file", "obsidian"
    pub api_key: Option<String>,
    pub file_path: Option<String>,
    pub sync_completed: bool,
    pub max_tasks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub notify_new_insights: bool,
    pub notify_context_changes: bool,
    pub notify_cache_refresh: bool,
    pub notification_timeout: u32, // milliseconds
    pub min_urgency_threshold: i32, // minimum urgency score to notify
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                planning_horizon_days: 7,
                analysis_interval: 30,
                auto_start: true,
                log_level: "info".to_string(),
                timezone: "America/Detroit".to_string(),
            },
            ai: AiConfig {
                provider: "anthropic".to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
                max_tokens: 2000,
                temperature: 0.7,
                api_key: None, // Falls back to ANTHROPIC_API_KEY environment variable
            },
            insights: InsightsConfig {
                enable_travel_prep: true,
                enable_maintenance_conflicts: true,
                enable_overcommitment_warnings: true,
                enable_pattern_observations: true,
                high_urgency_days: 2,
                medium_urgency_days: 5,
                max_insights_per_day: 10,
                quiet_hours_start: "22:00".to_string(),
                quiet_hours_end: "08:00".to_string(),
            },
            personality: PersonalityConfig {
                user_title: "Sir".to_string(),
                formality: "balanced".to_string(),
                humor_level: "occasional".to_string(),
                assistant_persona: "trusted family assistant".to_string(),
                persona_reference: Some("like Alfred from Batman".to_string()),
                childcare_helper_term: "Helper Day".to_string(),
            },
            google_calendar: Some(GoogleCalendarConfig {
                enabled: false, // Disabled by default, user must configure
                client_id: String::new(),
                client_secret: String::new(),
                redirect_uri: "http://localhost:8080/auth/callback".to_string(),
                calendar_ids: vec!["primary".to_string()],
                sync_interval_minutes: 15,
            }),
            calendar_owners: None,
            context_sources: Some(ContextSourcesConfig {
                obsidian: Some(ObsidianConfig {
                    enabled: true,
                    vault_path: "~/Documents/Obsidian Vault".to_string(),
                    daily_notes_folder: "Work/Daily".to_string(),
                    daily_notes_format: "YYYY-MM-DD".to_string(),
                    templates_folder: "Templates".to_string(),
                    people_folder: "Work/People".to_string(),
                    projects_folder: "Work/Projects".to_string(),
                    parse_dataview: true,
                    parse_tasks: true,
                    parse_frontmatter: true,
                    relationship_alert_days: 21,
                    ignored_folders: vec![".obsidian".to_string(), ".trash".to_string()],
                    ignored_files: vec![".DS_Store".to_string()],
                }),
                weather: Some(WeatherConfig {
                    enabled: false, // Disabled by default, needs API key
                    api_key: String::new(),
                    location: "Detroit, MI".to_string(),
                    units: "imperial".to_string(),
                    cache_duration_minutes: 30,
                }),
                tasks: Some(TasksConfig {
                    enabled: false, // Disabled by default
                    source_type: "todoist".to_string(),
                    api_key: None,
                    file_path: None,
                    sync_completed: true,
                    max_tasks: 100,
                }),
            }),
            notifications: Some(NotificationConfig {
                enabled: true,
                notify_new_insights: true,
                notify_context_changes: false, // Less noisy by default
                notify_cache_refresh: false,   // Less noisy by default
                notification_timeout: 5000,    // 5 seconds
                min_urgency_threshold: 3,      // Medium+ urgency
            }),
        }
    }
}

impl Config {
    pub async fn load() -> Result<Arc<RwLock<Config>>> {
        let config_path = Self::get_config_path()?;
        
        let mut config = if config_path.exists() {
            let content = fs::read_to_string(&config_path).await
                .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
            
            toml::from_str(&content)
                .with_context(|| "Failed to parse config file")?
        } else {
            info!("Config file not found, creating default configuration");
            let default_config = Config::default();
            default_config.save().await?;
            default_config
        };
        
        // Load secrets from SOPS and override config values
        match SopsSecrets::load() {
            Ok(secrets) => {
                config.apply_sops_secrets(&secrets);
            }
            Err(e) => {
                warn!("Failed to load SOPS secrets, using config file values: {}", e);
            }
        }
        
        Ok(Arc::new(RwLock::new(config)))
    }
    
    /// Apply SOPS secrets to override config values
    fn apply_sops_secrets(&mut self, secrets: &SopsSecrets) {
        // Override AI API key
        if let Some(claude_api_key) = secrets.get("claude_api_key") {
            debug!("Using Claude API key from SOPS");
            self.ai.api_key = Some(claude_api_key.clone());
        }
        
        // Override Google Calendar client secret
        if let Some(google_client_secret) = secrets.get("google_calendar_client_secret") {
            debug!("Using Google Calendar client secret from SOPS");
            if let Some(ref mut google_config) = self.google_calendar {
                google_config.client_secret = google_client_secret.clone();
            }
        }
        
        // Override weather API key
        if let Some(weather_api_key) = secrets.get("openweathermap_api_key") {
            debug!("Using OpenWeatherMap API key from SOPS");
            if let Some(ref mut context_sources) = self.context_sources {
                if let Some(ref mut weather_config) = context_sources.weather {
                    weather_config.api_key = weather_api_key.clone();
                }
            }
        }
        
        // Override Todoist API key if present
        if let Some(todoist_api_key) = secrets.get("todoist_api_key") {
            debug!("Using Todoist API key from SOPS");
            if let Some(ref mut context_sources) = self.context_sources {
                if let Some(ref mut tasks_config) = context_sources.tasks {
                    tasks_config.api_key = Some(todoist_api_key.clone());
                }
            }
        }
        
        info!("Applied SOPS secrets to configuration");
    }
    
    pub async fn save(&self) -> Result<()> {
        let config_path = Self::get_config_path()?;
        
        // Ensure config directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).await
                .with_context(|| format!("Failed to create config directory: {:?}", parent))?;
        }
        
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        fs::write(&config_path, content).await
            .with_context(|| format!("Failed to write config file: {:?}", config_path))?;
        
        info!("Configuration saved to {:?}", config_path);
        Ok(())
    }
    
    pub fn get_config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Failed to get config directory")?
            .join("jasper-companion");
        
        Ok(config_dir.join("config.toml"))
    }
    
    pub fn get_database_path(&self) -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap().join(".local/share"))
            .join("jasper-companion")
            .join("app_data.db")
    }
    
    pub fn get_data_dir() -> Result<PathBuf> {
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap().join(".local/share"))
            .join("jasper-companion");
        
        Ok(data_dir)
    }
    
    pub fn get_calendar_owner(&self, calendar_id: &str) -> Option<String> {
        self.calendar_owners.as_ref()
            .and_then(|owners| owners.get(calendar_id))
            .cloned()
    }
    
    /// Get personality information as a tuple (user_title, formality, timezone)
    pub fn get_personality_info(&self) -> (String, String, String) {
        (
            self.personality.user_title.clone(),
            self.personality.formality.clone(),
            self.general.timezone.clone()
        )
    }
    
    /// Get enhanced personality configuration for prompt generation
    pub fn get_personality_config(&self) -> (&PersonalityConfig, &str) {
        (&self.personality, &self.general.timezone)
    }
    
    /// Update calendar owners mapping from discovered calendar information
    pub fn update_calendar_owners(&mut self, calendar_owners: HashMap<String, String>) {
        self.calendar_owners = Some(calendar_owners);
    }
    
    /// Get planning horizon as chrono Duration
    pub fn get_planning_horizon(&self) -> chrono::Duration {
        chrono::Duration::days(self.general.planning_horizon_days as i64)
    }
    
    /// Get API key from config or environment variable
    pub fn get_api_key(&self) -> Option<String> {
        self.ai.api_key.clone()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
    }
    
    /// Get API configuration (provider, model, max_tokens, temperature)
    pub fn get_api_config(&self) -> (String, String, u32, f32) {
        (
            self.ai.provider.clone(),
            self.ai.model.clone(),
            self.ai.max_tokens,
            self.ai.temperature
        )
    }
    
    /// Get timezone as parsed Tz object, falling back to UTC if invalid
    pub fn get_timezone(&self) -> chrono_tz::Tz {
        self.general.timezone.parse::<chrono_tz::Tz>()
            .unwrap_or(chrono_tz::UTC)
    }
    
    /// Get Obsidian configuration
    pub fn get_obsidian_config(&self) -> Option<&ObsidianConfig> {
        self.context_sources.as_ref()?.obsidian.as_ref()
    }
    
    /// Get Weather configuration
    pub fn get_weather_config(&self) -> Option<&WeatherConfig> {
        self.context_sources.as_ref()?.weather.as_ref()
    }
    
    /// Get Tasks configuration
    pub fn get_tasks_config(&self) -> Option<&TasksConfig> {
        self.context_sources.as_ref()?.tasks.as_ref()
    }
    
    /// Get Notification configuration
    pub fn get_notification_config(&self) -> Option<&NotificationConfig> {
        self.notifications.as_ref()
    }
    
    /// Check if a context source is enabled
    pub fn is_context_source_enabled(&self, source_id: &str) -> bool {
        match source_id {
            "obsidian" => self.get_obsidian_config().map_or(false, |c| c.enabled),
            "weather" => self.get_weather_config().map_or(false, |c| c.enabled),
            "tasks" => self.get_tasks_config().map_or(false, |c| c.enabled),
            "calendar" => true, // Always enabled
            _ => false,
        }
    }
}