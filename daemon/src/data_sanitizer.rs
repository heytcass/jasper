use anyhow::Result;
use chrono::{DateTime, Utc, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;
use regex::Regex;

use crate::database::{Event, EnrichedEvent};
use crate::config::Config;
use crate::context_sources::{ContextData, ContextContent, NotesContext, TaskContext, WeatherContext};

// Pre-compiled regex patterns for better performance
static EMAIL_REGEX: OnceLock<Regex> = OnceLock::new();
static PHONE_REGEX: OnceLock<Regex> = OnceLock::new();
static URL_REGEX: OnceLock<Regex> = OnceLock::new();

fn get_email_regex() -> &'static Regex {
    EMAIL_REGEX.get_or_init(|| {
        Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap()
    })
}

fn get_phone_regex() -> &'static Regex {
    PHONE_REGEX.get_or_init(|| {
        Regex::new(r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b").unwrap()
    })
}

fn get_url_regex() -> &'static Regex {
    URL_REGEX.get_or_init(|| {
        Regex::new(r"https?://[^\s]+").unwrap()
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SanitizationLevel {
    Strict,   // Strip all specific details, keep only patterns
    Moderate, // Anonymize names/places but keep event types
    Minimal,  // Keep most details, just remove obvious PII
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizedEvent {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub event_type: String,
    pub calendar_type: String,
    pub duration_minutes: i64,
    pub day_of_week: String,
    pub time_of_day: String, // "morning", "afternoon", "evening"
    pub location: Option<String>, // Preserve location for intelligence
    pub calendar_owner: Option<String>, // e.g., "Tom", "Wife", "Son"
    pub is_all_day: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizedCalendarContext {
    pub events: Vec<SanitizedEvent>,
    pub time_range: String,
    pub total_events: usize,
    pub busiest_day: Option<String>,
    pub pattern_summary: String,
}

#[derive(Clone)]
pub struct DataSanitizer {
    level: SanitizationLevel,
    sensitive_keywords: HashMap<String, String>,
}

impl DataSanitizer {
    pub fn new(level: SanitizationLevel) -> Self {
        let mut sensitive_keywords = HashMap::new();
        
        // Medical/Health
        sensitive_keywords.insert("doctor".to_string(), "medical appointment".to_string());
        sensitive_keywords.insert("dentist".to_string(), "dental appointment".to_string());
        sensitive_keywords.insert("therapy".to_string(), "wellness appointment".to_string());
        sensitive_keywords.insert("counseling".to_string(), "wellness appointment".to_string());
        sensitive_keywords.insert("hospital".to_string(), "medical appointment".to_string());
        
        // Legal/Financial
        sensitive_keywords.insert("lawyer".to_string(), "legal consultation".to_string());
        sensitive_keywords.insert("attorney".to_string(), "legal consultation".to_string());
        sensitive_keywords.insert("divorce".to_string(), "legal matter".to_string());
        sensitive_keywords.insert("court".to_string(), "legal appointment".to_string());
        sensitive_keywords.insert("bank".to_string(), "financial meeting".to_string());
        
        // Personal
        sensitive_keywords.insert("interview".to_string(), "career opportunity".to_string());
        sensitive_keywords.insert("date".to_string(), "social meeting".to_string());
        sensitive_keywords.insert("anniversary".to_string(), "personal celebration".to_string());
        
        Self {
            level,
            sensitive_keywords,
        }
    }
    
    pub fn sanitize_events(&self, events: &[Event]) -> Result<SanitizedCalendarContext> {
        let mut sanitized_events = Vec::with_capacity(events.len());
        for event in events.iter() {
            sanitized_events.push(self.sanitize_event(event));
        }
        
        self.build_context(sanitized_events, events)
    }
    
    pub fn sanitize_enriched_events(&self, enriched_events: &[EnrichedEvent]) -> Result<SanitizedCalendarContext> {
        let mut sanitized_events = Vec::with_capacity(enriched_events.len());
        let mut events = Vec::with_capacity(enriched_events.len());
        
        for enriched in enriched_events.iter() {
            sanitized_events.push(self.sanitize_enriched_event(enriched, None));
            events.push(enriched.event.clone());
        }
        
        self.build_context(sanitized_events, &events)
    }
    
    pub fn sanitize_enriched_events_with_config(&self, enriched_events: &[EnrichedEvent], config: &Config) -> Result<SanitizedCalendarContext> {
        let mut sanitized_events = Vec::with_capacity(enriched_events.len());
        let mut events = Vec::with_capacity(enriched_events.len());
        
        for enriched in enriched_events.iter() {
            sanitized_events.push(self.sanitize_enriched_event(enriched, Some(config)));
            events.push(enriched.event.clone());
        }
        
        self.build_context(sanitized_events, &events)
    }
    
    fn build_context(&self, sanitized_events: Vec<SanitizedEvent>, events: &[Event]) -> Result<SanitizedCalendarContext> {
        let pattern_summary = self.generate_pattern_summary(&sanitized_events);
        let busiest_day = self.find_busiest_day(&sanitized_events);
        
        let start_time = events.iter()
            .map(|e| DateTime::from_timestamp(e.start_time, 0).unwrap_or_default())
            .min()
            .unwrap_or_else(Utc::now);
        
        let end_time = events.iter()
            .map(|e| DateTime::from_timestamp(e.end_time.unwrap_or(e.start_time), 0).unwrap_or_default())
            .max()
            .unwrap_or_else(Utc::now);
        
        Ok(SanitizedCalendarContext {
            events: sanitized_events,
            time_range: format!("{} to {}", 
                start_time.format("%Y-%m-%d"), 
                end_time.format("%Y-%m-%d")),
            total_events: events.len(),
            busiest_day,
            pattern_summary,
        })
    }
    
    fn sanitize_event(&self, event: &Event) -> SanitizedEvent {
        let start_dt = DateTime::from_timestamp(event.start_time, 0).unwrap_or_default();
        let end_dt = event.end_time
            .map(|ts| DateTime::from_timestamp(ts, 0).unwrap_or_default());
        
        let duration = if let Some(end) = end_dt {
            (end - start_dt).num_minutes()
        } else {
            60 // Default 1 hour
        };
        
        let (sanitized_title, sanitized_desc) = match self.level {
            SanitizationLevel::Strict => self.strict_sanitize(event),
            SanitizationLevel::Moderate => self.moderate_sanitize(event),
            SanitizationLevel::Minimal => self.minimal_sanitize(event),
        };
        
        SanitizedEvent {
            id: format!("event_{}", event.id),
            title: sanitized_title,
            description: sanitized_desc,
            start_time: start_dt,
            end_time: end_dt,
            event_type: event.event_type.clone().unwrap_or_else(|| "general".to_string()),
            calendar_type: "personal".to_string(), // TODO: Get from calendar relationship
            duration_minutes: duration,
            day_of_week: start_dt.format("%A").to_string(),
            time_of_day: self.classify_time_of_day(&start_dt),
            location: event.location.clone(),
            calendar_owner: None, // Basic events don't have calendar context
            is_all_day: event.is_all_day.unwrap_or(false),
        }
    }
    
    fn sanitize_enriched_event(&self, enriched: &EnrichedEvent, config: Option<&Config>) -> SanitizedEvent {
        let event = &enriched.event;
        let start_dt = DateTime::from_timestamp(event.start_time, 0).unwrap_or_default();
        let end_dt = event.end_time
            .map(|ts| DateTime::from_timestamp(ts, 0).unwrap_or_default());
        
        let duration = if let Some(end) = end_dt {
            (end - start_dt).num_minutes()
        } else {
            60 // Default 1 hour
        };
        
        let (sanitized_title, sanitized_desc) = match self.level {
            SanitizationLevel::Strict => self.strict_sanitize(event),
            SanitizationLevel::Moderate => self.moderate_sanitize(event),
            SanitizationLevel::Minimal => self.minimal_sanitize(event),
        };
        
        // Extract calendar owner using config mappings if available
        let calendar_owner = enriched.calendar_info.as_ref()
            .map(|cal| self.extract_calendar_owner_with_config(&cal.calendar_id, config));
        
        // Preserve location for intelligence, but sanitize if needed
        let location = match self.level {
            SanitizationLevel::Strict => None,
            SanitizationLevel::Moderate => event.location.as_ref().map(|l| self.sanitize_location(l)),
            SanitizationLevel::Minimal => event.location.clone(),
        };
        
        SanitizedEvent {
            id: format!("event_{}", event.id),
            title: sanitized_title,
            description: sanitized_desc,
            start_time: start_dt,
            end_time: end_dt,
            event_type: event.event_type.clone().unwrap_or_else(|| "general".to_string()),
            calendar_type: enriched.calendar_info.as_ref()
                .and_then(|cal| cal.calendar_type.clone())
                .unwrap_or_else(|| "personal".to_string()),
            duration_minutes: duration,
            day_of_week: start_dt.format("%A").to_string(),
            time_of_day: self.classify_time_of_day(&start_dt),
            location,
            calendar_owner,
            is_all_day: event.is_all_day.unwrap_or(false),
        }
    }
    
    fn strict_sanitize(&self, event: &Event) -> (String, Option<String>) {
        let title = event.title.as_deref().unwrap_or("Event");
        
        // Categorize by keywords but strip specifics
        for (keyword, replacement) in &self.sensitive_keywords {
            if title.to_lowercase().contains(keyword) {
                return (replacement.clone(), None);
            }
        }
        
        // Generic categorization
        let generic_title = if title.to_lowercase().contains("meeting") {
            "Meeting"
        } else if title.to_lowercase().contains("call") {
            "Phone call"
        } else if title.to_lowercase().contains("appointment") {
            "Appointment"
        } else if title.to_lowercase().contains("travel") || title.to_lowercase().contains("vacation") {
            "Travel event"
        } else if title.to_lowercase().contains("maintenance") || title.to_lowercase().contains("repair") {
            "Maintenance task"
        } else {
            "Scheduled event"
        };
        
        (generic_title.to_string(), None)
    }
    
    fn moderate_sanitize(&self, event: &Event) -> (String, Option<String>) {
        let title = event.title.as_deref().unwrap_or("Event");
        
        // Replace sensitive keywords but keep event structure (optimized)
        let mut sanitized = title.to_lowercase();
        let mut found_replacements = false;
        
        for (keyword, replacement) in &self.sensitive_keywords {
            if sanitized.contains(keyword) {
                sanitized = sanitized.replace(keyword, replacement);
                found_replacements = true;
            }
        }
        
        // If no replacements were made, preserve original casing
        if !found_replacements {
            sanitized = title.to_string();
        }
        
        // Remove obvious PII patterns (names, emails, phone numbers)
        sanitized = self.remove_pii_patterns(&sanitized);
        
        let desc = event.description.as_ref().map(|d| self.remove_pii_patterns(d));
        
        (sanitized, desc)
    }
    
    fn minimal_sanitize(&self, event: &Event) -> (String, Option<String>) {
        let title = event.title.as_deref().unwrap_or("Event");
        let sanitized_title = self.remove_pii_patterns(title);
        let sanitized_desc = event.description.as_ref().map(|d| self.remove_pii_patterns(d));
        
        (sanitized_title, sanitized_desc)
    }
    
    fn remove_pii_patterns(&self, text: &str) -> String {
        // Chain regex replacements to minimize string allocations
        let after_emails = get_email_regex().replace_all(text, "[email]");
        let after_phones = get_phone_regex().replace_all(&after_emails, "[phone]");
        let after_urls = get_url_regex().replace_all(&after_phones, "[link]");
        
        after_urls.to_string()
    }
    
    fn classify_time_of_day(&self, dt: &DateTime<Utc>) -> String {
        let hour = dt.hour();
        match hour {
            5..=11 => "morning".to_string(),
            12..=17 => "afternoon".to_string(),
            18..=22 => "evening".to_string(),
            _ => "night".to_string(),
        }
    }
    
    fn generate_pattern_summary(&self, events: &[SanitizedEvent]) -> String {
        if events.is_empty() {
            return "No events scheduled".to_string();
        }
        
        let mut morning = 0;
        let mut afternoon = 0;
        let mut evening = 0;
        let mut meeting_count = 0;
        let mut travel_count = 0;
        
        for event in events {
            match event.time_of_day.as_str() {
                "morning" => morning += 1,
                "afternoon" => afternoon += 1,
                "evening" => evening += 1,
                _ => {}
            }
            
            if event.title.to_lowercase().contains("meeting") {
                meeting_count += 1;
            }
            if event.title.to_lowercase().contains("travel") {
                travel_count += 1;
            }
        }
        
        format!("Schedule includes {} events: {} morning, {} afternoon, {} evening. {} meetings, {} travel events.",
            events.len(), morning, afternoon, evening, meeting_count, travel_count)
    }
    
    fn find_busiest_day(&self, events: &[SanitizedEvent]) -> Option<String> {
        if events.is_empty() {
            return None;
        }
        
        let mut day_counts: HashMap<String, usize> = HashMap::new();
        
        for event in events {
            let date = event.start_time.format("%Y-%m-%d").to_string();
            *day_counts.entry(date).or_insert(0) += 1;
        }
        
        day_counts.into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(date, count)| format!("{} ({} events)", date, count))
    }
    
    pub fn extract_calendar_owner_with_config(&self, calendar_name: &str, config: Option<&Config>) -> String {
        // First try to use config mappings if available
        if let Some(config) = config {
            if let Some(owner) = config.get_calendar_owner(calendar_name) {
                return owner;
            }
        }
        
        // Fallback to the original string matching approach
        self.extract_calendar_owner(calendar_name)
    }
    
    fn extract_calendar_owner(&self, calendar_name: &str) -> String {
        // Extract meaningful owner names from calendar names
        let lower_name = calendar_name.to_lowercase();
        
        if lower_name.contains("family") || lower_name.contains("shared") {
            "Family".to_string()
        } else if lower_name.contains("wife") || lower_name.contains("spouse") {
            "Wife".to_string()
        } else if lower_name.contains("son") || lower_name.contains("child") {
            "Son".to_string()
        } else if lower_name.contains("daughter") {
            "Daughter".to_string()
        } else if lower_name.contains("work") || lower_name.contains("office") {
            "Work".to_string()
        } else {
            // Enhanced name extraction from email addresses and calendar names
            self.extract_name_from_identifier(calendar_name)
        }
    }
    
    /// Extract a human-readable name from calendar identifier (email, etc.)
    fn extract_name_from_identifier(&self, identifier: &str) -> String {
        // Handle email addresses
        if let Some(local_part) = identifier.split('@').next() {
            // Handle common email patterns like first.last, firstname.lastname, etc.
            if local_part.contains('.') {
                let parts: Vec<&str> = local_part.split('.').collect();
                if parts.len() >= 2 {
                    // Capitalize first and last names
                    let first_name = self.capitalize_name(parts[0]);
                    let last_name = self.capitalize_name(parts[1]);
                    return format!("{} {}", first_name, last_name);
                }
            }
            
            // Handle patterns with numbers or underscores
            let clean_name = local_part
                .replace('_', " ")
                .replace('-', " ")
                .split_whitespace()
                .map(|word| self.capitalize_name(word))
                .collect::<Vec<_>>()
                .join(" ");
                
            if !clean_name.is_empty() {
                return clean_name;
            }
        }
        
        // Fallback: just capitalize the identifier
        self.capitalize_name(identifier)
    }
    
    /// Capitalize a name appropriately
    fn capitalize_name(&self, name: &str) -> String {
        if name.is_empty() {
            return name.to_string();
        }
        
        // Remove numbers and special characters, keep only letters
        let clean: String = name.chars()
            .filter(|c| c.is_alphabetic())
            .collect();
            
        if clean.is_empty() {
            return "Person".to_string();
        }
        
        // Capitalize first letter, lowercase the rest
        let mut chars = clean.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        }
    }
    
    fn sanitize_location(&self, location: &str) -> String {
        // Remove sensitive location details but keep general area
        let mut sanitized = location.to_string();
        
        // Remove specific addresses but keep general areas
        sanitized = regex::Regex::new(r"\d+\s+[A-Za-z0-9\s]+(?:Street|St|Avenue|Ave|Road|Rd|Lane|Ln|Drive|Dr|Boulevard|Blvd)")
            .unwrap()
            .replace_all(&sanitized, "[Address]")
            .to_string();
        
        // Keep business names and general areas
        sanitized
    }
    
    /// Sanitize context data from various sources
    pub fn sanitize_context_data(&self, context_data: &ContextData) -> Result<ContextData> {
        let mut sanitized = context_data.clone();
        
        match &context_data.content {
            ContextContent::Notes(notes_context) => {
                sanitized.content = ContextContent::Notes(self.sanitize_notes_context(notes_context)?);
            }
            ContextContent::Tasks(task_context) => {
                sanitized.content = ContextContent::Tasks(self.sanitize_task_context(task_context)?);
            }
            ContextContent::Weather(weather_context) => {
                sanitized.content = ContextContent::Weather(self.sanitize_weather_context(weather_context)?);
            }
            ContextContent::Calendar(_) => {
                // Calendar context is already handled by existing methods
            }
            ContextContent::Generic(_) => {
                // Generic context passed through for now
            }
        }
        
        Ok(sanitized)
    }
    
    /// Sanitize notes context (Obsidian data)
    fn sanitize_notes_context(&self, notes_context: &NotesContext) -> Result<NotesContext> {
        let mut sanitized = notes_context.clone();
        
        // Sanitize daily notes
        for daily_note in &mut sanitized.daily_notes {
            daily_note.content = self.remove_pii_patterns(&daily_note.content);
            daily_note.title = self.remove_pii_patterns(&daily_note.title);
            
            // Sanitize tasks within daily notes
            for task in &mut daily_note.tasks {
                task.title = self.remove_pii_patterns(&task.title);
                if let Some(ref desc) = task.description {
                    task.description = Some(self.remove_pii_patterns(desc));
                }
            }
        }
        
        // Sanitize projects
        for project in &mut sanitized.active_projects {
            project.name = self.remove_pii_patterns(&project.name);
            if let Some(ref desc) = project.description {
                project.description = Some(self.remove_pii_patterns(desc));
            }
            
            // Sanitize project tasks
            for task in &mut project.tasks {
                task.title = self.remove_pii_patterns(&task.title);
                if let Some(ref desc) = task.description {
                    task.description = Some(self.remove_pii_patterns(desc));
                }
            }
        }
        
        // Sanitize activities
        for activity in &mut sanitized.recent_activities {
            activity.title = self.remove_pii_patterns(&activity.title);
            if let Some(ref desc) = activity.description {
                activity.description = Some(self.remove_pii_patterns(desc));
            }
        }
        
        // Sanitize pending tasks
        for task in &mut sanitized.pending_tasks {
            task.title = self.remove_pii_patterns(&task.title);
            if let Some(ref desc) = task.description {
                task.description = Some(self.remove_pii_patterns(desc));
            }
        }
        
        // Relationship alerts - keep structure but sanitize sensitive info
        for alert in &mut sanitized.relationship_alerts {
            match self.level {
                SanitizationLevel::Strict => {
                    alert.person_name = format!("Contact {}", alert.urgency);
                    alert.company = None;
                }
                SanitizationLevel::Moderate => {
                    alert.person_name = self.remove_pii_patterns(&alert.person_name);
                    if let Some(ref company) = alert.company {
                        alert.company = Some(self.remove_pii_patterns(company));
                    }
                }
                SanitizationLevel::Minimal => {
                    // Keep most information
                }
            }
        }
        
        Ok(sanitized)
    }
    
    /// Sanitize task context
    fn sanitize_task_context(&self, task_context: &TaskContext) -> Result<TaskContext> {
        let mut sanitized = task_context.clone();
        
        // Sanitize task titles and descriptions
        for task in &mut sanitized.tasks {
            task.title = self.remove_pii_patterns(&task.title);
            if let Some(ref desc) = task.description {
                task.description = Some(self.remove_pii_patterns(desc));
            }
        }
        
        Ok(sanitized)
    }
    
    /// Sanitize weather context
    fn sanitize_weather_context(&self, weather_context: &WeatherContext) -> Result<WeatherContext> {
        // Weather data is generally not sensitive, but we might want to
        // generalize location information in strict mode
        let mut sanitized = weather_context.clone();
        
        if matches!(self.level, SanitizationLevel::Strict) {
            // Replace specific location with general area
            sanitized.current_conditions = regex::Regex::new(r"\b[A-Z][a-z]+,\s*[A-Z]{2}\b")
                .unwrap()
                .replace_all(&sanitized.current_conditions, "[Location]")
                .to_string();
        }
        
        Ok(sanitized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_strict_sanitization() {
        let sanitizer = DataSanitizer::new(SanitizationLevel::Strict);
        
        let event = Event {
            id: 1,
            source_id: "test".to_string(),
            calendar_id: 1,
            title: Some("Doctor appointment with Dr. Smith".to_string()),
            description: Some("Annual checkup at downtown clinic".to_string()),
            start_time: Utc::now().timestamp(),
            end_time: None,
            location: Some("Downtown clinic".to_string()),
            event_type: Some("medical".to_string()),
            participants: Some("[]".to_string()),
            raw_data_json: Some("{}".to_string()),
            is_all_day: Some(false),
        };
        
        let (title, _) = sanitizer.strict_sanitize(&event);
        assert_eq!(title, "medical appointment");
    }
    
    #[test]
    fn test_pii_removal() {
        let sanitizer = DataSanitizer::new(SanitizationLevel::Minimal);
        
        let text = "Meeting with john.doe@company.com at 555-123-4567";
        let result = sanitizer.remove_pii_patterns(text);
        assert_eq!(result, "Meeting with [email] at [phone]");
    }
}