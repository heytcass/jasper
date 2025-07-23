use anyhow::Result;
use chrono_tz::Tz;

use crate::frontend_framework::{FrontendFormatter, InsightData, JsonFrontendFormatter};

/// Terminal formatter for CLI/debug output with color support
pub struct TerminalFrontendFormatter {
    pub use_colors: bool,
}

impl TerminalFrontendFormatter {
    pub fn new(use_colors: bool) -> Self {
        Self { use_colors }
    }
    
    fn colorize(&self, text: &str, color: &str) -> String {
        if self.use_colors {
            match color {
                "red" => format!("\x1b[31m{}\x1b[0m", text),
                "orange" => format!("\x1b[33m{}\x1b[0m", text),
                "blue" => format!("\x1b[34m{}\x1b[0m", text),
                "gray" => format!("\x1b[90m{}\x1b[0m", text),
                "green" => format!("\x1b[32m{}\x1b[0m", text),
                "bold" => format!("\x1b[1m{}\x1b[0m", text),
                _ => text.to_string(),
            }
        } else {
            text.to_string()
        }
    }
}

impl FrontendFormatter<String> for TerminalFrontendFormatter {
    fn format(&self, insights: &[InsightData], timezone: Tz) -> Result<String> {
        if insights.is_empty() {
            return self.format_empty(timezone);
        }

        let mut output = String::new();
        
        // Header
        output.push_str(&self.colorize("🤖 Jasper Insights", "bold"));
        output.push_str("\n");
        output.push_str(&self.colorize("═══════════════════", "gray"));
        output.push_str("\n\n");

        // Sort by urgency (highest first)
        let mut sorted_insights = insights.to_vec();
        sorted_insights.sort_by(|a, b| b.urgency_score.cmp(&a.urgency_score));

        for (i, insight) in sorted_insights.iter().enumerate() {
            // Urgency indicator and icon
            let urgency_color = match insight.urgency_score {
                9..=10 => "red",
                7..=8 => "orange",
                5..=6 => "blue",
                3..=4 => "gray",
                _ => "gray",
            };
            
            let icon = insight.recommended_icon.as_deref()
                .unwrap_or(insight.category.default_icon());
            
            output.push_str(&format!("{}. {} ", i + 1, icon));
            output.push_str(&self.colorize(&insight.message, urgency_color));
            output.push_str("\n");
            
            // Action if available
            if !insight.action_needed.is_empty() && insight.action_needed != "Review this insight" {
                output.push_str(&format!("   💡 {}\n", insight.action_needed));
            }
            
            // Timestamp
            let local_time = insight.discovered_at.with_timezone(&timezone);
            output.push_str(&self.colorize(
                &format!("   🕐 {}", local_time.format("%Y-%m-%d %I:%M %p")),
                "gray"
            ));
            output.push_str("\n");
            
            if i < sorted_insights.len() - 1 {
                output.push_str("\n");
            }
        }

        Ok(output)
    }

    fn frontend_id(&self) -> &'static str {
        "terminal"
    }

    fn frontend_name(&self) -> &'static str {
        "Terminal/CLI Output"
    }

    fn format_empty(&self, _timezone: Tz) -> Result<String> {
        let mut output = String::new();
        output.push_str(&self.colorize("📅 Jasper", "bold"));
        output.push_str("\n");
        output.push_str(&self.colorize("No urgent insights at this time", "green"));
        output.push_str("\n");
        Ok(output)
    }

    fn format_error(&self, error: &str, _timezone: Tz) -> Result<String> {
        let mut output = String::new();
        output.push_str(&self.colorize("❌ Jasper Error", "red"));
        output.push_str("\n");
        output.push_str(&self.colorize(error, "red"));
        output.push_str("\n");
        Ok(output)
    }
}

impl JsonFrontendFormatter for TerminalFrontendFormatter {
    fn format_json(&self, insights: &[InsightData], timezone: Tz) -> Result<String> {
        let output = self.format(insights, timezone)?;
        // For terminal, we just return the formatted string directly (not JSON-encoded)
        // since it's meant for direct display
        Ok(output)
    }
    
    fn frontend_id(&self) -> &'static str {
        FrontendFormatter::frontend_id(self)
    }
    
    fn frontend_name(&self) -> &'static str {
        FrontendFormatter::frontend_name(self)
    }
    
    fn format_empty_json(&self, timezone: Tz) -> Result<String> {
        self.format_empty(timezone)
    }
    
    fn format_error_json(&self, error: &str, timezone: Tz) -> Result<String> {
        self.format_error(error, timezone)
    }
}

impl Default for TerminalFrontendFormatter {
    fn default() -> Self {
        Self::new(true) // Use colors by default
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend_framework::{UrgencyLevel, InsightCategory, FrontendFormatter};
    use chrono::Utc;

    fn create_test_insight(urgency: i32, message: &str) -> InsightData {
        InsightData {
            id: "test-1".to_string(),
            message: message.to_string(),
            action_needed: "Test action".to_string(),
            urgency_score: urgency,
            urgency_level: UrgencyLevel::from_score(urgency),
            discovered_at: Utc::now(),
            recommended_icon: Some("🧪".to_string()),
            category: InsightCategory::General,
            event_ids: vec![],
        }
    }

    #[test]
    fn test_terminal_formatter_empty() {
        let formatter = TerminalFrontendFormatter::new(false); // No colors for testing
        let result = formatter.format_empty(chrono_tz::UTC).unwrap();
        
        assert!(result.contains("📅 Jasper"));
        assert!(result.contains("No urgent insights"));
    }

    #[test]
    fn test_terminal_formatter_with_insights() {
        let formatter = TerminalFrontendFormatter::new(false);
        let insights = vec![
            create_test_insight(8, "High priority insight"),
            create_test_insight(3, "Low priority insight"),
        ];
        
        let result = formatter.format(&insights, chrono_tz::UTC).unwrap();
        
        assert!(result.contains("🤖 Jasper Insights"));
        assert!(result.contains("High priority insight"));
        assert!(result.contains("Low priority insight"));
        // High priority should come first (sorted by urgency)
        let high_pos = result.find("High priority").unwrap();
        let low_pos = result.find("Low priority").unwrap();
        assert!(high_pos < low_pos);
    }

    #[test]
    fn test_frontend_metadata() {
        let formatter = TerminalFrontendFormatter::new(false);
        assert_eq!(FrontendFormatter::frontend_id(&formatter), "terminal");
        assert_eq!(FrontendFormatter::frontend_name(&formatter), "Terminal/CLI Output");
    }

    #[test]
    fn test_color_support() {
        let formatter_with_colors = TerminalFrontendFormatter::new(true);
        let formatter_no_colors = TerminalFrontendFormatter::new(false);
        
        let colored = formatter_with_colors.colorize("test", "red");
        let plain = formatter_no_colors.colorize("test", "red");
        
        assert!(colored.contains("\x1b[31m")); // ANSI red
        assert_eq!(plain, "test"); // No ANSI codes
    }
}