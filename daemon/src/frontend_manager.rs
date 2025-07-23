use anyhow::Result;
use chrono_tz::Tz;

use crate::frontend_framework::{FrontendRegistry, InsightData};
use crate::formatters::{WaybarFrontendFormatter, TerminalFrontendFormatter};

/// Frontend manager that initializes and manages all available frontend formatters
pub struct FrontendManager {
    registry: FrontendRegistry,
}

impl FrontendManager {
    pub fn new() -> Self {
        let mut registry = FrontendRegistry::new();
        
        // Register built-in formatters
        registry.register(WaybarFrontendFormatter::new());
        registry.register(TerminalFrontendFormatter::new(true)); // With colors
        
        Self { registry }
    }
    
    /// Get formatted output for a specific frontend
    pub fn format(&self, frontend_id: &str, insights: &[InsightData], timezone: Tz) -> Result<String> {
        self.registry.format(frontend_id, insights, timezone)
    }
    
    /// Get formatted empty state for a specific frontend
    pub fn format_empty(&self, frontend_id: &str, timezone: Tz) -> Result<String> {
        self.registry.format_empty(frontend_id, timezone)
    }
    
    /// Get formatted error state for a specific frontend
    pub fn format_error(&self, frontend_id: &str, error: &str, timezone: Tz) -> Result<String> {
        self.registry.format_error(frontend_id, error, timezone)
    }
    
    /// List all available frontends
    pub fn list_frontends(&self) -> Vec<(String, String)> {
        self.registry.list_frontends()
    }
    
    /// Check if a frontend is available
    pub fn has_frontend(&self, frontend_id: &str) -> bool {
        self.registry.has_frontend(frontend_id)
    }
    
    /// Get the underlying registry (for advanced use cases)
    pub fn registry(&self) -> &FrontendRegistry {
        &self.registry
    }
    
    /// Get mutable access to registry (for adding custom formatters)
    pub fn registry_mut(&mut self) -> &mut FrontendRegistry {
        &mut self.registry
    }
}

impl Default for FrontendManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend_framework::{UrgencyLevel, InsightCategory};
    use chrono::Utc;

    fn create_test_insight() -> InsightData {
        InsightData {
            id: "test-1".to_string(),
            message: "Test insight message".to_string(),
            action_needed: "Test action".to_string(),
            urgency_score: 5,
            urgency_level: UrgencyLevel::Info,
            discovered_at: Utc::now(),
            recommended_icon: Some("🧪".to_string()),
            category: InsightCategory::General,
            event_ids: vec![1, 2],
        }
    }

    #[test]
    fn test_frontend_manager_initialization() {
        let manager = FrontendManager::new();
        
        // Should have both waybar and terminal formatters
        assert!(manager.has_frontend("waybar"));
        assert!(manager.has_frontend("terminal"));
        assert!(!manager.has_frontend("nonexistent"));
        
        let frontends = manager.list_frontends();
        assert_eq!(frontends.len(), 2);
        
        // Check that both frontends are listed
        let ids: Vec<&str> = frontends.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"waybar"));
        assert!(ids.contains(&"terminal"));
    }

    #[test]
    fn test_waybar_formatting() {
        let manager = FrontendManager::new();
        let insights = vec![create_test_insight()];
        
        let result = manager.format("waybar", &insights, chrono_tz::UTC).unwrap();
        
        // Should be valid JSON
        let json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(json.get("text").is_some());
        assert!(json.get("tooltip").is_some());
        assert!(json.get("class").is_some());
    }

    #[test]
    fn test_terminal_formatting() {
        let manager = FrontendManager::new();
        let insights = vec![create_test_insight()];
        
        let result = manager.format("terminal", &insights, chrono_tz::UTC).unwrap();
        
        // Should contain terminal-specific formatting
        assert!(result.contains("🤖 Jasper Insights"));
        assert!(result.contains("Test insight message"));
        assert!(result.contains("🧪")); // Test icon
    }

    #[test]
    fn test_empty_state_formatting() {
        let manager = FrontendManager::new();
        
        let waybar_empty = manager.format_empty("waybar", chrono_tz::UTC).unwrap();
        let terminal_empty = manager.format_empty("terminal", chrono_tz::UTC).unwrap();
        
        // Waybar should return JSON
        assert!(serde_json::from_str::<serde_json::Value>(&waybar_empty).is_ok());
        
        // Terminal should return plain text
        assert!(terminal_empty.contains("📅 Jasper"));
        assert!(terminal_empty.contains("No urgent insights"));
    }

    #[test]
    fn test_error_formatting() {
        let manager = FrontendManager::new();
        let error_msg = "Test error message";
        
        let waybar_error = manager.format_error("waybar", error_msg, chrono_tz::UTC).unwrap();
        let terminal_error = manager.format_error("terminal", error_msg, chrono_tz::UTC).unwrap();
        
        // Both should contain the error message
        assert!(waybar_error.contains(error_msg));
        assert!(terminal_error.contains(error_msg));
        
        // Waybar should be JSON
        assert!(serde_json::from_str::<serde_json::Value>(&waybar_error).is_ok());
        
        // Terminal should contain error formatting
        assert!(terminal_error.contains("❌ Jasper Error"));
    }

    #[test]
    fn test_unknown_frontend() {
        let manager = FrontendManager::new();
        let insights = vec![create_test_insight()];
        
        let result = manager.format("unknown", &insights, chrono_tz::UTC);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown frontend"));
    }
}