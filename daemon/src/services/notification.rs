use anyhow::Result;
use std::process::Command;
use tracing::{debug, warn};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use parking_lot::{RwLock, Mutex};
use std::collections::HashMap;
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
    /// Notification method preference (auto, dbus, notify-send)
    pub preferred_method: String,
    /// Application name for notifications  
    pub app_name: String,
    /// Custom desktop entry name for better integration
    pub desktop_entry: String,
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
            preferred_method: "auto".to_string(),
            app_name: "Jasper".to_string(),
            desktop_entry: "jasper".to_string(),
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
    // Atomic deduplication cache to prevent race conditions
    notification_cache: Arc<Mutex<HashMap<String, DateTime<Utc>>>>,
}

impl NotificationService {
    pub fn new(config: NotificationConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            last_notification: Arc::new(RwLock::new(None)),
            notification_cooldown: chrono::Duration::minutes(2), // Prevent spam
            notification_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Send a notification if configured and not on cooldown
    pub async fn notify(&self, notification_type: NotificationType) -> Result<()> {
        // Use atomic mutex-based deduplication to prevent race conditions
        let notification_hash = format!("{:x}", md5::compute(format!("{:?}", notification_type).as_bytes()));
        let now = Utc::now();
        
        // Check deduplication cache with atomic operations
        {
            let mut cache = self.notification_cache.lock();
            
            // Clean up old entries (older than 5 seconds)
            let cutoff = now - chrono::Duration::seconds(5);
            cache.retain(|_, &mut timestamp| timestamp > cutoff);
            
            // Check if this notification was recently sent
            if let Some(&last_sent) = cache.get(&notification_hash) {
                if now.signed_duration_since(last_sent) < chrono::Duration::seconds(5) {
                    debug!("Notification recently sent, skipping duplicate: {}", notification_hash);
                    return Ok(());
                }
            }
            
            // Record this notification attempt
            cache.insert(notification_hash.clone(), now);
        }
        
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

    /// Send a desktop notification using the best available method
    async fn send_desktop_notification(
        &self, 
        notification_type: &NotificationType,
        config: &NotificationConfig
    ) -> Result<()> {
        let (title, body, icon) = self.format_notification(notification_type);
        
        // Determine the best notification method once
        let method = self.determine_notification_method(config);
        debug!("Using notification method: {:?} for: {}", method, title);
        
        match method {
            NotificationMethod::DBus => {
                self.send_dbus_notification(&title, &body, &icon, notification_type, config).await
            }
            NotificationMethod::NotifySend => {
                self.send_notify_send_notification(&title, &body, &icon, notification_type, config).await
            }
            NotificationMethod::None => {
                debug!("No notification method available, skipping: {}", title);
                Ok(())
            }
        }
    }
    
    /// Determine the best notification method based on configuration and availability
    fn determine_notification_method(&self, config: &NotificationConfig) -> NotificationMethod {
        match config.preferred_method.as_str() {
            "dbus" => {
                if self.is_dbus_notification_available() {
                    NotificationMethod::DBus
                } else {
                    warn!("D-Bus notifications requested but not available, falling back");
                    self.get_fallback_method()
                }
            }
            "notify-send" => {
                if self.is_notify_send_available() {
                    NotificationMethod::NotifySend
                } else {
                    warn!("notify-send requested but not available, falling back");
                    self.get_fallback_method()
                }
            }
            "auto" | _ => self.get_best_available_method(),
        }
    }
    
    /// Get the best available notification method
    fn get_best_available_method(&self) -> NotificationMethod {
        if self.is_dbus_notification_available() {
            NotificationMethod::DBus
        } else if self.is_notify_send_available() {
            NotificationMethod::NotifySend
        } else {
            NotificationMethod::None
        }
    }
    
    /// Get fallback method when preferred method is unavailable
    fn get_fallback_method(&self) -> NotificationMethod {
        if self.is_notify_send_available() {
            NotificationMethod::NotifySend
        } else if self.is_dbus_notification_available() {
            NotificationMethod::DBus
        } else {
            NotificationMethod::None
        }
    }

    /// Send notification via D-Bus (native method)
    async fn send_dbus_notification(
        &self,
        title: &str,
        body: &str,
        icon: &Option<String>,
        notification_type: &NotificationType,
        config: &NotificationConfig
    ) -> Result<()> {
        let timeout = config.notification_timeout as i32;
        let urgency = self.get_urgency_for_type(notification_type);
        let category = self.get_category_for_type(notification_type);

        let title_clone = title.to_string();
        let body_clone = body.to_string();
        let icon_clone = icon.clone();
        let category_string = category.to_string();

        // Use spawn_blocking for D-Bus operations
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

            if let Some(icon_name) = &icon_clone {
                notification.icon(icon_name);
            }

            match notification.show() {
                Ok(handle) => {
                    let id = handle.id().to_string();
                    debug!("D-Bus notification sent, handle: {}", id);
                    Ok(id)
                }
                Err(e) => Err(anyhow::anyhow!("D-Bus notification failed: {}", e))
            }
        }).await;

        match notification_result {
            Ok(Ok(handle_id)) => {
                debug!("D-Bus notification successful: {}", handle_id);
                Ok(())
            }
            Ok(Err(e)) => {
                warn!("D-Bus notification failed: {}", e);
                Err(e)
            }
            Err(e) => {
                warn!("D-Bus notification task failed: {}", e);
                Err(anyhow::anyhow!("D-Bus notification task error: {}", e))
            }
        }
    }

    /// Send notification via notify-send command
    async fn send_notify_send_notification(
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
        let urgency = match self.get_urgency_for_type(notification_type) {
            Urgency::Low => "low",
            Urgency::Normal => "normal",
            Urgency::Critical => "critical",
        };
        cmd.arg("--urgency").arg(urgency);

        // Add title and body
        cmd.arg(title).arg(body);

        debug!("Executing fallback notification command: {:?}", cmd);

        let output = cmd.output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("notify-send command failed: {}", stderr));
        }

        Ok(())
    }
    
    /// Get urgency level for notification type
    fn get_urgency_for_type(&self, notification_type: &NotificationType) -> Urgency {
        match notification_type {
            NotificationType::NewInsight { .. } => Urgency::Normal,
            NotificationType::ContextChanged { .. } => Urgency::Low,
            NotificationType::CacheRefreshed => Urgency::Low,
            NotificationType::AnalysisComplete { .. } => Urgency::Normal,
        }
    }
    
    /// Get category for notification type
    fn get_category_for_type(&self, notification_type: &NotificationType) -> &'static str {
        match notification_type {
            NotificationType::NewInsight { .. } => "ai-insight",
            NotificationType::ContextChanged { .. } => "context-update", 
            NotificationType::CacheRefreshed => "system",
            NotificationType::AnalysisComplete { .. } => "analysis",
        }
    }
    
    /// Check if notify-send is available
    fn is_notify_send_available(&self) -> bool {
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

        // Create a test notification and send via the normal routing
        let test_notification = NotificationType::NewInsight {
            message: "🧪 Test notification from Jasper! If you see this, notifications are working correctly.".to_string(),
            icon: Some("dialog-information".to_string()),
        };

        // Send via the normal notification routing system
        debug!("Sending test notification via standard routing");
        self.notify(test_notification).await?;
        
        debug!("Test notification completed successfully");
        Ok(())
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
            Ok(_handle) => {
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
    
    /// Get detailed diagnostic information for debugging
    pub fn get_diagnostic_info(&self) -> NotificationDiagnostics {
        let config = self.get_config();
        
        NotificationDiagnostics {
            enabled: config.enabled,
            preferred_method: config.preferred_method.clone(),
            determined_method: self.determine_notification_method(&config),
            dbus_available: self.is_dbus_notification_available(),
            notify_send_available: self.is_notify_send_available(),
            cache_size: self.notification_cache.lock().len(),
            last_notification: *self.last_notification.read(),
            cooldown_remaining: if !self.is_cooldown_expired() {
                let last_notification = self.last_notification.read();
                if let Some(last_time) = *last_notification {
                    let elapsed = Utc::now().signed_duration_since(last_time);
                    Some((self.notification_cooldown - elapsed).to_std().unwrap_or(std::time::Duration::ZERO))
                } else {
                    None
                }
            } else {
                None
            },
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

/// Detailed diagnostic information for troubleshooting
#[derive(Debug)]
#[allow(dead_code)]
pub struct NotificationDiagnostics {
    pub enabled: bool,
    pub preferred_method: String,
    pub determined_method: NotificationMethod,
    pub dbus_available: bool,
    pub notify_send_available: bool,
    pub cache_size: usize,
    pub last_notification: Option<DateTime<Utc>>,
    pub cooldown_remaining: Option<std::time::Duration>,
}