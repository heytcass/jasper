use anyhow::Result;
use serde::{Deserialize, Serialize};
use chrono_tz::Tz;
use crate::database::Correlation;

#[derive(Debug, Serialize, Deserialize)]
pub struct WaybarOutput {
    pub text: String,
    pub tooltip: Option<String>,
    pub alt: Option<String>,
    pub class: Option<String>,
}

pub struct WaybarFormatter {
    timezone: Tz,
}

impl WaybarFormatter {
    pub fn new(timezone: Tz) -> Self {
        Self { timezone }
    }

    pub fn format_correlations(&self, correlations: &[Correlation]) -> Result<WaybarOutput> {
        if correlations.is_empty() {
            return Ok(WaybarOutput {
                text: "󰃭".to_string(),
                tooltip: Some("No urgent calendar insights at this time".to_string()),
                alt: Some("clear".to_string()),
                class: Some("clear".to_string()),
            });
        }

        // For single insight approach, we should typically have just one correlation
        // but we'll take the first one (or highest priority if multiple)
        let primary_correlation = correlations.iter()
            .max_by_key(|c| c.urgency_score)
            .unwrap();

        // Use AI-recommended glyph or fallback to default
        let icon = self.get_display_icon(primary_correlation);
        
        // Set class based on urgency score
        let class = match primary_correlation.urgency_score {
            9..=10 => "critical",
            7..=8 => "warning", 
            5..=6 => "info",
            3..=4 => "low",
            _ => "minimal",
        };
        
        // Show only the icon - minimal visual footprint
        let text = icon;

        // Create tooltip for single insight
        let tooltip = self.create_single_insight_tooltip(primary_correlation);

        Ok(WaybarOutput {
            text,
            tooltip: Some(tooltip),
            alt: Some("insight".to_string()),
            class: Some(class.to_string()),
        })
    }

    fn get_display_icon(&self, correlation: &Correlation) -> String {
        // Try AI-recommended glyph first
        if let Some(glyph) = &correlation.recommended_glyph {
            if !glyph.is_empty() && self.is_valid_glyph(glyph) {
                return glyph.clone();
            }
        }
        
        // Fallback to default calendar icon
        "󰃭".to_string()
    }
    
    fn is_valid_glyph(&self, glyph: &str) -> bool {
        // Validate glyph is proper Unicode and not empty
        !glyph.is_empty() && glyph.chars().count() == 1
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

    fn create_single_insight_tooltip(&self, correlation: &Correlation) -> String {
        let mut tooltip = String::new();
        
        // Add header
        tooltip.push_str("🤖 Jasper Insight\n");
        tooltip.push_str("═══════════════════\n\n");

        // Add the single insight
        let icon = self.get_display_icon(correlation);
        tooltip.push_str(&format!("{} {}\n", icon, correlation.insight));
        
        // Add action if available
        if !correlation.action_needed.is_empty() && correlation.action_needed != "Review this insight" {
            tooltip.push_str(&format!("\n💡 Tip: {}\n", correlation.action_needed));
        }

        // Convert UTC time to local timezone
        let local_time = correlation.discovered_at.with_timezone(&self.timezone);
        tooltip.push_str(&format!("\n🕐 Generated: {}", local_time.format("%I:%M %p")));
        tooltip
    }

    /// Format for single-line output (useful for simple status display)
    pub fn format_simple(&self, correlations: &[Correlation]) -> String {
        if correlations.is_empty() {
            return "󰃭 All clear".to_string();
        }

        let primary_correlation = correlations.iter()
            .max_by_key(|c| c.urgency_score)
            .unwrap();

        let icon = self.get_display_icon(primary_correlation);
        format!("{} {}", icon, self.truncate_text(&primary_correlation.insight, 80))
    }
}

impl Default for WaybarFormatter {
    fn default() -> Self {
        Self::new(chrono_tz::UTC)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_empty_correlations() {
        let formatter = WaybarFormatter::new(chrono_tz::UTC);
        let result = formatter.format_correlations(&[]).unwrap();
        
        assert_eq!(result.text, "󰃭");
        assert_eq!(result.class, Some("clear".to_string()));
    }

    #[test]
    fn test_high_urgency_formatting() {
        let formatter = WaybarFormatter::new(chrono_tz::UTC);
        let correlations = vec![
            Correlation {
                id: "1".to_string(),
                event_ids: vec![1, 2],
                insight: "You have a scheduling conflict between two important meetings".to_string(),
                action_needed: "Reschedule one of the meetings".to_string(),
                urgency_score: 9,
                discovered_at: Utc::now(),
                recommended_glyph: Some("🚨".to_string()),
            }
        ];

        let result = formatter.format_correlations(&correlations).unwrap();
        
        assert!(result.text.contains("🚨"));
        assert_eq!(result.class, Some("critical".to_string()));
    }

    #[test]
    fn test_text_truncation() {
        let formatter = WaybarFormatter::new(chrono_tz::UTC);
        let long_text = "This is a very long insight that should definitely be truncated because it's too long for the status bar";
        
        let truncated = formatter.truncate_text(long_text, 50);
        assert!(truncated.chars().count() <= 50);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn test_tooltip_formatting() {
        let formatter = WaybarFormatter::new(chrono_tz::UTC);
        let correlations = vec![
            Correlation {
                id: "1".to_string(),
                event_ids: vec![1],
                insight: "First insight".to_string(),
                action_needed: "First action".to_string(),
                urgency_score: 8,
                discovered_at: Utc::now(),
                recommended_glyph: None,
            },
            Correlation {
                id: "2".to_string(),
                event_ids: vec![2],
                insight: "Second insight".to_string(),
                action_needed: "Second action".to_string(),
                urgency_score: 6,
                discovered_at: Utc::now(),
                recommended_glyph: None,
            },
        ];

        let result = formatter.format_correlations(&correlations).unwrap();
        let tooltip = result.tooltip.unwrap();
        
        assert!(tooltip.contains("🤖 Jasper Insight"));
        // Only the highest urgency insight (First insight with urgency 8) should be shown
        assert!(tooltip.contains("First insight"));
        assert!(!tooltip.contains("Second insight")); // Second insight should not be shown
        assert!(!tooltip.contains("Urgency:")); // No urgency display in tooltip
    }
}