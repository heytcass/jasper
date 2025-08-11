use anyhow::Result;
use std::process::Command;
use tracing::{debug, warn};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use parking_lot::RwLock;
use notify_rust::{Notification, Hint, Urgency};

/// Notification types for different insight events
#[derive(Debug, Clone)]
pub enum NotificationType {
    NewInsight { message: String, icon: Option<String> },
    ContextChanged { changes: String },
    CacheRefreshed,
    AnalysisComplete { insights_count: usize },
}

/// Configuration for notification behavior
#[derive(Debug, Clone)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub notify_new_insights: bool,
    pub notify_context_changes: bool,
    pub notify_cache_refresh: bool,
    pub notification_timeout: u32, // milliseconds
    pub min_urgency_threshold: i32, // minimum urgency score to notify
}

impl Default for NotificationConfig {
    fn default() -> Self {
        // Check if daemon notifications should be disabled (e.g., when GNOME extension handles them)
        let daemon_notifications_disabled = std::env::var("JASPER_DISABLE_DAEMON_NOTIFICATIONS")
            .map(|v| v == "true")
            .unwrap_or(false);
            
        Self {
            enabled: !daemon_notifications_disabled,
            notify_new_insights: true,
            notify_context_changes: false, // Less noisy by default
            notify_cache_refresh: false,   // Less noisy by default
            notification_timeout: 5000,    // 5 seconds
            min_urgency_threshold: 3,      // Medium+ urgency
        }
    }
}

/// Available notification delivery methods
#[derive(Debug, Clone, PartialEq)]
pub enum NotificationMethod {
    DBus,
    NotifySend,
    None,
}

/// Information about notification system capabilities
#[derive(Debug, Clone)]
pub struct NotificationCapabilities {
    pub dbus_available: bool,
    pub notify_send_available: bool,
    pub preferred_method: NotificationMethod,
}

/// Service for sending desktop notifications about insight updates
pub struct NotificationService {
    config: Arc<RwLock<NotificationConfig>>,
    last_notification: Arc<RwLock<Option<DateTime<Utc>>>>,
    notification_cooldown: chrono::Duration,
}

impl NotificationService {
    pub fn new(config: NotificationConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            last_notification: Arc::new(RwLock::new(None)),
            notification_cooldown: chrono::Duration::minutes(2), // Prevent spam
        }
    }

    /// Send a notification if configured and not on cooldown
    pub async fn notify(&self, notification_type: NotificationType) -> Result<()> {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};
        
        // Create a simple file-based lock to prevent multiple notifications from multiple waybar instances
        let lock_dir = "/tmp/jasper-notifications";
        let _ = fs::create_dir_all(&lock_dir);
        
        // Create a lock file based on notification content hash for deduplication
        let notification_hash = format!("{:x}", md5::compute(format!("{:?}", notification_type).as_bytes()));
        let lock_file = format!("{}/notify-{}", lock_dir, notification_hash);
        
        // Check if notification was sent recently (within 5 seconds)
        if let Ok(metadata) = fs::metadata(&lock_file) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(duration_since_epoch) = modified.duration_since(UNIX_EPOCH) {
                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                    if now.as_secs() - duration_since_epoch.as_secs() < 5 {
                        debug!("Notification recently sent by another process, skipping");
                        return Ok(());
                    }
                }
            }
        }
        
        // Create/update lock file
        let _ = fs::write(&lock_file, "locked");
        
        let config = self.config.read().clone();
        
        if !config.enabled {
            debug!("Notifications disabled, skipping");
            return Ok(());
        }

        // Check if we should send this type of notification
        let should_send = match &notification_type {
            NotificationType::NewInsight { .. } => config.notify_new_insights,
            NotificationType::ContextChanged { .. } => config.notify_context_changes,
            NotificationType::CacheRefreshed => config.notify_cache_refresh,
            NotificationType::AnalysisComplete { .. } => config.notify_new_insights,
        };

        if !should_send {
            debug!("Notification type disabled in config: {:?}", notification_type);
            return Ok(());
        }

        // Check cooldown to prevent notification spam
        if !self.is_cooldown_expired() {
            debug!("Notification on cooldown, skipping");
            return Ok(());
        }

        // Send the notification
        match self.send_desktop_notification(&notification_type, &config).await {
            Ok(_) => {
                self.update_last_notification_time();
                debug!("Notification sent successfully: {:?}", notification_type);
            }
            Err(e) => {
                warn!("Failed to send notification: {}", e);
            }
        }

        Ok(())
    }

    /// Send a desktop notification using native D-Bus via notify-rust
    async fn send_desktop_notification(
        &self, 
        notification_type: &NotificationType,
        config: &NotificationConfig
    ) -> Result<()> {
        let (title, body, icon) = self.format_notification(notification_type);

        // Prepare all notification parameters for the blocking task
        let timeout = config.notification_timeout as i32;
        let urgency = match notification_type {
            NotificationType::NewInsight { .. } => Urgency::Normal,
            NotificationType::ContextChanged { .. } => Urgency::Low,
            NotificationType::CacheRefreshed => Urgency::Low,
            NotificationType::AnalysisComplete { .. } => Urgency::Normal,
        };

        let category = match notification_type {
            NotificationType::NewInsight { .. } => "ai-insight",
            NotificationType::ContextChanged { .. } => "context-update",
            NotificationType::CacheRefreshed => "system",
            NotificationType::AnalysisComplete { .. } => "analysis",
        };

        let title_clone = title.clone();
        let body_clone = body.clone();
        let icon_clone = icon.clone();
        let category_string = category.to_string();

        debug!("Sending notification via D-Bus: {}", title);

        // Use spawn_blocking and create the notification entirely within the blocking context
        let notification_result = tokio::task::spawn_blocking(move || -> Result<String> {
            let mut notification = Notification::new();
            notification
                .summary(&title_clone)
                .body(&body_clone)
                .appname("Jasper")
                .timeout(timeout)
                .urgency(urgency)
                .hint(Hint::Category(category_string))
                .hint(Hint::DesktopEntry("jasper".to_string()));

            // Add icon if available
            if let Some(icon_name) = &icon_clone {
                notification.icon(icon_name);
            }

            // Send the notification and get the handle
            match notification.show() {
                Ok(handle) => {
                    let id = handle.id().to_string();
                    debug!("Notification sent successfully, handle: {}", id);
                    Ok(id)
                }
                Err(e) => Err(anyhow::anyhow!("D-Bus notification failed: {}", e))
            }
        }).await;

        match notification_result {
            Ok(Ok(handle_id)) => {
                debug!("Native D-Bus notification sent with handle: {}", handle_id);
                Ok(())
            }
            Ok(Err(e)) => {
                // Try fallback to shell command if D-Bus fails
                warn!("Native D-Bus notification failed ({}), attempting fallback to notify-send", e);
                self.send_notification_fallback(&title, &body, &icon, notification_type, config).await
            }
            Err(e) => {
                warn!("Failed to spawn blocking task for notification ({}), attempting fallback", e);
                self.send_notification_fallback(&title, &body, &icon, notification_type, config).await
            }
        }
    }

    /// Fallback notification method using shell command (for compatibility)
    async fn send_notification_fallback(
        &self,
        title: &str,
        body: &str,
        icon: &Option<String>,
        notification_type: &NotificationType,
        config: &NotificationConfig
    ) -> Result<()> {
        let mut cmd = Command::new("notify-send");
        cmd.arg("--app-name=Jasper")
            .arg("--expire-time")
            .arg(config.notification_timeout.to_string());

        // Add icon if available
        if let Some(icon_name) = icon {
            cmd.arg("--icon").arg(icon_name);
        }

        // Add urgency level based on notification type
        let urgency = match notification_type {
            NotificationType::NewInsight { .. } => "normal",
            NotificationType::ContextChanged { .. } => "low",
            NotificationType::CacheRefreshed => "low",
            NotificationType::AnalysisComplete { .. } => "normal",
        };
        cmd.arg("--urgency").arg(urgency);

        // Add title and body
        cmd.arg(title).arg(body);

        debug!("Executing fallback notification command: {:?}", cmd);

        let output = cmd.output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Both D-Bus and notify-send failed. notify-send error: {}", stderr));
        }

        Ok(())
    }

    /// Format notification content based on type
    fn format_notification(&self, notification_type: &NotificationType) -> (String, String, Option<String>) {
        match notification_type {
            NotificationType::NewInsight { message, icon } => {
                let title = "📅 Jasper: New Insight".to_string();
                let body = self.truncate_message(message, 200);
                let icon_name = icon.as_ref().map(|_| "calendar".to_string())
                    .or_else(|| Some("dialog-information".to_string()));
                (title, body, icon_name)
            }
            NotificationType::ContextChanged { changes } => {
                let title = "🔄 Jasper: Context Updated".to_string();
                let body = format!("Your schedule context has changed: {}", 
                    self.truncate_message(changes, 150));
                (title, body, Some("dialog-information".to_string()))
            }
            NotificationType::CacheRefreshed => {
                let title = "♻️ Jasper: Cache Refreshed".to_string();
                let body = "Insight cache has been refreshed with latest data".to_string();
                (title, body, Some("view-refresh".to_string()))
            }
            NotificationType::AnalysisComplete { insights_count } => {
                let title = "✅ Jasper: Analysis Complete".to_string();
                let body = match insights_count {
                    0 => "No new insights found".to_string(),
                    1 => "1 new insight available".to_string(),
                    n => format!("{} new insights available", n),
                };
                (title, body, Some("dialog-information".to_string()))
            }
        }
    }

    /// Truncate message to prevent overly long notifications
    fn truncate_message(&self, message: &str, max_length: usize) -> String {
        if message.len() <= max_length {
            message.to_string()
        } else {
            format!("{}...", &message[..max_length.saturating_sub(3)])
        }
    }

    /// Check if the notification cooldown has expired
    fn is_cooldown_expired(&self) -> bool {
        let last_notification = self.last_notification.read();
        match *last_notification {
            Some(last_time) => {
                let now = Utc::now();
                now.signed_duration_since(last_time) > self.notification_cooldown
            }
            None => true, // No previous notification
        }
    }

    /// Update the timestamp of the last notification
    fn update_last_notification_time(&self) {
        let mut last_notification = self.last_notification.write();
        *last_notification = Some(Utc::now());
    }

    /// Update notification configuration
    pub fn update_config(&self, config: NotificationConfig) {
        let mut current_config = self.config.write();
        *current_config = config;
        debug!("Notification configuration updated");
    }

    /// Get current notification configuration
    pub fn get_config(&self) -> NotificationConfig {
        self.config.read().clone()
    }

    /// Test notification system by sending a test notification
    pub async fn test_notification(&self) -> Result<()> {
        // Temporarily bypass cooldown for test
        {
            let mut last_notification = self.last_notification.write();
            *last_notification = None;
        }

        // Send a test notification directly via D-Bus first
        let test_result = tokio::task::spawn_blocking(move || -> Result<String> {
            let mut notification = Notification::new();
            notification
                .summary("🧪 Jasper Test")
                .body("This is a test notification from Jasper. If you see this, D-Bus notifications are working correctly!")
                .appname("Jasper")
                .timeout(5000)
                .urgency(Urgency::Normal)
                .hint(Hint::Category("test".to_string()))
                .hint(Hint::DesktopEntry("jasper".to_string()));

            match notification.show() {
                Ok(handle) => {
                    let id = handle.id().to_string();
                    Ok(id)
                }
                Err(e) => Err(anyhow::anyhow!("Test D-Bus notification failed: {}", e))
            }
        }).await;

        match test_result {
            Ok(Ok(handle_id)) => {
                debug!("Test D-Bus notification sent with handle: {}", handle_id);
                Ok(())
            }
            Ok(Err(e)) => {
                // Try fallback test with notify-send
                warn!("D-Bus test failed ({}), trying notify-send fallback", e);
                let test_notification = NotificationType::NewInsight {
                    message: "This is a test notification from Jasper via notify-send fallback. If you see this, shell notifications are working!".to_string(),
                    icon: Some("🧪".to_string()),
                };
                self.notify(test_notification).await?;
                Ok(())
            }
            Err(e) => {
                warn!("Failed to spawn test notification task ({})", e);
                Err(anyhow::anyhow!("Test notification system failed: {}", e))
            }
        }
    }

    /// Check if notification system is available (D-Bus and/or notify-send)
    pub fn is_notification_system_available(&self) -> bool {
        // First try D-Bus connectivity
        if self.is_dbus_notification_available() {
            return true;
        }

        // Fallback to notify-send check
        match Command::new("which").arg("notify-send").output() {
            Ok(output) => output.status.success(),
            Err(_) => {
                // Try direct execution as fallback
                match Command::new("notify-send").arg("--version").output() {
                    Ok(output) => output.status.success(),
                    Err(_) => false,
                }
            }
        }
    }

    /// Check if D-Bus notifications are available
    pub fn is_dbus_notification_available(&self) -> bool {
        // Use spawn_blocking to test D-Bus availability in a blocking context
        let rt = tokio::runtime::Handle::try_current();
        match rt {
            Ok(handle) => {
                // We're in an async context, use spawn_blocking
                let result = std::thread::spawn(move || {
                    // Try to create a minimal notification without showing it
                    match Notification::new() {
                        _notification => {
                            // Just check if we can create the notification object successfully
                            // We don't show it to avoid spam during availability checks
                            debug!("D-Bus notification system appears to be available");
                            true
                        }
                    }
                }).join();
                
                result.unwrap_or(false)
            }
            Err(_) => {
                // We're not in an async context, can test directly
                match Notification::new() {
                    _notification => {
                        debug!("D-Bus notification system appears to be available");
                        true
                    }
                }
            }
        }
    }

    /// Get detailed information about available notification methods
    pub fn get_notification_capabilities(&self) -> NotificationCapabilities {
        let dbus_available = self.is_dbus_notification_available();
        let notify_send_available = match Command::new("which").arg("notify-send").output() {
            Ok(output) => output.status.success(),
            Err(_) => false,
        };

        NotificationCapabilities {
            dbus_available,
            notify_send_available,
            preferred_method: if dbus_available { 
                NotificationMethod::DBus 
            } else if notify_send_available { 
                NotificationMethod::NotifySend 
            } else { 
                NotificationMethod::None 
            },
        }
    }

    /// Get a summary of notification system status
    pub fn get_system_info(&self) -> NotificationSystemInfo {
        let capabilities = self.get_notification_capabilities();
        NotificationSystemInfo {
            notifications_available: self.is_notification_system_available(),
            capabilities,
            config: self.get_config(),
            last_notification: *self.last_notification.read(),
            cooldown_active: !self.is_cooldown_expired(),
        }
    }
}

/// Information about the notification system status
#[derive(Debug)]
pub struct NotificationSystemInfo {
    pub notifications_available: bool,
    pub capabilities: NotificationCapabilities,
    pub config: NotificationConfig,
    pub last_notification: Option<DateTime<Utc>>,
    pub cooldown_active: bool,
}