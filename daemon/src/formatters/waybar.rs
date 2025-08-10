use anyhow::Result;
use serde::{Deserialize, Serialize};
use chrono_tz::Tz;

use crate::frontend_framework::{FrontendFormatter, InsightData, JsonFrontendFormatter};

/// Waybar-specific output format
#[derive(Debug, Serialize, Deserialize)]
pub struct WaybarOutput {
    pub text: String,
    pub tooltip: Option<String>,
    pub alt: Option<String>,
    pub class: Option<String>,
}

/// Waybar formatter implementing the new FrontendFormatter trait
pub struct WaybarFrontendFormatter;

impl FrontendFormatter<WaybarOutput> for WaybarFrontendFormatter {
    fn format(&self, insights: &[InsightData], timezone: Tz) -> Result<WaybarOutput> {
        if insights.is_empty() {
            return self.format_empty(timezone);
        }

        // Take the highest urgency insight (same logic as before)
        let primary_insight = insights.iter()
            .max_by_key(|i| i.urgency_score)
            .unwrap();

        // Use AI-recommended icon or fallback to category default
        let icon = self.get_display_icon(primary_insight);
        
        // Set class based on urgency level (using new standardized levels)
        let class = primary_insight.urgency_level.css_class();
        
        // Show only the icon - minimal visual footprint
        let text = icon;

        // Create tooltip for single insight
        let tooltip = self.create_tooltip(primary_insight, timezone);

        Ok(WaybarOutput {
            text,
            tooltip: Some(tooltip),
            alt: Some("insight".to_string()),
            class: Some(class.to_string()),
        })
    }

    fn frontend_id(&self) -> &'static str {
        "waybar"
    }

    fn frontend_name(&self) -> &'static str {
        "Waybar Status Bar"
    }

    fn format_empty(&self, _timezone: Tz) -> Result<WaybarOutput> {
        Ok(WaybarOutput {
            text: "󰃭".to_string(),
            tooltip: Some("No urgent calendar insights at this time".to_string()),
            alt: Some("clear".to_string()),
            class: Some("clear".to_string()),
        })
    }

    fn format_error(&self, error: &str, _timezone: Tz) -> Result<WaybarOutput> {
        Ok(WaybarOutput {
            text: "📅".to_string(),
            tooltip: Some(format!("Jasper error: {}", error)),
            alt: Some("error".to_string()),
            class: Some("error".to_string()),
        })
    }
}

impl WaybarFrontendFormatter {
    pub fn new() -> Self {
        Self
    }

    fn get_display_icon(&self, insight: &InsightData) -> String {
        // Try AI-recommended icon first
        if let Some(ref icon) = insight.recommended_icon {
            if !icon.is_empty() && self.is_valid_glyph(icon) {
                return icon.clone();
            }
        }
        
        // Fallback to category-based icon
        insight.category.default_icon().to_string()
    }
    
    fn is_valid_glyph(&self, glyph: &str) -> bool {
        // Validate glyph is proper Unicode and not empty
        // Allow multi-character emojis (e.g., 🌧️ which is rain + variation selector)
        !glyph.is_empty() && glyph.chars().count() <= 3
    }

    fn create_tooltip(&self, insight: &InsightData, timezone: Tz) -> String {
        let mut tooltip = String::new();
        
        // Add header
        tooltip.push_str("🤖 Jasper Insight\n");
        tooltip.push_str("═══════════════════\n\n");

        // Add the insight with icon
        let icon = self.get_display_icon(insight);
        tooltip.push_str(&format!("{} {}\n", icon, insight.message));
        
        // Add action if available and meaningful
        if !insight.action_needed.is_empty() && insight.action_needed != "Review this insight" {
            tooltip.push_str(&format!("\n💡 Tip: {}\n", insight.action_needed));
        }

        // Convert UTC time to local timezone
        let local_time = insight.discovered_at.with_timezone(&timezone);
        tooltip.push_str(&format!("\n🕐 Generated: {}", local_time.format("%I:%M %p")));
        
        tooltip
    }

    /// Legacy method for backwards compatibility with existing code
    pub fn format_correlations(&self, correlations: &[crate::database::Correlation], timezone: Tz) -> Result<WaybarOutput> {
        // Convert correlations to InsightData
        let insights: Vec<InsightData> = correlations.iter()
            .map(InsightData::from_correlation)
            .collect();
        
        self.format(&insights, timezone)
    }

    /// Simple text format for terminal/debugging
    pub fn format_simple(&self, insights: &[InsightData]) -> String {
        if insights.is_empty() {
            return "󰃭 All clear".to_string();
        }

        let primary_insight = insights.iter()
            .max_by_key(|i| i.urgency_score)
            .unwrap();

        let icon = self.get_display_icon(primary_insight);
        format!("{} {}", icon, self.truncate_text(&primary_insight.message, 80))
    }

    fn truncate_text(&self, text: &str, max_len: usize) -> String {
        if text.chars().count() <= max_len {
            text.to_string()
        } else {
            let mut result = String::new();
            let mut char_count = 0;
            for c in text.chars() {
                if char_count >= max_len.saturating_sub(1) {
                    break;
                }
                result.push(c);
                char_count += 1;
            }
            result.push('…');
            result
        }
    }
}

impl Default for WaybarFrontendFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonFrontendFormatter for WaybarFrontendFormatter {
    fn format_json(&self, insights: &[InsightData], timezone: Tz) -> Result<String> {
        let output = self.format(insights, timezone)?;
        Ok(serde_json::to_string(&output)?)
    }
    
    fn frontend_id(&self) -> &'static str {
        FrontendFormatter::frontend_id(self)
    }
    
    fn frontend_name(&self) -> &'static str {
        FrontendFormatter::frontend_name(self)
    }
    
    fn format_empty_json(&self, timezone: Tz) -> Result<String> {
        let output = self.format_empty(timezone)?;
        Ok(serde_json::to_string(&output)?)
    }
    
    fn format_error_json(&self, error: &str, timezone: Tz) -> Result<String> {
        let output = self.format_error(error, timezone)?;
        Ok(serde_json::to_string(&output)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend_framework::{UrgencyLevel, InsightCategory, FrontendFormatter};
    use chrono::Utc;

    fn create_test_insight() -> InsightData {
        InsightData {
            id: "test-1".to_string(),
            message: "You have a scheduling conflict between two meetings".to_string(),
            action_needed: "Reschedule one of the meetings".to_string(),
            urgency_score: 8,
            urgency_level: UrgencyLevel::Warning,
            discovered_at: Utc::now(),
            recommended_icon: Some("🚨".to_string()),
            category: InsightCategory::Calendar,
            event_ids: vec![1, 2],
        }
    }

    #[test]
    fn test_waybar_formatter_empty() {
        let formatter = WaybarFrontendFormatter::new();
        let result = formatter.format_empty(chrono_tz::UTC).unwrap();
        
        assert_eq!(result.text, "󰃭");
        assert_eq!(result.class, Some("clear".to_string()));
    }

    #[test]
    fn test_waybar_formatter_with_insight() {
        let formatter = WaybarFrontendFormatter::new();
        let insights = vec![create_test_insight()];
        
        let result = formatter.format(&insights, chrono_tz::UTC).unwrap();
        
        assert!(result.text.contains("🚨"));
        assert_eq!(result.class, Some("warning".to_string()));
        assert!(result.tooltip.is_some());
    }

    #[test]
    fn test_waybar_formatter_error() {
        let formatter = WaybarFrontendFormatter::new();
        let result = formatter.format_error("Test error", chrono_tz::UTC).unwrap();
        
        assert_eq!(result.text, "📅");
        assert_eq!(result.class, Some("error".to_string()));
        assert!(result.tooltip.unwrap().contains("Test error"));
    }

    #[test]
    fn test_frontend_metadata() {
        let formatter = WaybarFrontendFormatter::new();
        assert_eq!(FrontendFormatter::frontend_id(&formatter), "waybar");
        assert_eq!(FrontendFormatter::frontend_name(&formatter), "Waybar Status Bar");
    }

    #[test]
    fn test_text_truncation() {
        let formatter = WaybarFrontendFormatter::new();
        let long_text = "This is a very long insight that should definitely be truncated because it's too long for the status bar";
        
        let truncated = formatter.truncate_text(long_text, 50);
        assert!(truncated.chars().count() <= 50);
        assert!(truncated.ends_with('…'));
    }
}