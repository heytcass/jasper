#![allow(dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;

/// Standardized intermediate format for insights that can be transformed to any frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsightData {
    pub id: String,
    pub message: String,
    pub action_needed: String,
    pub urgency_score: i32,
    pub urgency_level: UrgencyLevel,
    pub discovered_at: DateTime<Utc>,
    pub recommended_icon: Option<String>,
    pub category: InsightCategory,
    pub event_ids: Vec<i64>,
}

/// Standardized urgency levels for consistent theming across frontends
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UrgencyLevel {
    Minimal,  // 0-2
    Low,      // 3-4  
    Info,     // 5-6
    Warning,  // 7-8
    Critical, // 9-10
}

/// Categories of insights for frontend-specific handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InsightCategory {
    Calendar,
    Travel,
    Maintenance,
    Overcommitment,
    Pattern,
    General,
}

/// Generic trait for transforming insights to frontend-specific formats
pub trait FrontendFormatter<T> {
    /// Transform insights into frontend-specific format
    fn format(&self, insights: &[InsightData], timezone: Tz) -> Result<T>;
    
    /// Get the identifier for this frontend (e.g., "waybar", "gnome", "kde")
    fn frontend_id(&self) -> &'static str;
    
    /// Get human-readable name for this frontend
    fn frontend_name(&self) -> &'static str;
    
    /// Format for empty/no insights state
    fn format_empty(&self, timezone: Tz) -> Result<T>;
    
    /// Format for error states (optional override)
    fn format_error(&self, _error: &str, timezone: Tz) -> Result<T> {
        // Default implementation - frontends can override
        self.format_empty(timezone)
    }
}

/// Simplified trait for frontend formatters that output JSON strings
pub trait JsonFrontendFormatter: Send + Sync {
    fn format_json(&self, insights: &[InsightData], timezone: Tz) -> Result<String>;
    fn frontend_id(&self) -> &'static str;
    fn frontend_name(&self) -> &'static str;
    fn format_empty_json(&self, timezone: Tz) -> Result<String>;
    fn format_error_json(&self, error: &str, timezone: Tz) -> Result<String>;
}

// Note: Each formatter needs to implement JsonFrontendFormatter directly
// to avoid Rust's orphan rule and trait coherence issues

/// Registry for managing multiple frontend formatters
pub struct FrontendRegistry {
    formatters: std::collections::HashMap<String, Box<dyn JsonFrontendFormatter>>,
}

impl FrontendRegistry {
    pub fn new() -> Self {
        Self {
            formatters: std::collections::HashMap::new(),
        }
    }
    
    /// Register a new frontend formatter
    pub fn register<F>(&mut self, formatter: F) 
    where 
        F: JsonFrontendFormatter + 'static,
    {
        let id = formatter.frontend_id().to_string();
        self.formatters.insert(id, Box::new(formatter));
    }
    
    /// Get formatted output for a specific frontend
    pub fn format(&self, frontend_id: &str, insights: &[InsightData], timezone: Tz) -> Result<String> {
        match self.formatters.get(frontend_id) {
            Some(formatter) => formatter.format_json(insights, timezone),
            None => Err(anyhow::anyhow!("Unknown frontend: {}", frontend_id))
        }
    }
    
    /// Get formatted empty state for a specific frontend  
    pub fn format_empty(&self, frontend_id: &str, timezone: Tz) -> Result<String> {
        match self.formatters.get(frontend_id) {
            Some(formatter) => formatter.format_empty_json(timezone),
            None => Err(anyhow::anyhow!("Unknown frontend: {}", frontend_id))
        }
    }
    
    /// Get formatted error state for a specific frontend
    pub fn format_error(&self, frontend_id: &str, error: &str, timezone: Tz) -> Result<String> {
        match self.formatters.get(frontend_id) {
            Some(formatter) => formatter.format_error_json(error, timezone),
            None => Err(anyhow::anyhow!("Unknown frontend: {}", frontend_id))
        }
    }
    
    /// List all registered frontends
    pub fn list_frontends(&self) -> Vec<(String, String)> {
        self.formatters.iter()
            .map(|(id, formatter)| (id.clone(), formatter.frontend_name().to_string()))
            .collect()
    }
    
    /// Check if a frontend is registered
    pub fn has_frontend(&self, frontend_id: &str) -> bool {
        self.formatters.contains_key(frontend_id)
    }
}

impl Default for FrontendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper functions for converting from existing data structures
impl InsightData {
    /// Convert from existing Correlation struct
    pub fn from_correlation(correlation: &crate::database::Correlation) -> Self {
        Self {
            id: correlation.id.clone(),
            message: correlation.insight.clone(),
            action_needed: correlation.action_needed.clone(),
            urgency_score: correlation.urgency_score,
            urgency_level: UrgencyLevel::from_score(correlation.urgency_score),
            discovered_at: correlation.discovered_at,
            recommended_icon: correlation.recommended_glyph.clone(),
            category: InsightCategory::infer_from_content(&correlation.insight),
            event_ids: correlation.event_ids.clone(),
        }
    }
}

impl UrgencyLevel {
    pub fn from_score(score: i32) -> Self {
        match score {
            9..=10 => UrgencyLevel::Critical,
            7..=8 => UrgencyLevel::Warning,
            5..=6 => UrgencyLevel::Info,
            3..=4 => UrgencyLevel::Low,
            _ => UrgencyLevel::Minimal,
        }
    }
    
    pub fn css_class(&self) -> &'static str {
        match self {
            UrgencyLevel::Critical => "critical",
            UrgencyLevel::Warning => "warning", 
            UrgencyLevel::Info => "info",
            UrgencyLevel::Low => "low",
            UrgencyLevel::Minimal => "minimal",
        }
    }
    
    pub fn color_code(&self) -> &'static str {
        match self {
            UrgencyLevel::Critical => "#ff4444",  // Red
            UrgencyLevel::Warning => "#ffaa00",   // Orange
            UrgencyLevel::Info => "#4488ff",      // Blue  
            UrgencyLevel::Low => "#888888",       // Gray
            UrgencyLevel::Minimal => "#cccccc",   // Light gray
        }
    }
}

impl InsightCategory {
    pub fn infer_from_content(content: &str) -> Self {
        let content_lower = content.to_lowercase();
        
        if content_lower.contains("travel") || content_lower.contains("flight") || content_lower.contains("trip") {
            InsightCategory::Travel
        } else if content_lower.contains("maintenance") || content_lower.contains("repair") {
            InsightCategory::Maintenance  
        } else if content_lower.contains("overbooked") || content_lower.contains("conflict") || content_lower.contains("overlap") {
            InsightCategory::Overcommitment
        } else if content_lower.contains("pattern") || content_lower.contains("usually") || content_lower.contains("typically") {
            InsightCategory::Pattern
        } else if content_lower.contains("calendar") || content_lower.contains("meeting") || content_lower.contains("event") {
            InsightCategory::Calendar
        } else {
            InsightCategory::General
        }
    }
    
    pub fn default_icon(&self) -> &'static str {
        match self {
            InsightCategory::Calendar => "📅",
            InsightCategory::Travel => "✈️", 
            InsightCategory::Maintenance => "🔧",
            InsightCategory::Overcommitment => "⚠️",
            InsightCategory::Pattern => "📊",
            InsightCategory::General => "💡",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_urgency_level_from_score() {
        assert_eq!(UrgencyLevel::from_score(10).css_class(), "critical");
        assert_eq!(UrgencyLevel::from_score(8).css_class(), "warning");
        assert_eq!(UrgencyLevel::from_score(5).css_class(), "info");
        assert_eq!(UrgencyLevel::from_score(3).css_class(), "low");
        assert_eq!(UrgencyLevel::from_score(0).css_class(), "minimal");
    }
    
    #[test]
    fn test_insight_category_inference() {
        assert!(matches!(
            InsightCategory::infer_from_content("Your flight departs at 5 PM"),
            InsightCategory::Travel
        ));
        
        assert!(matches!(
            InsightCategory::infer_from_content("Scheduling conflict detected"),
            InsightCategory::Overcommitment
        ));
        
        assert!(matches!(
            InsightCategory::infer_from_content("Calendar meeting tomorrow"), 
            InsightCategory::Calendar
        ));
    }
    
    #[test]
    fn test_frontend_registry() {
        let registry = FrontendRegistry::new();
        
        // Test that we can create a registry
        assert_eq!(registry.list_frontends().len(), 0);
        assert!(!registry.has_frontend("test"));
    }
}