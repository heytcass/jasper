use serde_json::{json, Value};
use tracing::{debug, error, info};
use zbus::{proxy, Connection};

/// Noctalia adapter — connects to Jasper daemon via D-Bus and outputs
/// JSON with separate fields for the Noctalia bar widget plugin.
///
/// Unlike the Waybar adapter, this does NOT unregister on exit.
/// The "noctalia" frontend stays registered so the daemon knows a
/// display is active between polls. The daemon auto-expires frontends
/// after 60 s of missed heartbeats.

#[proxy(
    interface = "org.jasper.Daemon1",
    default_service = "org.jasper.Daemon",
    default_path = "/org/jasper/Daemon"
)]
trait JasperDaemon {
    async fn get_latest_insight(&self) -> zbus::Result<(i64, String, String, String)>;
    async fn register_frontend(&self, frontend_id: String, pid: i32) -> zbus::Result<bool>;
    async fn heartbeat(&self, frontend_id: String) -> zbus::Result<bool>;
    async fn force_refresh(&self) -> zbus::Result<bool>;
    async fn get_status(&self) -> zbus::Result<(bool, u32, i64)>;
}

pub struct NoctaliaAdapter {
    proxy: Option<JasperDaemonProxy<'static>>,
}

impl NoctaliaAdapter {
    pub async fn new() -> Self {
        Self { proxy: None }
    }

    /// Connect to daemon, register (idempotent) and send heartbeat.
    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let connection = Connection::session().await?;
        let proxy = JasperDaemonProxy::new(&connection).await?;

        // Register — the daemon treats duplicate registrations as no-ops
        let pid = std::process::id() as i32;
        let ok = proxy.register_frontend("noctalia".into(), pid).await?;
        if !ok {
            error!("Failed to register noctalia frontend");
            return Err("Registration failed".into());
        }

        // Heartbeat keeps the frontend alive between polls
        let _ = proxy.heartbeat("noctalia".into()).await;

        self.proxy = Some(proxy);
        debug!("Noctalia adapter connected");
        Ok(())
    }

    /// Get latest insight as Noctalia-friendly JSON.
    pub async fn get_output(&self) -> Value {
        let Some(ref proxy) = self.proxy else {
            return Self::make_output(0, "", "", "error");
        };

        match proxy.get_latest_insight().await {
            Ok((id, emoji, insight, _hash)) => {
                if id > 0 {
                    Self::make_output(id, &emoji, &insight, "active")
                } else {
                    Self::make_output(0, &emoji, &insight, "waiting")
                }
            }
            Err(e) => {
                error!("D-Bus GetLatestInsight failed: {}", e);
                Self::make_output(0, "", "", "error")
            }
        }
    }

    /// Trigger a forced refresh, then return the (possibly new) insight.
    pub async fn refresh_and_get(&self) -> Value {
        let Some(ref proxy) = self.proxy else {
            return Self::make_output(0, "", "", "error");
        };

        match proxy.force_refresh().await {
            Ok(true) => info!("Force refresh completed"),
            Ok(false) => error!("Force refresh returned false"),
            Err(e) => error!("Force refresh failed: {}", e),
        }

        self.get_output().await
    }

    fn make_output(id: i64, emoji: &str, insight: &str, state: &str) -> Value {
        json!({
            "id": id,
            "emoji": emoji,
            "insight": insight,
            "state": state
        })
    }
}

// ── Public entry points called from main.rs ────────────────────────

/// `jasper-companion-daemon noctalia`
pub async fn run_noctalia_mode() -> Result<(), Box<dyn std::error::Error>> {
    let mut adapter = NoctaliaAdapter::new().await;

    let output = match adapter.connect().await {
        Ok(()) => adapter.get_output().await,
        Err(_) => NoctaliaAdapter::make_output(0, "", "", "offline"),
    };

    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

/// `jasper-companion-daemon noctalia-refresh`
pub async fn run_noctalia_refresh() -> Result<(), Box<dyn std::error::Error>> {
    let mut adapter = NoctaliaAdapter::new().await;

    let output = match adapter.connect().await {
        Ok(()) => adapter.refresh_and_get().await,
        Err(_) => NoctaliaAdapter::make_output(0, "", "", "offline"),
    };

    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_adapter_creation() {
        let adapter = NoctaliaAdapter::new().await;
        assert!(adapter.proxy.is_none());
    }

    #[test]
    fn test_make_output() {
        let out = NoctaliaAdapter::make_output(42, "🎯", "Big meeting at 3 PM", "active");
        assert_eq!(out["id"], 42);
        assert_eq!(out["emoji"], "🎯");
        assert_eq!(out["insight"], "Big meeting at 3 PM");
        assert_eq!(out["state"], "active");
    }

    #[test]
    fn test_offline_output() {
        let out = NoctaliaAdapter::make_output(0, "", "", "offline");
        assert_eq!(out["state"], "offline");
        assert_eq!(out["id"], 0);
    }
}
