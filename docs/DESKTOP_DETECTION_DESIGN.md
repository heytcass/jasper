# Desktop Detection Module Design

## Executive Summary

Based on extensive research of cross-desktop Linux applications, NixOS packaging patterns, and D-Bus service management, this document outlines the design for Jasper's desktop environment detection and unified packaging system.

## Research Findings That Drive This Design

### Key Challenges Discovered

1. **D-Bus Notification Conflicts**: Multiple notification services compete for `org.freedesktop.Notifications`
2. **GNOME Extension Limitations**: System-wide extensions require manual activation via dconf
3. **Environment Variable Inconsistencies**: Different DEs use different variables
4. **Wayland vs X11 Differences**: Sway/Hyprland prefer Wayland protocols over D-Bus
5. **NixOS Conditional Packaging**: Override patterns vs runtime detection tradeoffs

### Successful Patterns from Research

1. **NetworkManager Pattern**: `withGnome` override flags for conditional desktop components
2. **GSConnect Pattern**: Proper GNOME extension packaging with UUID management
3. **D-Bus Dispatcher Pattern**: Environment-aware service dispatch scripts
4. **Feature Flag Pattern**: Rust conditional compilation with desktop-specific features

## Architecture Design

### Core Detection Strategy

```rust
// daemon/src/desktop_detection.rs

use anyhow::Result;
use std::env;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub enum DesktopEnvironment {
    Gnome,
    Kde,
    Sway,
    Hyprland, 
    Xfce,
    Unknown(String),
}

#[derive(Debug, Clone)]
pub struct DesktopContext {
    pub primary_de: DesktopEnvironment,
    pub session_type: SessionType, // X11 or Wayland
    pub available_components: ComponentAvailability,
    pub notification_service: NotificationService,
}

#[derive(Debug, Clone)]
pub enum SessionType {
    X11,
    Wayland,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ComponentAvailability {
    pub waybar: bool,
    pub gnome_shell: bool,
    pub kde_plasma: bool,
    pub mako: bool,
    pub dunst: bool,
}

#[derive(Debug, Clone)]
pub enum NotificationService {
    GnomeShell,
    KdeNotify,
    Mako,
    Dunst,
    None,
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
            return Ok(context.clone());
        }
        
        let context = self.detect_with_fallbacks()?;
        self.cached_context = Some(context.clone());
        Ok(context)
    }
    
    fn detect_with_fallbacks(&self) -> Result<DesktopContext> {
        // Strategy 1: XDG environment variables (most reliable)
        if let Ok(context) = self.detect_via_xdg_vars() {
            return Ok(context);
        }
        
        // Strategy 2: Process detection
        if let Ok(context) = self.detect_via_processes() {
            return Ok(context);
        }
        
        // Strategy 3: Session file analysis
        if let Ok(context) = self.detect_via_session_files() {
            return Ok(context);
        }
        
        // Fallback: Unknown environment with component detection
        Ok(self.detect_unknown_environment())
    }
    
    fn detect_via_xdg_vars(&self) -> Result<DesktopContext> {
        // XDG_CURRENT_DESKTOP is colon-separated list
        let xdg_desktop = env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| env::var("XDG_SESSION_DESKTOP"))
            .or_else(|_| env::var("DESKTOP_SESSION"))?;
            
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
    
    fn parse_xdg_desktop(&self, xdg_desktop: &str) -> DesktopEnvironment {
        // Handle colon-separated values (like PATH)
        let desktop_entries: Vec<&str> = xdg_desktop.split(':').collect();
        
        for entry in &desktop_entries {
            match entry.to_lowercase().as_str() {
                "gnome" => return DesktopEnvironment::Gnome,
                "kde" | "plasma" => return DesktopEnvironment::Kde,
                "sway" => return DesktopEnvironment::Sway,
                "hyprland" => return DesktopEnvironment::Hyprland,
                "xfce" | "xfce4" => return DesktopEnvironment::Xfce,
                _ => continue,
            }
        }
        
        DesktopEnvironment::Unknown(xdg_desktop.to_string())
    }
    
    fn detect_session_type(&self) -> SessionType {
        if env::var("WAYLAND_DISPLAY").is_ok() {
            SessionType::Wayland
        } else if env::var("DISPLAY").is_ok() {
            SessionType::X11
        } else {
            SessionType::Unknown
        }
    }
    
    fn detect_available_components(&self) -> ComponentAvailability {
        ComponentAvailability {
            waybar: self.is_process_running("waybar") || self.has_config_dir("waybar"),
            gnome_shell: self.is_process_running("gnome-shell"),
            kde_plasma: self.is_process_running("plasmashell") || 
                       env::var("KDE_FULL_SESSION").is_ok(),
            mako: self.is_process_running("mako") || self.command_exists("mako"),
            dunst: self.is_process_running("dunst") || self.command_exists("dunst"),
        }
    }
    
    fn detect_notification_service(&self, de: &DesktopEnvironment, components: &ComponentAvailability) -> NotificationService {
        match de {
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
                } else {
                    NotificationService::None
                }
            }
        }
    }
    
    // Utility methods for detection
    fn is_process_running(&self, process_name: &str) -> bool {
        Command::new("pgrep")
            .arg("-x")
            .arg(process_name)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
    
    fn command_exists(&self, command: &str) -> bool {
        Command::new("which")
            .arg(command)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
    
    fn has_config_dir(&self, config_name: &str) -> bool {
        if let Ok(home) = env::var("HOME") {
            std::path::Path::new(&home)
                .join(".config")
                .join(config_name)
                .exists()
        } else {
            false
        }
    }
}
```

## Enhanced Frontend Registry Integration

```rust
// daemon/src/frontend_framework.rs - Enhanced version

impl FrontendRegistry {
    /// Create registry with desktop environment detection
    pub fn new_with_detection(desktop_context: &DesktopContext) -> Self {
        let mut registry = Self::new();
        
        // Register formatters based on detected environment
        match desktop_context.primary_de {
            DesktopEnvironment::Gnome => {
                registry.register(GnomeFrontendFormatter::new(true));
            }
            DesktopEnvironment::Kde => {
                // Future: KDE Plasma widget formatter
                registry.register(TerminalFrontendFormatter::new(true));
            }
            DesktopEnvironment::Sway | DesktopEnvironment::Hyprland => {
                if desktop_context.available_components.waybar {
                    registry.register(WaybarFrontendFormatter::new());
                }
            }
            _ => {
                // Unknown environment - register available components
                if desktop_context.available_components.waybar {
                    registry.register(WaybarFrontendFormatter::new());
                }
                if desktop_context.available_components.gnome_shell {
                    registry.register(GnomeFrontendFormatter::new(true));
                }
            }
        }
        
        // Always register terminal formatter for debugging
        registry.register(TerminalFrontendFormatter::new(true));
        
        registry
    }
}
```

## Unified Package Architecture

```nix
# flake.nix - Unified package approach
{
  outputs = { self, nixpkgs, ... }: {
    packages = flake-utils.lib.eachDefaultSystem (system: {
      default = pkgs.callPackage ./nix/unified-package.nix {
        inherit system;
        jasperDaemon = self.packages.${system}.daemon;
        jasperGnomeExtension = self.packages.${system}.gnome-extension-component;
      };
      
      daemon = pkgs.rustPlatform.buildRustPackage {
        # Core daemon with desktop detection
        buildFeatures = [ "desktop-detection" ];
      };
      
      gnome-extension-component = pkgs.stdenv.mkDerivation {
        # GNOME extension as separate component
        installPhase = ''
          mkdir -p $out/share/gnome-shell/extensions/jasper@tom.local
          cp -r * $out/share/gnome-shell/extensions/jasper@tom.local/
        '';
      };
    });
  };
}
```

```nix
# nix/unified-package.nix
{ lib, pkgs, system, jasperDaemon, jasperGnomeExtension }:

pkgs.symlinkJoin {
  name = "jasper-companion-unified";
  
  paths = [ 
    jasperDaemon 
  ];
  
  # Conditional GNOME extension inclusion
  postBuild = ''
    # Create extension directory structure
    mkdir -p $out/share/gnome-shell/extensions
    
    # Conditionally symlink GNOME extension
    if [ -d "${jasperGnomeExtension}/share/gnome-shell/extensions" ]; then
      cp -r ${jasperGnomeExtension}/share/gnome-shell/extensions/* \
             $out/share/gnome-shell/extensions/
    fi
    
    # Create desktop detection utilities
    mkdir -p $out/bin
    cat > $out/bin/jasper-detect-desktop <<'EOF'
#!/usr/bin/env bash
# Desktop environment detection utility
${jasperDaemon}/bin/jasper-companion-daemon --detect-desktop
EOF
    chmod +x $out/bin/jasper-detect-desktop
  '';
  
  meta = with lib; {
    description = "Unified Jasper Companion with automatic desktop detection";
    platforms = platforms.linux;
  };
}
```

## NixOS Module Enhancement

```nix
# nix/module.nix - Simplified module
{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.jasperCompanion;
in
{
  options.services.jasperCompanion = {
    enable = mkEnableOption "Jasper Companion unified package";
    
    package = mkOption {
      type = types.package;
      default = pkgs.jasper-companion-unified;
      description = "Unified Jasper Companion package";
    };
    
    user = mkOption {
      type = types.str;
      description = "User to run Jasper Companion as";
    };
    
    autoDetectDesktop = mkOption {
      type = types.bool;
      default = true;
      description = "Automatically detect and enable desktop integrations";
    };
    
    forceEnableFrontends = mkOption {
      type = types.listOf types.str;
      default = [];
      description = "Manually force specific frontends (gnome, waybar, terminal)";
    };
  };
  
  config = mkIf cfg.enable {
    # Install unified package
    environment.systemPackages = [ cfg.package ];
    
    # User systemd service
    systemd.user.services.jasper-companion = {
      description = "Jasper Companion with Auto-Detection";
      after = [ "graphical-session.target" ];
      wantedBy = [ "default.target" ];
      
      serviceConfig = {
        Type = "simple";
        ExecStart = "${cfg.package}/bin/jasper-companion-daemon";
        Restart = "always";
        RestartSec = 5;
      };
      
      environment = {
        # Ensure environment variables are available
        JASPER_AUTO_DETECT = toString cfg.autoDetectDesktop;
        JASPER_FORCE_FRONTENDS = concatStringsSep "," cfg.forceEnableFrontends;
      };
    };
    
    # GNOME extension activation (if GNOME detected)
    programs.dconf.profiles.user.databases = mkIf cfg.autoDetectDesktop [{
      settings = mkIf (config.services.xserver.desktopManager.gnome.enable) {
        "org/gnome/shell" = {
          enabled-extensions = [
            "jasper@tom.local"
          ];
        };
      };
    }];
    
    # D-Bus service for cross-desktop notifications
    services.dbus.packages = [ cfg.package ];
  };
}
```

## Risk Mitigation Strategy

### Fallback Mechanisms
1. **Unknown Environment**: Gracefully handle unrecognized desktop environments
2. **Component Detection**: Check for available tools even if DE is unknown
3. **Process Failures**: Handle command execution failures gracefully
4. **Cache Invalidation**: Allow re-detection if environment changes

### Testing Strategy
1. **Mock Environments**: Create test fixtures for different DE combinations
2. **Integration Tests**: Test with real desktop environments
3. **Edge Cases**: Test with missing/broken components
4. **Performance**: Ensure detection is fast and doesn't block startup

## Implementation Phases

### Phase 1: Core Detection Module
- Implement `DesktopDetector` with fallback strategies
- Add comprehensive test coverage
- Validate against real desktop environments

### Phase 2: Frontend Registry Integration
- Update `FrontendRegistry::new_with_detection()`
- Modify daemon startup to use desktop context
- Test formatter activation

### Phase 3: Unified Package Creation
- Create `unified-package.nix`
- Update flake.nix structure
- Test conditional component installation

### Phase 4: NixOS Module Enhancement
- Simplify module configuration
- Add auto-detection options
- Test system-wide activation

This design leverages all the research findings to create a robust, production-ready solution that handles the real-world complexities of cross-desktop Linux environments.