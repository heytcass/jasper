use zbus::{Connection, proxy};

#[proxy(
    interface = "org.jasper.Daemon1",
    default_service = "org.jasper.Daemon",
    default_path = "/org/jasper/Daemon"
)]
trait JasperDaemon {
    async fn get_latest_insight(&self) -> zbus::Result<(i64, String, String, String)>;
    async fn register_frontend(&self, frontend_id: String, pid: i32) -> zbus::Result<bool>;
    async fn unregister_frontend(&self, frontend_id: String) -> zbus::Result<bool>;
    async fn heartbeat(&self, frontend_id: String) -> zbus::Result<bool>;
    async fn force_refresh(&self) -> zbus::Result<bool>;
    async fn get_status(&self) -> zbus::Result<(bool, u32, i64)>;

    #[zbus(signal)]
    async fn insight_updated(
        &self,
        insight_id: i64,
        emoji: String,
        preview: String,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn daemon_stopping(&self) -> zbus::Result<()>;
}

pub async fn connect() -> zbus::Result<JasperDaemonProxy<'static>> {
    let connection = Connection::session().await?;
    JasperDaemonProxy::new(&connection).await
}
