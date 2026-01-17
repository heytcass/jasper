use serde_json::{json, Value};
use tracing::{info, error, debug};
use zbus::{Connection, proxy};

/// Waybar adapter that connects to the simplified Jasper daemon via D-Bus
/// This replaces all the complex waybar formatting code in the old architecture

#[proxy(
    interface = "org.jasper.Daemon1",
    default_service = "org.jasper.Daemon",
    default_path = "/org/jasper/Daemon"
)]
trait JasperDaemon {
    async fn get_latest_insight(&self) -> zbus::Result<(i64, String, String, String)>;
    async fn register_frontend(&self, frontend_id: String, pid: i32) -> zbus::Result<bool>;
    async fn unregister_frontend(&self, frontend_id: String) -> zbus::Result<bool>;
    async fn get_status(&self) -> zbus::Result<(bool, u32, i64)>;
    
    // TODO: Add signal subscription for real-time updates
}

pub struct WaybarAdapter {
    proxy: Option<JasperDaemonProxy<'static>>,
}

impl WaybarAdapter {
    pub async fn new() -> Self {
        Self {
            proxy: None,
        }
    }

    /// Connect to the Jasper daemon
    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let connection = Connection::session().await?;
        let proxy = JasperDaemonProxy::new(&connection).await?;
        
        // Register as waybar frontend
        let success = proxy.register_frontend("waybar".to_string(), std::process::id() as i32).await?;
        if !success {
            error!("Failed to register with Jasper daemon");
            return Err("Registration failed".into());
        }
        
        self.proxy = Some(proxy);
        info!("Connected to Jasper daemon");
        Ok(())
    }

    /// Get the current insight formatted for waybar
    pub async fn get_waybar_output(&self) -> Value {
        let Some(ref proxy) = self.proxy else {
            return self.error_output("Not connected to daemon");
        };

        match proxy.get_latest_insight().await {
            Ok((id, emoji, insight, _context_hash)) => {
                if id > 0 {
                    self.format_insight_output(&emoji, &insight)
                } else {
                    self.waiting_output()
                }
            }
            Err(e) => {
                error!("Failed to get insight from daemon: {}", e);
                self.error_output("Daemon error")
            }
        }
    }

    /// Format insight for waybar JSON output
    fn format_insight_output(&self, emoji: &str, insight: &str) -> Value {
        // Truncate long insights for waybar display
        let display_text = if insight.len() > 50 {
            format!("{}...", &insight[..47])
        } else {
            insight.to_string()
        };

        json!({
            "text": format!("{} {}", emoji, display_text),
            "tooltip": insight,
            "class": "jasper-insight",
            "percentage": 100
        })
    }

    /// Output for when daemon is analyzing
    fn waiting_output(&self) -> Value {
        json!({
            "text": "🔍 Analyzing...",
            "tooltip": "Jasper is analyzing your context",
            "class": "jasper-waiting",
            "percentage": 0
        })
    }

    /// Output for error states
    fn error_output(&self, message: &str) -> Value {
        json!({
            "text": "⚠️ Jasper",
            "tooltip": format!("Error: {}", message),
            "class": "jasper-error",
            "percentage": 0
        })
    }

    /// Check daemon status
    pub async fn get_status(&self) -> Result<(bool, u32, i64), Box<dyn std::error::Error>> {
        let Some(ref proxy) = self.proxy else {
            return Err("Not connected to daemon".into());
        };

        let status = proxy.get_status().await?;
        Ok(status)
    }

    /// Disconnect from daemon
    pub async fn disconnect(&mut self) {
        if let Some(ref proxy) = self.proxy {
            if let Err(e) = proxy.unregister_frontend("waybar".to_string()).await {
                error!("Failed to unregister from daemon: {}", e);
            }
        }
        self.proxy = None;
        debug!("Disconnected from Jasper daemon");
    }
}

impl Drop for WaybarAdapter {
    fn drop(&mut self) {
        // Note: This is sync drop, so we can't await the disconnect
        // The daemon will clean up expired frontends automatically
    }
}

/// Main function for waybar integration
/// This replaces the old waybar command in main.rs
pub async fn run_waybar_mode() -> Result<(), Box<dyn std::error::Error>> {
    // Don't initialize logging - it's already initialized by main.rs

    let mut adapter = WaybarAdapter::new().await;

    // Try to connect to daemon
    if let Err(_e) = adapter.connect().await {
        // If daemon is not running, output error state and exit
        let output = adapter.error_output("Daemon not running");
        println!("{}", serde_json::to_string(&output)?);
        return Ok(());
    }

    // Get and output the current insight
    let output = adapter.get_waybar_output().await;
    println!("{}", serde_json::to_string(&output)?);

    // Disconnect cleanly
    adapter.disconnect().await;

    Ok(())
}

/// Simple status check for waybar
pub async fn waybar_status() -> Result<(), Box<dyn std::error::Error>> {
    let mut adapter = WaybarAdapter::new().await;
    
    match adapter.connect().await {
        Ok(()) => {
            match adapter.get_status().await {
                Ok((_is_running, active_frontends, insights_count)) => {
                    println!("Daemon: Running");
                    println!("Active Frontends: {}", active_frontends);
                    println!("Insights Generated: {}", insights_count);
                }
                Err(e) => {
                    println!("Status Error: {}", e);
                }
            }
            adapter.disconnect().await;
        }
        Err(_) => {
            println!("Daemon: Not Running");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_waybar_adapter_creation() {
        let adapter = WaybarAdapter::new().await;
        assert!(adapter.proxy.is_none());
    }

    #[test]
    fn test_output_formatting() {
        let adapter = WaybarAdapter { proxy: None };
        
        // Test insight formatting
        let output = adapter.format_insight_output("🎯", "Short insight");
        assert!(output["text"].as_str().unwrap().contains("🎯"));
        assert!(output["text"].as_str().unwrap().contains("Short insight"));
        
        // Test long insight truncation
        let long_insight = "This is a very long insight that should be truncated for waybar display";
        let output = adapter.format_insight_output("📅", long_insight);
        assert!(output["text"].as_str().unwrap().len() <= 52); // emoji + space + 47 chars + "..."
        assert!(output["tooltip"].as_str().unwrap() == long_insight);
        
        // Test error output
        let output = adapter.error_output("Test error");
        assert!(output["text"].as_str().unwrap().contains("⚠️"));
        assert!(output["tooltip"].as_str().unwrap().contains("Test error"));
    }
}