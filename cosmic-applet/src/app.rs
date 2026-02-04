use crate::config::{JasperAppletConfig, APP_ID};
use crate::dbus_client;

use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::window::Id;
use cosmic::iced::{Limits, Subscription};
use cosmic::iced_winit::commands::popup::{destroy_popup, get_popup};
use cosmic::prelude::*;
use cosmic::widget;
use futures_util::SinkExt;
use tracing::{debug, error, info, warn};

const FRONTEND_ID: &str = "cosmic-applet";

#[derive(Default)]
pub struct JasperApplet {
    core: cosmic::Core,
    popup: Option<Id>,
    config: JasperAppletConfig,
    current_emoji: String,
    current_text: String,
    insight_id: i64,
    daemon_online: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    InsightReceived(i64, String, String),
    DaemonOffline,
    ForceRefresh,
    RefreshComplete(bool),
    PollTick,
    HeartbeatTick,
    DbusConnected,
    DbusConnectionFailed,
    UpdateConfig(JasperAppletConfig),
    ConfigChannel,
}

impl cosmic::Application for JasperApplet {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let config = cosmic_config::Config::new(Self::APP_ID, JasperAppletConfig::VERSION)
            .map(|context| match JasperAppletConfig::get_entry(&context) {
                Ok(config) => config,
                Err((_errors, config)) => config,
            })
            .unwrap_or_default();

        let app = JasperApplet {
            core,
            config,
            current_emoji: "\u{1f4c5}".to_string(), // calendar emoji
            current_text: "Connecting...".to_string(),
            ..Default::default()
        };

        // Spawn initial D-Bus connection + fetch
        let init_task = Task::perform(
            async {
                match connect_and_register().await {
                    Ok((_emoji, _text, _id)) => true,
                    Err(_) => false,
                }
            },
            |success| {
                cosmic::Action::App(if success {
                    Message::DbusConnected
                } else {
                    Message::DbusConnectionFailed
                })
            },
        );

        // Also do a poll immediately to get actual data
        let poll_task = Task::perform(fetch_insight(), |result| {
            cosmic::Action::App(match result {
                Some((id, emoji, text)) => Message::InsightReceived(id, emoji, text),
                None => Message::DaemonOffline,
            })
        });

        (app, Task::batch([init_task, poll_task]))
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Self::Message> {
        if self.config.show_text_in_panel {
            let max = self.config.panel_text_max_chars as usize;
            let display_text = if self.current_text.len() > max {
                format!("{}...", &self.current_text[..max.saturating_sub(3)])
            } else {
                self.current_text.clone()
            };
            let label = format!("{} {}", self.current_emoji, display_text);
            self.core
                .applet
                .text_button(label)
                .on_press(Message::TogglePopup)
                .into()
        } else {
            // Emoji-only panel button
            self.core
                .applet
                .text_button(self.current_emoji.clone())
                .on_press(Message::TogglePopup)
                .into()
        }
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let status_text = if self.daemon_online {
            &self.current_text
        } else {
            "Daemon offline"
        };

        let content = widget::list_column()
            .padding(10)
            .spacing(8)
            .add(widget::text::title4(format!(
                "{} Jasper Insight",
                self.current_emoji
            )))
            .add(widget::text::body(status_text))
            .add(
                widget::button::text("Refresh")
                    .on_press(Message::ForceRefresh),
            );

        self.core.applet.popup_container(content).into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let poll_secs = self.config.poll_interval_secs.max(1) as u64;

        struct PollSub;
        struct HeartbeatSub;

        Subscription::batch(vec![
            // Poll for new insights
            Subscription::run_with_id(
                std::any::TypeId::of::<PollSub>(),
                cosmic::iced::stream::channel(4, move |mut channel| async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(poll_secs)).await;
                        let _ = channel.send(Message::PollTick).await;
                    }
                }),
            ),
            // Heartbeat every 5 seconds
            Subscription::run_with_id(
                std::any::TypeId::of::<HeartbeatSub>(),
                cosmic::iced::stream::channel(4, move |mut channel| async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        let _ = channel.send(Message::HeartbeatTick).await;
                    }
                }),
            ),
            // Watch for config changes
            self.core()
                .watch_config::<JasperAppletConfig>(Self::APP_ID)
                .map(|update| Message::UpdateConfig(update.config)),
        ])
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(400.0)
                        .min_width(300.0)
                        .min_height(100.0)
                        .max_height(600.0);
                    get_popup(popup_settings)
                };
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::InsightReceived(id, emoji, text) => {
                self.insight_id = id;
                self.current_emoji = emoji;
                self.current_text = text;
                self.daemon_online = true;
                debug!("Insight updated: id={}", id);
            }
            Message::DaemonOffline => {
                self.daemon_online = false;
                self.current_emoji = "\u{1f4c5}".to_string();
                self.current_text = "Daemon offline".to_string();
            }
            Message::ForceRefresh => {
                return Task::perform(
                    async {
                        match dbus_client::connect().await {
                            Ok(proxy) => proxy.force_refresh().await.unwrap_or(false),
                            Err(_) => false,
                        }
                    },
                    |success| cosmic::Action::App(Message::RefreshComplete(success)),
                );
            }
            Message::RefreshComplete(success) => {
                if success {
                    info!("Force refresh completed");
                    // Fetch the new insight immediately
                    return Task::perform(fetch_insight(), |result| {
                        cosmic::Action::App(match result {
                            Some((id, emoji, text)) => Message::InsightReceived(id, emoji, text),
                            None => Message::DaemonOffline,
                        })
                    });
                } else {
                    warn!("Force refresh failed");
                }
            }
            Message::PollTick => {
                return Task::perform(fetch_insight(), |result| {
                    cosmic::Action::App(match result {
                        Some((id, emoji, text)) => Message::InsightReceived(id, emoji, text),
                        None => Message::DaemonOffline,
                    })
                });
            }
            Message::HeartbeatTick => {
                return Task::perform(
                    async {
                        if let Ok(proxy) = dbus_client::connect().await {
                            let _ = proxy
                                .heartbeat(FRONTEND_ID.to_string())
                                .await;
                        }
                    },
                    |_| cosmic::Action::App(Message::ConfigChannel),
                );
            }
            Message::DbusConnected => {
                self.daemon_online = true;
                info!("Connected to Jasper daemon over D-Bus");
            }
            Message::DbusConnectionFailed => {
                self.daemon_online = false;
                self.current_emoji = "\u{1f4c5}".to_string();
                self.current_text = "Daemon offline".to_string();
                warn!("Failed to connect to Jasper daemon");
            }
            Message::UpdateConfig(config) => {
                self.config = config;
            }
            Message::ConfigChannel => {}
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}

async fn connect_and_register() -> Result<(String, String, i64), Box<dyn std::error::Error + Send + Sync>> {
    let proxy = dbus_client::connect().await?;
    let pid = std::process::id() as i32;
    proxy.register_frontend(FRONTEND_ID.to_string(), pid).await?;

    let (id, emoji, text, _hash) = proxy.get_latest_insight().await?;
    Ok((emoji, text, id))
}

async fn fetch_insight() -> Option<(i64, String, String)> {
    let proxy = dbus_client::connect().await.ok()?;
    let (id, emoji, text, _hash) = proxy.get_latest_insight().await.ok()?;
    if id > 0 {
        Some((id, emoji, text))
    } else {
        Some((0, "\u{1f50d}".to_string(), "Analyzing...".to_string()))
    }
}
