use anyhow::Result;
use std::process::Command;
use tracing::{debug, warn};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use parking_lot::RwLock;

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
        Self {
            enabled: true,
            notify_new_insights: true,
            notify_context_changes: false, // Less noisy by default
            notify_cache_refresh: false,   // Less noisy by default
            notification_timeout: 5000,    // 5 seconds
            min_urgency_threshold: 3,      // Medium+ urgency
        }
    }
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

    /// Send a desktop notification using notify-send
    async fn send_desktop_notification(
        &self, 
        notification_type: &NotificationType,
        config: &NotificationConfig
    ) -> Result<()> {
        let (title, body, icon) = self.format_notification(notification_type);

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

        debug!("Executing notification command: {:?}", cmd);

        let output = cmd.output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("notify-send failed: {}", stderr));
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
        let test_notification = NotificationType::NewInsight {
            message: "This is a test notification from Jasper. If you see this, notifications are working correctly!".to_string(),
            icon: Some("󰃭".to_string()),
        };

        // Temporarily bypass cooldown for test
        {
            let mut last_notification = self.last_notification.write();
            *last_notification = None;
        }

        self.notify(test_notification).await?;
        
        debug!("Test notification sent");
        Ok(())
    }

    /// Check if notify-send is available on the system
    pub fn is_notification_system_available(&self) -> bool {
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

    /// Get a summary of notification system status
    pub fn get_system_info(&self) -> NotificationSystemInfo {
        NotificationSystemInfo {
            notifications_available: self.is_notification_system_available(),
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
    pub config: NotificationConfig,
    pub last_notification: Option<DateTime<Utc>>,
    pub cooldown_active: bool,
}