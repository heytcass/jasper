#![allow(dead_code)]

use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::fs;
use tracing::{info, debug, warn};
use chrono;
use chrono_tz;
// URL validation without external crate

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
    /// Don't generate insights during sleep hours
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
    /// Notification method preference (auto, dbus, notify-send)
    #[serde(default = "default_notification_method")]
    pub preferred_method: String,
    /// Application name for notifications
    #[serde(default = "default_app_name")]
    pub app_name: String,
    /// Custom desktop entry name for better integration
    #[serde(default = "default_desktop_entry")]
    pub desktop_entry: String,
}

// Default functions for notification configuration
fn default_notification_method() -> String {
    "auto".to_string()
}

fn default_app_name() -> String {
    "Jasper".to_string()
}

fn default_desktop_entry() -> String {
    "jasper".to_string()
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
                model: "claude-sonnet-4-5".to_string(),
                max_tokens: 2000,
                temperature: 0.7,
                api_key: None, // Falls back to ANTHROPIC_API_KEY environment variable
            },
            insights: InsightsConfig {
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
                preferred_method: default_notification_method(),
                app_name: default_app_name(),
                desktop_entry: default_desktop_entry(),
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
        
        // Validate configuration
        config.validate()?;
        
        Ok(Arc::new(RwLock::new(config)))
    }
    
    /// Apply SOPS secrets to override config values
    fn apply_sops_secrets(&mut self, secrets: &SopsSecrets) {
        // Override AI API key
        if let Some(anthropic_api_key) = secrets.get("services.anthropic_api_key") {
            debug!("Using Anthropic API key from SOPS");
            self.ai.api_key = Some(anthropic_api_key.clone());
        }
        
        // Override Google Calendar credentials
        if let Some(google_client_id) = secrets.get("services.google_calendar.client_id") {
            debug!("Using Google Calendar client ID from SOPS");
            if let Some(ref mut google_config) = self.google_calendar {
                google_config.client_id = google_client_id.clone();
            }
        }
        
        if let Some(google_client_secret) = secrets.get("services.google_calendar.client_secret") {
            debug!("Using Google Calendar client secret from SOPS");
            if let Some(ref mut google_config) = self.google_calendar {
                google_config.client_secret = google_client_secret.clone();
            }
        }
        
        // Override weather API key
        if let Some(weather_api_key) = secrets.get("services.openweathermap") {
            debug!("Using OpenWeatherMap API key from SOPS");
            if let Some(ref mut context_sources) = self.context_sources {
                if let Some(ref mut weather_config) = context_sources.weather {
                    weather_config.api_key = weather_api_key.clone();
                    weather_config.enabled = true; // Enable weather when API key is available
                    debug!("Weather context source enabled with API key from SOPS");
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
    
    pub fn get_database_path(&self) -> Result<PathBuf> {
        let data_dir = dirs::data_local_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
            .ok_or_else(|| anyhow::anyhow!("Unable to determine data directory"))?;
        
        Ok(data_dir.join("jasper-companion").join("app_data.db"))
    }
    
    pub fn get_data_dir() -> Result<PathBuf> {
        let data_dir = dirs::data_local_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
            .ok_or_else(|| anyhow::anyhow!("Unable to determine data directory"))?
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
    
    /// Validate configuration values
    fn validate(&self) -> Result<()> {
        self.validate_basic_config()
            .context("Basic configuration validation failed")?;
        
        self.validate_network_config()
            .context("Network configuration validation failed")?;
        
        self.validate_dependencies()
            .context("Dependency validation failed")?;
        
        self.validate_context_sources()
            .context("Context sources validation failed")?;
        
        self.validate_security_settings()
            .context("Security settings validation failed")?;
        
        Ok(())
    }
    
    /// Validate email address format
    fn validate_email(&self, email: &str, field_name: &str) -> Result<()> {
        if email.is_empty() {
            return Err(anyhow::anyhow!("{} cannot be empty", field_name));
        }
        
        // Basic email validation without external dependencies
        let email_parts: Vec<&str> = email.split('@').collect();
        if email_parts.len() != 2 {
            return Err(anyhow::anyhow!(
                "{} '{}' is invalid: must contain exactly one @ symbol", 
                field_name, email
            ));
        }
        
        let local = email_parts[0];
        let domain = email_parts[1];
        
        // Validate local part
        if local.is_empty() || local.len() > 64 {
            return Err(anyhow::anyhow!(
                "{} '{}' is invalid: local part must be 1-64 characters", 
                field_name, email
            ));
        }
        
        // Validate domain part
        if domain.is_empty() || domain.len() > 255 {
            return Err(anyhow::anyhow!(
                "{} '{}' is invalid: domain part must be 1-255 characters", 
                field_name, email
            ));
        }
        
        // Basic domain format check
        if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') {
            return Err(anyhow::anyhow!(
                "{} '{}' is invalid: domain must contain at least one dot and not start/end with dot", 
                field_name, email
            ));
        }
        
        // Check for invalid characters (basic check)
        if email.contains(' ') || email.contains('\t') || email.contains('\n') {
            return Err(anyhow::anyhow!(
                "{} '{}' is invalid: cannot contain whitespace", 
                field_name, email
            ));
        }
        
        Ok(())
    }
    
    /// Validate URL format
    fn validate_url(&self, url: &str, field_name: &str) -> Result<()> {
        if url.is_empty() {
            return Err(anyhow::anyhow!("{} cannot be empty", field_name));
        }
        
        // Basic URL validation without external dependencies
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(anyhow::anyhow!(
                "{} '{}' is invalid: must start with http:// or https://", 
                field_name, url
            ));
        }
        
        // Check for basic URL structure
        let url_without_scheme = if url.starts_with("https://") {
            &url[8..]
        } else {
            &url[7..]
        };
        
        if url_without_scheme.is_empty() {
            return Err(anyhow::anyhow!(
                "{} '{}' is invalid: missing domain after scheme", 
                field_name, url
            ));
        }
        
        // Basic domain validation (before first slash or end of string)
        let domain_part = url_without_scheme.split('/').next().unwrap_or("");
        if domain_part.is_empty() {
            return Err(anyhow::anyhow!(
                "{} '{}' is invalid: missing domain", 
                field_name, url
            ));
        }
        
        // Check for invalid characters
        if url.contains(' ') || url.contains('\t') || url.contains('\n') {
            return Err(anyhow::anyhow!(
                "{} '{}' is invalid: cannot contain whitespace", 
                field_name, url
            ));
        }
        
        // Check for localhost/IP addresses for security warnings
        if url.starts_with("http://") && !domain_part.starts_with("localhost") && 
           !domain_part.starts_with("127.0.0.1") && !domain_part.starts_with("::1") {
            warn!("{} uses HTTP (not HTTPS) with non-localhost domain: {}", field_name, url);
        }
        
        Ok(())
    }
    
    /// Validate timezone string
    fn validate_timezone(&self, timezone: &str, field_name: &str) -> Result<()> {
        if timezone.is_empty() {
            return Err(anyhow::anyhow!("{} cannot be empty", field_name));
        }
        
        // Use chrono_tz for validation
        match timezone.parse::<chrono_tz::Tz>() {
            Ok(_) => Ok(()),
            Err(_) => {
                let suggestions = vec![
                    "UTC", "America/New_York", "Europe/London", "Asia/Tokyo", 
                    "America/Detroit", "Europe/Paris", "Asia/Shanghai"
                ];
                Err(anyhow::anyhow!(
                    "{} '{}' is invalid timezone. Common examples: {}", 
                    field_name, timezone, suggestions.join(", ")
                ))
            }
        }
    }
    
    fn validate_basic_config(&self) -> Result<()> {
        // Validate timezone using helper function
        self.validate_timezone(&self.general.timezone, "general.timezone")?;
        
        // Validate planning horizon with reasonable bounds
        if self.general.planning_horizon_days == 0 {
            return Err(anyhow::anyhow!("Planning horizon must be at least 1 day"));
        }
        if self.general.planning_horizon_days > 365 {
            return Err(anyhow::anyhow!(
                "Planning horizon cannot exceed 365 days (got: {}). Consider a more reasonable timeframe for performance.", 
                self.general.planning_horizon_days
            ));
        }
        if self.general.planning_horizon_days > 90 {
            warn!("Planning horizon of {} days is quite large and may impact performance", 
                  self.general.planning_horizon_days);
        }
        
        // Enhanced AI configuration validation
        if self.ai.temperature < 0.0 || self.ai.temperature > 2.0 {
            return Err(anyhow::anyhow!(
                "AI temperature must be between 0.0-2.0 (got: {}). Lower values (0.1-0.7) are recommended for consistent results.", 
                self.ai.temperature
            ));
        }
        if self.ai.temperature > 1.5 {
            warn!("High AI temperature ({}) may produce unpredictable results", self.ai.temperature);
        }
        
        // Validate AI max tokens with model-specific limits
        let max_token_limit = if self.ai.model.contains("gpt-4") {
            128000 // GPT-4 Turbo limit
        } else if self.ai.model.contains("claude") {
            200000 // Claude 3 limit
        } else {
            32000 // Conservative default
        };
        
        if self.ai.max_tokens < 100 {
            return Err(anyhow::anyhow!("AI max_tokens too low ({}). Minimum 100 tokens required for meaningful responses.", self.ai.max_tokens));
        }
        if self.ai.max_tokens > max_token_limit {
            return Err(anyhow::anyhow!(
                "AI max_tokens ({}) exceeds model limit for '{}' ({})", 
                self.ai.max_tokens, self.ai.model, max_token_limit
            ));
        }
        
        // Validate and parse quiet hours with logical consistency
        let quiet_start = chrono::NaiveTime::parse_from_str(&self.insights.quiet_hours_start, "%H:%M")
            .map_err(|_| anyhow::anyhow!(
                "Invalid quiet_hours_start '{}'. Expected format: HH:MM (24-hour format)", 
                self.insights.quiet_hours_start
            ))?;
        
        let quiet_end = chrono::NaiveTime::parse_from_str(&self.insights.quiet_hours_end, "%H:%M")
            .map_err(|_| anyhow::anyhow!(
                "Invalid quiet_hours_end '{}'. Expected format: HH:MM (24-hour format)", 
                self.insights.quiet_hours_end
            ))?;
        
        // Check for logical quiet hours (allowing overnight periods)
        if quiet_start == quiet_end {
            warn!("Quiet hours start and end are the same ({}). This effectively disables all insights.", 
                  self.insights.quiet_hours_start);
        }
        
        Ok(())
    }
    
    fn validate_network_config(&self) -> Result<()> {
        // Validate Google Calendar OAuth configuration
        if let Some(ref gc) = self.google_calendar {
            if gc.enabled {
                self.validate_oauth_config(&gc.client_id, &gc.client_secret, &gc.redirect_uri)
                    .context("Google Calendar OAuth validation failed")?;
            }
        }
        
        Ok(())
    }
    
    fn validate_oauth_config(&self, client_id: &str, client_secret: &str, redirect_uri: &str) -> Result<()> {
        // Validate client ID format (Google OAuth client IDs have specific patterns)
        if client_id.is_empty() {
            return Err(anyhow::anyhow!("OAuth client_id cannot be empty"));
        }
        if client_id.len() < 50 || !client_id.ends_with(".googleusercontent.com") {
            warn!("client_id '{}' doesn't match expected Google OAuth format", 
                  &client_id[..client_id.len().min(20)]);
        }
        
        // Validate client secret
        if client_secret.is_empty() {
            return Err(anyhow::anyhow!("OAuth client_secret cannot be empty"));
        }
        if client_secret.len() < 20 {
            warn!("client_secret appears to be too short for a valid OAuth secret");
        }
        
        // Validate redirect URI
        if redirect_uri.is_empty() {
            return Err(anyhow::anyhow!("OAuth redirect_uri cannot be empty"));
        }
        
        // Use comprehensive URL validation
        self.validate_url(redirect_uri, "redirect_uri")?;
        
        // Security checks for redirect URI
        if redirect_uri.starts_with("http://") {
            if !redirect_uri.contains("localhost") && !redirect_uri.contains("127.0.0.1") {
                return Err(anyhow::anyhow!(
                    "HTTP redirect_uri only allowed for localhost/127.0.0.1, got: {}", 
                    redirect_uri
                ));
            }
        }
        
        // Check for common OAuth security issues
        if redirect_uri.contains("#") {
            warn!("redirect_uri contains fragment (#), which may cause OAuth issues");
        }
        
        Ok(())
    }
    
    fn validate_dependencies(&self) -> Result<()> {
        // Check for optional dependencies based on enabled features
        if let Some(ref sources) = self.context_sources {
            if let Some(ref obsidian) = sources.obsidian {
                if obsidian.enabled {
                    self.validate_obsidian_config(obsidian)
                        .context("Obsidian configuration validation failed")?;
                }
            }
        }
        
        Ok(())
    }
    
    fn validate_obsidian_config(&self, obsidian: &ObsidianConfig) -> Result<()> {
        let vault_path = Path::new(&obsidian.vault_path);
        
        // Check if vault path exists
        if !vault_path.exists() {
            return Err(anyhow::anyhow!(
                "Obsidian vault path does not exist: {}", 
                obsidian.vault_path
            ));
        }
        
        // Check if it's a directory
        if !vault_path.is_dir() {
            return Err(anyhow::anyhow!(
                "Obsidian vault path is not a directory: {}", 
                obsidian.vault_path
            ));
        }
        
        // Check for .obsidian directory (indicates valid Obsidian vault)
        let obsidian_dir = vault_path.join(".obsidian");
        if !obsidian_dir.exists() {
            warn!("No .obsidian directory found in '{}'. This may not be a valid Obsidian vault.", 
                  obsidian.vault_path);
        }
        
        // Obsidian config is valid - check basic required folders exist
        debug!("Obsidian vault validation successful for: {}", obsidian.vault_path);
        
        Ok(())
    }
    
    fn validate_context_sources(&self) -> Result<()> {
        if let Some(ref sources) = self.context_sources {
            let mut enabled_sources = 0;
            
            // Check each context source (calendar via Google Calendar config)
            if let Some(ref gc) = self.google_calendar {
                if gc.enabled { enabled_sources += 1; }
            }
            if let Some(ref obs) = sources.obsidian {
                if obs.enabled { enabled_sources += 1; }
            }
            if let Some(ref weather) = sources.weather {
                if weather.enabled { enabled_sources += 1; }
            }
            
            if enabled_sources == 0 {
                warn!("No context sources are enabled. The system will have limited functionality.");
            }
            
            debug!("Validated {} enabled context sources", enabled_sources);
        } else {
            warn!("No context sources configuration found");
        }
        
        Ok(())
    }
    
    fn validate_security_settings(&self) -> Result<()> {
        // Check for insecure configurations
        if let Some(ref gc) = self.google_calendar {
            if gc.enabled && gc.redirect_uri.starts_with("http://") {
                if !gc.redirect_uri.contains("localhost") && !gc.redirect_uri.contains("127.0.0.1") {
                    return Err(anyhow::anyhow!(
                        "HTTP OAuth redirect is insecure for non-localhost URLs: {}", 
                        gc.redirect_uri
                    ));
                }
            }
        }
        
        // Validate configured paths don't have obvious security issues
        if let Some(ref sources) = self.context_sources {
            if let Some(ref obs) = sources.obsidian {
                if obs.enabled {
                    if obs.vault_path.contains("../") || obs.vault_path.starts_with("/tmp/") {
                        warn!("Potentially insecure vault path: {}", obs.vault_path);
                    }
                }
            }
        }
        
        Ok(())
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