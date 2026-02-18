use crate::errors::JasperResult;
use crate::new_daemon_core::SimplifiedDaemonCore;

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use zbus::{interface, Connection, ConnectionBuilder, SignalContext};

/// Simplified D-Bus service for frontend communication
pub struct SimplifiedDbusService {
    daemon: Arc<RwLock<SimplifiedDaemonCore>>,
}

impl SimplifiedDbusService {
    pub fn new(daemon: Arc<RwLock<SimplifiedDaemonCore>>) -> Self {
        Self { daemon }
    }

    /// Start the D-Bus service
    pub async fn start(daemon: Arc<RwLock<SimplifiedDaemonCore>>) -> JasperResult<()> {
        let service = SimplifiedDbusService::new(daemon);

        let _connection = ConnectionBuilder::session()
            .unwrap()
            .name("org.jasper.Daemon")?
            .serve_at("/org/jasper/Daemon", service)?
            .build()
            .await?;

        info!("D-Bus service started at org.jasper.Daemon");

        // Keep the service running
        std::future::pending::<()>().await;
        Ok(())
    }
}

#[interface(name = "org.jasper.Daemon1")]
impl SimplifiedDbusService {
    /// Get the latest insight
    async fn get_latest_insight(&self) -> (i64, String, String, String) {
        match self.daemon.read().await.get_latest_insight() {
            Ok(Some(insight)) => (
                insight.id,
                insight.emoji,
                insight.insight,
                insight.context_hash.unwrap_or_default(),
            ),
            Ok(None) => (
                0,
                "🔍".to_string(),
                "No insights available".to_string(),
                "".to_string(),
            ),
            Err(e) => {
                warn!("Failed to get latest insight: {}", e);
                (
                    0,
                    "⚠️".to_string(),
                    "Error retrieving insights".to_string(),
                    "".to_string(),
                )
            }
        }
    }

    /// Get insight by ID
    async fn get_insight_by_id(&self, insight_id: i64) -> (i64, String, String, String) {
        match self.daemon.read().await.get_insight_by_id(insight_id) {
            Ok(Some(insight)) => (
                insight.id,
                insight.emoji,
                insight.insight,
                insight.context_hash.unwrap_or_default(),
            ),
            Ok(None) => (
                0,
                "❓".to_string(),
                "Insight not found".to_string(),
                "".to_string(),
            ),
            Err(e) => {
                warn!("Failed to get insight by ID {}: {}", insight_id, e);
                (
                    0,
                    "⚠️".to_string(),
                    "Error retrieving insight".to_string(),
                    "".to_string(),
                )
            }
        }
    }

    /// Register a frontend as active
    async fn register_frontend(&self, frontend_id: String, pid: i32) -> bool {
        let pid_option = if pid > 0 { Some(pid) } else { None };

        match self
            .daemon
            .read()
            .await
            .register_frontend(&frontend_id, pid_option)
        {
            Ok(()) => {
                info!(
                    "Frontend registered: {} (PID: {:?})",
                    frontend_id, pid_option
                );
                true
            }
            Err(e) => {
                error!("Failed to register frontend {}: {}", frontend_id, e);
                false
            }
        }
    }

    /// Unregister a frontend
    async fn unregister_frontend(&self, frontend_id: String) -> bool {
        match self.daemon.read().await.unregister_frontend(&frontend_id) {
            Ok(()) => {
                info!("Frontend unregistered: {}", frontend_id);
                true
            }
            Err(e) => {
                error!("Failed to unregister frontend {}: {}", frontend_id, e);
                false
            }
        }
    }

    /// Update frontend heartbeat
    async fn heartbeat(&self, frontend_id: String) -> bool {
        match self
            .daemon
            .read()
            .await
            .update_frontend_heartbeat(&frontend_id)
        {
            Ok(()) => {
                debug!("Heartbeat updated for frontend: {}", frontend_id);
                true
            }
            Err(e) => {
                warn!("Failed to update heartbeat for {}: {}", frontend_id, e);
                false
            }
        }
    }

    /// Force immediate context refresh and analysis
    async fn force_refresh(&self) -> bool {
        match self.daemon.write().await.force_refresh().await {
            Ok(()) => {
                info!("Forced context refresh completed");
                true
            }
            Err(e) => {
                error!("Failed to force refresh: {}", e);
                false
            }
        }
    }

    /// Get daemon status
    async fn get_status(&self) -> (bool, u32, i64) {
        match self.daemon.read().await.get_status().await {
            Ok(status) => (
                status.is_running,
                status.active_frontends as u32,
                status.insights_count,
            ),
            Err(e) => {
                error!("Failed to get daemon status: {}", e);
                (false, 0, 0)
            }
        }
    }

    // TODO: Add signal methods
    // These would be called by the daemon when new insights are available

    /// Signal emitted when a new insight is available
    #[zbus(signal)]
    async fn insight_updated(
        signal_ctxt: &SignalContext<'_>,
        insight_id: i64,
        emoji: String,
        preview: String,
    ) -> zbus::Result<()>;

    /// Signal emitted when daemon is stopping
    #[zbus(signal)]
    async fn daemon_stopping(signal_ctxt: &SignalContext<'_>) -> zbus::Result<()>;
}

/// Helper struct to emit signals from the daemon
pub struct DbusSignalEmitter {
    connection: Connection,
}

impl DbusSignalEmitter {
    pub async fn new() -> JasperResult<Self> {
        let connection = Connection::session().await?;
        Ok(Self { connection })
    }

    /// Emit insight updated signal
    pub async fn emit_insight_updated(
        &self,
        insight_id: i64,
        emoji: &str,
        preview: &str,
    ) -> JasperResult<()> {
        let object_path = "/org/jasper/Daemon";
        let interface_name = "org.jasper.Daemon1";

        self.connection
            .emit_signal(
                None::<&str>,
                object_path,
                interface_name,
                "InsightUpdated",
                &(insight_id, emoji, preview),
            )
            .await?;

        debug!("Emitted InsightUpdated signal for insight {}", insight_id);
        Ok(())
    }

    /// Emit daemon stopping signal (available for graceful shutdown)
    #[allow(dead_code)]
    pub async fn emit_daemon_stopping(&self) -> JasperResult<()> {
        let object_path = "/org/jasper/Daemon";
        let interface_name = "org.jasper.Daemon1";

        self.connection
            .emit_signal(
                None::<&str>,
                object_path,
                interface_name,
                "DaemonStopping",
                &(),
            )
            .await?;

        info!("Emitted DaemonStopping signal");
        Ok(())
    }
}
