#![allow(dead_code)]

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod obsidian;
pub mod calendar;
pub mod weather;
pub mod tasks;

/// Core trait for all context sources
#[async_trait]
pub trait ContextSource: Send + Sync {
    /// Get the unique identifier for this context source
    fn source_id(&self) -> &str;
    
    /// Get the display name for this context source
    fn display_name(&self) -> &str;
    
    /// Check if this context source is enabled and configured
    fn is_enabled(&self) -> bool;
    
    /// Fetch context data for the given time range
    async fn fetch_context(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<ContextData>;
    
    /// Get the priority of this context source (higher = more important)
    fn priority(&self) -> i32 {
        100 // Default priority
    }
    
    /// Get configuration requirements for this source
    fn required_config(&self) -> Vec<String> {
        vec![]
    }
    
    /// Validate configuration for this source
    fn validate_config(&self, config: &HashMap<String, String>) -> Result<()> {
        // Check required config keys
        for key in self.required_config() {
            if !config.contains_key(&key) {
                return Err(anyhow::anyhow!("Missing required config key: {}", key));
            }
        }
        Ok(())
    }
}

/// Context data returned by context sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextData {
    pub source_id: String,
    pub timestamp: DateTime<Utc>,
    pub data_type: ContextDataType,
    pub priority: i32,
    pub content: ContextContent,
    pub metadata: HashMap<String, String>,
}

/// Types of context data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextDataType {
    Calendar,
    Tasks,
    Notes,
    Weather,
    Traffic,
    Health,
    Financial,
    Social,
    Learning,
    Other(String),
}

/// Content payload for context data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextContent {
    Calendar(CalendarContext),
    Tasks(TaskContext),
    Notes(NotesContext),
    Weather(WeatherContext),
    Generic(GenericContext),
}

/// Calendar-specific context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarContext {
    pub events: Vec<crate::database::Event>,
    pub conflicts: Vec<String>,
    pub upcoming_deadlines: Vec<String>,
}

/// Task-specific context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub tasks: Vec<Task>,
    pub overdue_count: usize,
    pub upcoming_count: usize,
}

/// Notes-specific context (for Obsidian integration)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesContext {
    pub daily_notes: Vec<DailyNote>,
    pub active_projects: Vec<Project>,
    pub recent_activities: Vec<Activity>,
    pub pending_tasks: Vec<Task>,
    pub relationship_alerts: Vec<RelationshipAlert>,
}

/// Weather-specific context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherContext {
    pub current_conditions: String,
    pub forecast: Vec<WeatherForecast>,
    pub alerts: Vec<String>,
}

/// Generic context for extensibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericContext {
    pub data: HashMap<String, serde_json::Value>,
    pub summary: String,
    pub insights: Vec<String>,
}

/// Task representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub due_date: Option<DateTime<Utc>>,
    pub priority: i32,
    pub status: TaskStatus,
    pub tags: Vec<String>,
    pub source: String,
}

/// Task status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
    Blocked,
}

/// Daily note representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyNote {
    pub date: DateTime<Utc>,
    pub title: String,
    pub content: String,
    pub tasks: Vec<Task>,
    pub mood: Option<String>,
    pub energy_level: Option<i32>,
    pub focus_areas: Vec<String>,
}

/// Project representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: ProjectStatus,
    pub due_date: Option<DateTime<Utc>>,
    pub client: Option<String>,
    pub priority: i32,
    pub progress: f32, // 0.0 to 1.0
    pub tasks: Vec<Task>,
}

/// Project status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProjectStatus {
    Active,
    Pending,
    Completed,
    OnHold,
    Cancelled,
}

/// Activity representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub activity_type: ActivityType,
    pub duration: Option<i64>, // minutes
    pub outcome: Option<String>,
}

/// Activity type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActivityType {
    Meeting,
    Call,
    Email,
    Work,
    Learning,
    Personal,
    Travel,
    Other(String),
}

/// Relationship alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipAlert {
    pub person_name: String,
    pub company: Option<String>,
    pub last_contact: DateTime<Utc>,
    pub days_since_contact: i64,
    pub relationship_type: RelationshipType,
    pub urgency: i32,
}

/// Relationship type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelationshipType {
    Professional,
    Personal,
    Client,
    Vendor,
    Family,
    Friend,
    Other(String),
}

/// Weather forecast
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherForecast {
    pub date: DateTime<Utc>,
    pub temperature_high: f32,
    pub temperature_low: f32,
    pub conditions: String,
    pub precipitation_chance: f32,
    pub description: String,
}

/// Context source manager
pub struct ContextSourceManager {
    sources: Vec<Box<dyn ContextSource>>,
}

impl ContextSourceManager {
    /// Create a new context source manager
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }
    
    /// Add a context source
    pub fn add_source(&mut self, source: Box<dyn ContextSource>) {
        self.sources.push(source);
    }
    
    /// Get all enabled context sources
    pub fn get_enabled_sources(&self) -> Vec<&dyn ContextSource> {
        self.sources
            .iter()
            .filter(|s| s.is_enabled())
            .map(|s| s.as_ref())
            .collect()
    }
    
    /// Fetch context from all enabled sources
    pub async fn fetch_all_context(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<ContextData>> {
        let mut all_context = Vec::new();
        
        for source in self.get_enabled_sources() {
            match source.fetch_context(start, end).await {
                Ok(context) => all_context.push(context),
                Err(e) => {
                    tracing::warn!("Failed to fetch context from {}: {}", source.source_id(), e);
                }
            }
        }
        
        // Sort by priority (higher priority first)
        all_context.sort_by(|a, b| b.priority.cmp(&a.priority));
        
        Ok(all_context)
    }
    
    /// Get context source by ID
    pub fn get_source(&self, source_id: &str) -> Option<&dyn ContextSource> {
        self.sources
            .iter()
            .find(|s| s.source_id() == source_id)
            .map(|s| s.as_ref())
    }
    
    /// Validate all source configurations
    pub fn validate_configurations(&self, config: &HashMap<String, HashMap<String, String>>) -> Result<()> {
        for source in &self.sources {
            if let Some(source_config) = config.get(source.source_id()) {
                source.validate_config(source_config)?;
            }
        }
        Ok(())
    }
}

impl Default for ContextSourceManager {
    fn default() -> Self {
        Self::new()
    }
}