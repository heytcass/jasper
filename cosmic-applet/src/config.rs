use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};

pub const APP_ID: &str = "com.system76.CosmicAppletJasper";

#[derive(Debug, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct JasperAppletConfig {
    /// Show insight text alongside emoji in the panel button
    pub show_text_in_panel: bool,
    /// Maximum characters of insight text to show in the panel
    pub panel_text_max_chars: u32,
    /// How often to poll the daemon for new insights (seconds)
    pub poll_interval_secs: u32,
}

impl Default for JasperAppletConfig {
    fn default() -> Self {
        Self {
            show_text_in_panel: false,
            panel_text_max_chars: 30,
            poll_interval_secs: 10,
        }
    }
}
