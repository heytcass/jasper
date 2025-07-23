use anyhow::Result;
use serde::{Deserialize, Serialize};
use chrono_tz::Tz;

use crate::frontend_framework::{FrontendFormatter, InsightData, JsonFrontendFormatter};

/// GNOME Shell panel indicator data format
#[derive(Debug, Serialize, Deserialize)]
pub struct GnomeIndicatorData {
    /// Text to display in the panel (usually just an icon)
    pub text: String,
    /// Tooltip text shown on hover
    pub tooltip: String,
    /// CSS class for styling (maps to urgency levels)
    pub style_class: String,
    /// Whether the indicator should be visible
    pub visible: bool,
    /// Detailed insight data for popup/menu
    pub insights: Vec<GnomeInsightItem>,
}

/// Individual insight item for GNOME Shell popup menu
#[derive(Debug, Serialize, Deserialize)]
pub struct GnomeInsightItem {
    pub id: String,
    pub icon: String,
    pub title: String,
    pub description: String,
    pub urgency: String,
    pub timestamp: String,
    pub action_text: Option<String>,
}

/// GNOME Shell formatter implementing the FrontendFormatter trait
pub struct GnomeFrontendFormatter {
    pub show_in_panel: bool,
}

impl GnomeFrontendFormatter {
    pub fn new(show_in_panel: bool) -> Self {
        Self { show_in_panel }
    }
    
    /// Convert urgency level to GNOME Shell CSS class
    fn urgency_to_css_class(&self, urgency_score: i32) -> String {
        match urgency_score {
            9..=10 => "jasper-critical".to_string(),
            7..=8 => "jasper-warning".to_string(),
            5..=6 => "jasper-info".to_string(),
            3..=4 => "jasper-low".to_string(),
            _ => "jasper-minimal".to_string(),
        }
    }
    
    /// Get the primary display icon based on insights
    fn get_panel_icon(&self, insights: &[InsightData]) -> String {
        if insights.is_empty() {
            return "📅".to_string();
        }
        
        // Use the highest urgency insight's icon
        let primary_insight = insights.iter()
            .max_by_key(|i| i.urgency_score)
            .unwrap();
            
        primary_insight.recommended_icon
            .as_deref()
            .unwrap_or(primary_insight.category.default_icon())
            .to_string()
    }
    
    /// Format a single insight for the GNOME popup menu
    fn format_insight_item(&self, insight: &InsightData, timezone: Tz) -> GnomeInsightItem {
        let local_time = insight.discovered_at.with_timezone(&timezone);
        let icon = insight.recommended_icon
            .as_deref()
            .unwrap_or(insight.category.default_icon())
            .to_string();
            
        GnomeInsightItem {
            id: insight.id.clone(),
            icon,
            title: insight.message.clone(),
            description: self.format_description(insight),
            urgency: insight.urgency_level.css_class().to_string(),
            timestamp: local_time.format("%I:%M %p").to_string(),
            action_text: if insight.action_needed.is_empty() || insight.action_needed == "Review this insight" {
                None
            } else {
                Some(insight.action_needed.clone())
            },
        }
    }
    
    /// Create a description combining category and event information
    fn format_description(&self, insight: &InsightData) -> String {
        let mut parts = Vec::new();
        
        // Add category context
        match insight.category {
            crate::frontend_framework::InsightCategory::Calendar => {
                if !insight.event_ids.is_empty() {
                    parts.push(format!("Affects {} event(s)", insight.event_ids.len()));
                }
            },
            crate::frontend_framework::InsightCategory::Travel => {
                parts.push("Travel planning".to_string());
            },
            crate::frontend_framework::InsightCategory::Overcommitment => {
                parts.push("Schedule conflict".to_string());
            },
            _ => {},
        }
        
        if parts.is_empty() {
            "Jasper AI insight".to_string()
        } else {
            parts.join(" • ")
        }
    }
}

impl FrontendFormatter<GnomeIndicatorData> for GnomeFrontendFormatter {
    fn format(&self, insights: &[InsightData], timezone: Tz) -> Result<GnomeIndicatorData> {
        if insights.is_empty() {
            return self.format_empty(timezone);
        }

        // Sort insights by urgency for consistent ordering
        let mut sorted_insights = insights.to_vec();
        sorted_insights.sort_by(|a, b| b.urgency_score.cmp(&a.urgency_score));
        
        let primary_insight = &sorted_insights[0];
        let panel_icon = self.get_panel_icon(&sorted_insights);
        let style_class = self.urgency_to_css_class(primary_insight.urgency_score);
        
        // Convert all insights to GNOME format
        let gnome_insights: Vec<GnomeInsightItem> = sorted_insights.iter()
            .map(|insight| self.format_insight_item(insight, timezone))
            .collect();
        
        // Create tooltip summary
        let tooltip = if sorted_insights.len() == 1 {
            format!("Jasper: {}", primary_insight.message)
        } else {
            format!("Jasper: {} insights (most urgent: {})", 
                   sorted_insights.len(), 
                   primary_insight.message)
        };

        Ok(GnomeIndicatorData {
            text: panel_icon,
            tooltip,
            style_class,
            visible: self.show_in_panel,
            insights: gnome_insights,
        })
    }

    fn frontend_id(&self) -> &'static str {
        "gnome"
    }

    fn frontend_name(&self) -> &'static str {
        "GNOME Shell Extension"
    }

    fn format_empty(&self, _timezone: Tz) -> Result<GnomeIndicatorData> {
        Ok(GnomeIndicatorData {
            text: "📅".to_string(),
            tooltip: "Jasper: No urgent insights at this time".to_string(),
            style_class: "jasper-clear".to_string(),
            visible: self.show_in_panel,
            insights: Vec::new(),
        })
    }

    fn format_error(&self, error: &str, _timezone: Tz) -> Result<GnomeIndicatorData> {
        Ok(GnomeIndicatorData {
            text: "⚠️".to_string(),
            tooltip: format!("Jasper error: {}", error),
            style_class: "jasper-error".to_string(),
            visible: self.show_in_panel,
            insights: Vec::new(),
        })
    }
}

impl JsonFrontendFormatter for GnomeFrontendFormatter {
    fn format_json(&self, insights: &[InsightData], timezone: Tz) -> Result<String> {
        let output = self.format(insights, timezone)?;
        Ok(serde_json::to_string_pretty(&output)?)
    }
    
    fn frontend_id(&self) -> &'static str {
        FrontendFormatter::frontend_id(self)
    }
    
    fn frontend_name(&self) -> &'static str {
        FrontendFormatter::frontend_name(self)
    }
    
    fn format_empty_json(&self, timezone: Tz) -> Result<String> {
        let output = self.format_empty(timezone)?;
        Ok(serde_json::to_string_pretty(&output)?)
    }
    
    fn format_error_json(&self, error: &str, timezone: Tz) -> Result<String> {
        let output = self.format_error(error, timezone)?;
        Ok(serde_json::to_string_pretty(&output)?)
    }
}

impl Default for GnomeFrontendFormatter {
    fn default() -> Self {
        Self::new(true) // Show in panel by default
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend_framework::{UrgencyLevel, InsightCategory, FrontendFormatter};
    use chrono::Utc;

    fn create_test_insight(urgency: i32, message: &str) -> InsightData {
        InsightData {
            id: "test-gnome-1".to_string(),
            message: message.to_string(),
            action_needed: "Review the scheduling conflict".to_string(),
            urgency_score: urgency,
            urgency_level: UrgencyLevel::from_score(urgency),
            discovered_at: Utc::now(),
            recommended_icon: Some("🚨".to_string()),
            category: InsightCategory::Calendar,
            event_ids: vec![1, 2],
        }
    }

    #[test]
    fn test_gnome_formatter_empty() {
        let formatter = GnomeFrontendFormatter::new(true);
        let result = formatter.format_empty(chrono_tz::UTC).unwrap();
        
        assert_eq!(result.text, "📅");
        assert_eq!(result.style_class, "jasper-clear");
        assert!(result.visible);
        assert!(result.insights.is_empty());
        assert!(result.tooltip.contains("No urgent insights"));
    }

    #[test]
    fn test_gnome_formatter_with_insights() {
        let formatter = GnomeFrontendFormatter::new(true);
        let insights = vec![
            create_test_insight(8, "High priority meeting conflict"),
            create_test_insight(4, "Low priority reminder"),
        ];
        
        let result = formatter.format(&insights, chrono_tz::UTC).unwrap();
        
        assert_eq!(result.text, "🚨"); // Uses the AI-recommended icon
        assert_eq!(result.style_class, "jasper-warning"); // Urgency 8 maps to warning
        assert!(result.visible);
        assert_eq!(result.insights.len(), 2);
        
        // Check that insights are sorted by urgency (high first)
        assert_eq!(result.insights[0].urgency, "warning");
        assert_eq!(result.insights[1].urgency, "low");
        assert!(result.tooltip.contains("High priority meeting conflict"));
    }

    #[test]
    fn test_gnome_formatter_error_handling() {
        let formatter = GnomeFrontendFormatter::new(false);
        let result = formatter.format_error("Test error message", chrono_tz::UTC).unwrap();
        
        assert_eq!(result.text, "⚠️");
        assert_eq!(result.style_class, "jasper-error");
        assert!(!result.visible); // Configured to not show in panel
        assert!(result.tooltip.contains("Test error message"));
    }

    #[test]
    fn test_gnome_frontend_metadata() {
        let formatter = GnomeFrontendFormatter::new(true);
        assert_eq!(FrontendFormatter::frontend_id(&formatter), "gnome");
        assert_eq!(FrontendFormatter::frontend_name(&formatter), "GNOME Shell Extension");
    }

    #[test]
    fn test_urgency_css_mapping() {
        let formatter = GnomeFrontendFormatter::new(true);
        
        assert_eq!(formatter.urgency_to_css_class(10), "jasper-critical");
        assert_eq!(formatter.urgency_to_css_class(8), "jasper-warning");
        assert_eq!(formatter.urgency_to_css_class(5), "jasper-info");
        assert_eq!(formatter.urgency_to_css_class(3), "jasper-low");
        assert_eq!(formatter.urgency_to_css_class(1), "jasper-minimal");
    }

    #[test]
    fn test_gnome_insight_formatting() {
        let formatter = GnomeFrontendFormatter::new(true);
        let insight = create_test_insight(7, "Meeting overlap detected");
        
        let gnome_item = formatter.format_insight_item(&insight, chrono_tz::UTC);
        
        assert_eq!(gnome_item.id, "test-gnome-1");
        assert_eq!(gnome_item.icon, "🚨");
        assert_eq!(gnome_item.title, "Meeting overlap detected");
        assert_eq!(gnome_item.urgency, "warning");
        assert!(gnome_item.action_text.is_some());
        assert_eq!(gnome_item.action_text.unwrap(), "Review the scheduling conflict");
    }

    #[test]
    fn test_json_serialization() {
        let formatter = GnomeFrontendFormatter::new(true);
        let insights = vec![create_test_insight(6, "Calendar insight")];
        
        let json = formatter.format_json(&insights, chrono_tz::UTC).unwrap();
        
        // Verify it's valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("text").is_some());
        assert!(parsed.get("tooltip").is_some());
        assert!(parsed.get("insights").is_some());
        assert!(parsed.get("style_class").is_some());
    }
}