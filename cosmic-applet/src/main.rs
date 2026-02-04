mod app;
mod config;
mod dbus_client;

fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    cosmic::applet::run::<app::JasperApplet>(())
}
