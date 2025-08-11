use anyhow::Result;
use std::env;
use std::process::Command;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, PartialEq)]
pub enum DesktopEnvironment {
    Gnome,
    Kde,
    Sway,
    Hyprland,
    Xfce,
    Unknown(String),
}

impl DesktopEnvironment {
    pub fn name(&self) -> &str {
        match self {
            DesktopEnvironment::Gnome => "GNOME",
            DesktopEnvironment::Kde => "KDE Plasma",
            DesktopEnvironment::Sway => "Sway",
            DesktopEnvironment::Hyprland => "Hyprland",
            DesktopEnvironment::Xfce => "XFCE",
            DesktopEnvironment::Unknown(name) => name,
        }
    }
    
    pub fn supports_extensions(&self) -> bool {
        matches!(self, DesktopEnvironment::Gnome)
    }
    
    pub fn typical_status_bar(&self) -> Option<&'static str> {
        match self {
            DesktopEnvironment::Sway | DesktopEnvironment::Hyprland => Some("waybar"),
            DesktopEnvironment::Gnome => Some("gnome-shell"),
            DesktopEnvironment::Kde => Some("plasma-panel"),
            DesktopEnvironment::Xfce => Some("xfce4-panel"),
            DesktopEnvironment::Unknown(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SessionType {
    X11,
    Wayland,
    Unknown,
}

impl SessionType {
    pub fn name(&self) -> &str {
        match self {
            SessionType::X11 => "X11",
            SessionType::Wayland => "Wayland",
            SessionType::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComponentAvailability {
    pub waybar: bool,
    pub gnome_shell: bool,
    pub kde_plasma: bool,
    pub mako: bool,
    pub dunst: bool,
}

impl ComponentAvailability {
    pub fn available_status_bars(&self) -> Vec<&'static str> {
        let mut bars = Vec::new();
        if self.waybar { bars.push("waybar"); }
        if self.gnome_shell { bars.push("gnome-shell"); }
        if self.kde_plasma { bars.push("plasma-panel"); }
        bars
    }
    
    pub fn available_notification_services(&self) -> Vec<&'static str> {
        let mut services = Vec::new();
        if self.gnome_shell { services.push("gnome-shell"); }
        if self.kde_plasma { services.push("kde-notify"); }
        if self.mako { services.push("mako"); }
        if self.dunst { services.push("dunst"); }
        services
    }
}

#[derive(Debug, Clone)]
pub enum NotificationService {
    GnomeShell,
    KdeNotify,
    Mako,
    Dunst,
    None,
}

impl NotificationService {
    pub fn name(&self) -> &str {
        match self {
            NotificationService::GnomeShell => "GNOME Shell",
            NotificationService::KdeNotify => "KDE Notifications",
            NotificationService::Mako => "mako",
            NotificationService::Dunst => "dunst",
            NotificationService::None => "None detected",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DesktopContext {
    pub primary_de: DesktopEnvironment,
    pub session_type: SessionType,
    pub available_components: ComponentAvailability,
    pub notification_service: NotificationService,
}

impl DesktopContext {
    pub fn summary(&self) -> String {
        format!(
            "Desktop: {} ({}), Status bars: {:?}, Notifications: {}",
            self.primary_de.name(),
            self.session_type.name(),
            self.available_components.available_status_bars(),
            self.notification_service.name()
        )
    }
}

pub struct DesktopDetector {
    // Cache results to avoid repeated detection
    cached_context: Option<DesktopContext>,
}

impl DesktopDetector {
    pub fn new() -> Self {
        Self { cached_context: None }
    }
    
    /// Primary detection method with multiple fallback strategies
    pub fn detect(&mut self) -> Result<DesktopContext> {
        if let Some(ref context) = self.cached_context {
            debug!("Using cached desktop context");
            return Ok(context.clone());
        }
        
        info!("Detecting desktop environment...");
        let context = self.detect_with_fallbacks()?;
        info!("Desktop detection result: {}", context.summary());
        
        self.cached_context = Some(context.clone());
        Ok(context)
    }
    
    /// Force re-detection (clears cache)
    pub fn refresh(&mut self) -> Result<DesktopContext> {
        info!("Refreshing desktop environment detection...");
        self.cached_context = None;
        self.detect()
    }
    
    fn detect_with_fallbacks(&self) -> Result<DesktopContext> {
        // Strategy 1: XDG environment variables (most reliable)
        if let Ok(context) = self.detect_via_xdg_vars() {
            debug!("Desktop detected via XDG environment variables");
            return Ok(context);
        }
        
        // Strategy 2: Process detection
        if let Ok(context) = self.detect_via_processes() {
            debug!("Desktop detected via running processes");
            return Ok(context);
        }
        
        // Strategy 3: Session file analysis
        if let Ok(context) = self.detect_via_session_files() {
            debug!("Desktop detected via session files");
            return Ok(context);
        }
        
        // Fallback: Unknown environment with component detection
        warn!("Desktop environment detection failed, using component-based fallback");
        Ok(self.detect_unknown_environment())
    }
    
    fn detect_via_xdg_vars(&self) -> Result<DesktopContext> {
        // XDG_CURRENT_DESKTOP is colon-separated list (like PATH)
        let xdg_desktop = env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| env::var("XDG_SESSION_DESKTOP"))
            .or_else(|_| env::var("DESKTOP_SESSION"))?;
            
        debug!("Found desktop environment variable: {}", xdg_desktop);
        
        let primary_de = self.parse_xdg_desktop(&xdg_desktop);
        let session_type = self.detect_session_type();
        let available_components = self.detect_available_components();
        let notification_service = self.detect_notification_service(&primary_de, &available_components);
        
        Ok(DesktopContext {
            primary_de,
            session_type,
            available_components,
            notification_service,
        })
    }
    
    fn detect_via_processes(&self) -> Result<DesktopContext> {
        let components = self.detect_available_components();
        
        let primary_de = if components.gnome_shell {
            DesktopEnvironment::Gnome
        } else if components.kde_plasma {
            DesktopEnvironment::Kde
        } else if self.is_process_running("sway") {
            DesktopEnvironment::Sway
        } else if self.is_process_running("Hyprland") {
            DesktopEnvironment::Hyprland
        } else if self.is_process_running("xfce4-session") {
            DesktopEnvironment::Xfce
        } else {
            return Err(anyhow::anyhow!("Could not detect desktop via processes"));
        };
        
        let session_type = self.detect_session_type();
        let notification_service = self.detect_notification_service(&primary_de, &components);
        
        Ok(DesktopContext {
            primary_de,
            session_type,
            available_components: components,
            notification_service,
        })
    }
    
    fn detect_via_session_files(&self) -> Result<DesktopContext> {
        // Look for session files that might indicate the desktop environment
        let session_files = [
            ("/usr/bin/gnome-session", DesktopEnvironment::Gnome),
            ("/usr/bin/plasma-session", DesktopEnvironment::Kde),
            ("/usr/bin/startxfce4", DesktopEnvironment::Xfce),
        ];
        
        for (path, de) in &session_files {
            if std::path::Path::new(path).exists() {
                let session_type = self.detect_session_type();
                let components = self.detect_available_components();
                let notification_service = self.detect_notification_service(de, &components);
                
                return Ok(DesktopContext {
                    primary_de: de.clone(),
                    session_type,
                    available_components: components,
                    notification_service,
                });
            }
        }
        
        Err(anyhow::anyhow!("No session files found"))
    }
    
    fn detect_unknown_environment(&self) -> DesktopContext {
        let session_type = self.detect_session_type();
        let components = self.detect_available_components();
        
        // Try to infer from components
        let primary_de = if components.gnome_shell {
            DesktopEnvironment::Gnome
        } else if components.kde_plasma {
            DesktopEnvironment::Kde
        } else {
            DesktopEnvironment::Unknown("Undetected".to_string())
        };
        
        let notification_service = self.detect_notification_service(&primary_de, &components);
        
        DesktopContext {
            primary_de,
            session_type,
            available_components: components,
            notification_service,
        }
    }
    
    fn parse_xdg_desktop(&self, xdg_desktop: &str) -> DesktopEnvironment {
        // Handle colon-separated values (like PATH)
        let desktop_entries: Vec<&str> = xdg_desktop.split(':').collect();
        debug!("Parsing XDG desktop entries: {:?}", desktop_entries);
        
        for entry in &desktop_entries {
            match entry.to_lowercase().as_str() {
                "gnome" => return DesktopEnvironment::Gnome,
                "kde" | "plasma" => return DesktopEnvironment::Kde,
                "sway" => return DesktopEnvironment::Sway,
                "hyprland" => return DesktopEnvironment::Hyprland,
                "xfce" | "xfce4" => return DesktopEnvironment::Xfce,
                _ => {
                    debug!("Unknown desktop entry: {}", entry);
                    continue;
                }
            }
        }
        
        DesktopEnvironment::Unknown(xdg_desktop.to_string())
    }
    
    fn detect_session_type(&self) -> SessionType {
        if env::var("WAYLAND_DISPLAY").is_ok() {
            debug!("Detected Wayland session");
            SessionType::Wayland
        } else if env::var("DISPLAY").is_ok() {
            debug!("Detected X11 session");
            SessionType::X11
        } else {
            debug!("Could not detect session type");
            SessionType::Unknown
        }
    }
    
    fn detect_available_components(&self) -> ComponentAvailability {
        let waybar = self.is_process_running("waybar") || self.has_config_dir("waybar");
        let gnome_shell = self.is_process_running("gnome-shell");
        let kde_plasma = self.is_process_running("plasmashell") || 
                        env::var("KDE_FULL_SESSION").is_ok();
        let mako = self.is_process_running("mako") || self.command_exists("mako");
        let dunst = self.is_process_running("dunst") || self.command_exists("dunst");
        
        debug!("Component availability - waybar: {}, gnome-shell: {}, kde-plasma: {}, mako: {}, dunst: {}", 
               waybar, gnome_shell, kde_plasma, mako, dunst);
        
        ComponentAvailability {
            waybar,
            gnome_shell,
            kde_plasma,
            mako,
            dunst,
        }
    }
    
    fn detect_notification_service(&self, de: &DesktopEnvironment, components: &ComponentAvailability) -> NotificationService {
        let service = match de {
            DesktopEnvironment::Gnome => NotificationService::GnomeShell,
            DesktopEnvironment::Kde => NotificationService::KdeNotify,
            DesktopEnvironment::Sway | DesktopEnvironment::Hyprland => {
                if components.mako {
                    NotificationService::Mako
                } else if components.dunst {
                    NotificationService::Dunst
                } else {
                    NotificationService::None
                }
            }
            _ => {
                if components.dunst {
                    NotificationService::Dunst
                } else if components.mako {
                    NotificationService::Mako
                } else {
                    NotificationService::None
                }
            }
        };
        
        debug!("Detected notification service: {}", service.name());
        service
    }
    
    // Utility methods for detection
    fn is_process_running(&self, process_name: &str) -> bool {
        let result = Command::new("pgrep")
            .arg("-x")
            .arg(process_name)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        
        if result {
            debug!("Process '{}' is running", process_name);
        }
        
        result
    }
    
    fn command_exists(&self, command: &str) -> bool {
        let result = Command::new("which")
            .arg(command)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        
        if result {
            debug!("Command '{}' is available", command);
        }
        
        result
    }
    
    fn has_config_dir(&self, config_name: &str) -> bool {
        let result = if let Ok(home) = env::var("HOME") {
            std::path::Path::new(&home)
                .join(".config")
                .join(config_name)
                .exists()
        } else {
            false
        };
        
        if result {
            debug!("Config directory '{}' exists", config_name);
        }
        
        result
    }
}

impl Default for DesktopDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_xdg_desktop() {
        let detector = DesktopDetector::new();
        
        assert_eq!(detector.parse_xdg_desktop("gnome"), DesktopEnvironment::Gnome);
        assert_eq!(detector.parse_xdg_desktop("kde"), DesktopEnvironment::Kde);
        assert_eq!(detector.parse_xdg_desktop("sway"), DesktopEnvironment::Sway);
        assert_eq!(detector.parse_xdg_desktop("GNOME:ubuntu:Unity"), DesktopEnvironment::Gnome);
        assert_eq!(detector.parse_xdg_desktop("ubuntu:GNOME"), DesktopEnvironment::Gnome);
    }
    
    #[test]
    fn test_desktop_environment_methods() {
        assert_eq!(DesktopEnvironment::Gnome.name(), "GNOME");
        assert!(DesktopEnvironment::Gnome.supports_extensions());
        assert!(!DesktopEnvironment::Sway.supports_extensions());
        assert_eq!(DesktopEnvironment::Sway.typical_status_bar(), Some("waybar"));
    }
    
    #[test]
    fn test_session_type_detection() {
        let detector = DesktopDetector::new();
        // Note: This will vary based on test environment
        let session_type = detector.detect_session_type();
        // Just ensure it returns something valid
        assert!(matches!(session_type, SessionType::X11 | SessionType::Wayland | SessionType::Unknown));
    }
}