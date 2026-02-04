# Enhanced NixOS Module for Unified Jasper Package
# Based on research of NixOS module patterns and desktop integration best practices

{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.jasperCompanion;
  
  # Default to unified package
  defaultPackage = pkgs.jasper-companion-unified or pkgs.jasper-companion;
  
  # Desktop environment detection helpers
  hasGnome = config.services.xserver.desktopManager.gnome.enable or false;
  hasCosmic = config.services.desktopManager.cosmic.enable or false;
  hasKde = config.services.xserver.desktopManager.plasma5.enable or false;
  hasWaybar = config.programs.waybar.enable or false;

  # Generate frontend list based on detection if auto-detect is enabled
  detectedFrontends =
    optionals hasGnome [ "gnome" ] ++
    optionals hasCosmic [ "cosmic" ] ++
    optionals hasKde [ "kde" ] ++
    optionals hasWaybar [ "waybar" ];
  
  # Final frontend list (manual override or auto-detected)
  activeFrontends = if cfg.forceEnableFrontends != [] 
                   then cfg.forceEnableFrontends 
                   else if cfg.autoDetectDesktop 
                   then detectedFrontends
                   else [ "terminal" ]; # fallback
  
  # GNOME extension auto-activation
  shouldEnableGnomeExtension = cfg.autoDetectDesktop && hasGnome && 
    (builtins.elem "gnome" activeFrontends);
    
  # Extension UUID from package passthru
  extensionUuid = cfg.package.passthru.extensionUuid or "jasper@tom.local";

in
{
  options.services.jasperCompanion = {
    enable = mkEnableOption "Jasper Companion unified package";
    
    package = mkOption {
      type = types.package;
      default = defaultPackage;
      description = lib.mdDoc ''
        Jasper Companion package to use. Defaults to the unified package with
        automatic desktop environment detection and integration.
      '';
    };
    
    user = mkOption {
      type = types.str;
      description = lib.mdDoc ''
        User under which Jasper Companion runs. This should be your main user account.
      '';
    };
    
    autoDetectDesktop = mkOption {
      type = types.bool;
      default = true;
      description = lib.mdDoc ''
        Automatically detect and enable appropriate desktop integrations based on
        the system configuration. When enabled, Jasper will automatically:
        
        - Enable GNOME extension if GNOME is configured
        - Enable Waybar integration if Waybar is configured  
        - Enable KDE integration if KDE is configured (future)
        - Provide cross-desktop D-Bus notifications
        
        Disable this if you want manual control over frontend selection.
      '';
    };
    
    forceEnableFrontends = mkOption {
      type = types.listOf (types.enum [ "gnome" "cosmic" "waybar" "kde" "terminal" ]);
      default = [];
      example = [ "gnome" "waybar" ];
      description = lib.mdDoc ''
        Manually specify which frontends to enable, overriding automatic detection.
        Available options:
        
        - `gnome`: GNOME Shell extension with panel integration
        - `cosmic`: COSMIC panel applet
        - `waybar`: JSON output for Waybar status bar
        - `kde`: KDE Plasma integration (planned)
        - `terminal`: Terminal-only interface
        
        Leave empty to use automatic detection.
      '';
    };
    
    enableDevelopmentMode = mkOption {
      type = types.bool;
      default = false;
      description = lib.mdDoc ''
        Enable development mode for testing and debugging. This uses development
        versions of extensions and enables additional logging.
      '';
    };
    
    extraConfig = mkOption {
      type = types.attrs;
      default = {};
      description = lib.mdDoc ''
        Additional configuration passed to the Jasper daemon. This allows
        fine-tuning of behavior beyond the desktop integration settings.
      '';
    };
  };
  
  config = mkIf cfg.enable {
    
    # Install unified package system-wide
    environment.systemPackages = [ cfg.package ];
    
    # User systemd service with desktop-aware configuration
    systemd.user.services.jasper-companion = {
      description = "Jasper Companion with Auto-Detection";
      documentation = [ "https://github.com/heytcass/jasper/docs/" ];
      after = [ "graphical-session.target" ];
      wants = [ "graphical-session.target" ];
      wantedBy = [ "default.target" ];
      
      serviceConfig = {
        Type = "simple";
        ExecStart = "${cfg.package}/bin/jasper-companion start";
        ExecReload = "${pkgs.coreutils}/bin/kill -HUP $MAINPID";
        Restart = "always";
        RestartSec = 5;
        
        # Security hardening
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectHome = "read-only";
        ProtectSystem = "strict";
        ReadWritePaths = [
          "%h/.local/share/jasper-companion"
          "%h/.config/jasper-companion"
        ];
        
        # D-Bus access for notifications
        PrivateNetwork = false;
      };
      
      environment = {
        # Pass configuration to daemon
        JASPER_AUTO_DETECT = toString cfg.autoDetectDesktop;
        JASPER_ACTIVE_FRONTENDS = concatStringsSep "," activeFrontends;
        JASPER_UNIFIED_PACKAGE = "1";
        
        # Disable daemon notifications when GNOME extension is enabled
        # Extension will handle notifications instead
        JASPER_DISABLE_DAEMON_NOTIFICATIONS = toString shouldEnableGnomeExtension;
        
        # Desktop environment variables
        XDG_RUNTIME_DIR = "/run/user/%i";
      } // cfg.extraConfig;
    };
    
    # D-Bus service integration for cross-desktop notifications
    services.dbus.packages = [ cfg.package ];
    
    # GNOME extension integration (conditional)
    programs.dconf = mkIf shouldEnableGnomeExtension {
      profiles.user.databases = [{
        settings = {
          "org/gnome/shell" = {
            enabled-extensions = [ extensionUuid ];
            
            # Auto-enable on first run
            disable-user-extensions = false;
          };
          
          # Extension-specific settings can be added here
          "org/gnome/shell/extensions/jasper" = {
            auto-refresh-enabled = true;
            refresh-interval = lib.gvariant.mkInt32 30;
            show-notifications = true;
          };
        };
      }];
    };
    
    # Note: Waybar integration is provided via the daemon's waybar command
    # Users should configure their waybar manually with:
    # "custom/jasper": {
    #   "exec": "jasper-companion waybar",
    #   "return-type": "json", 
    #   "interval": 30
    # }
    
    # System-level desktop integration
    xdg.portal = mkIf cfg.autoDetectDesktop {
      enable = mkDefault true;
      extraPortals = with pkgs; [
        # Ensure notification portal is available
        xdg-desktop-portal-gtk
      ] ++ optionals hasGnome [
        xdg-desktop-portal-gnome  
      ];
    };
    
    # Development mode configuration
    systemd.user.services.jasper-companion-dev = mkIf cfg.enableDevelopmentMode {
      description = "Jasper Companion Development Service";
      after = [ "jasper-companion.service" ];
      
      serviceConfig = {
        Type = "oneshot";
        ExecStart = "${cfg.package}/bin/jasper-gnome-extension-activate";
        RemainAfterExit = true;
      };
      
      environment = {
        JASPER_DEV_MODE = "1";
        JASPER_LOG_LEVEL = "debug";
      };
    };
    
    # User activation script for desktop detection
    systemd.user.services.jasper-desktop-detection = mkIf cfg.autoDetectDesktop {
      description = "Jasper Desktop Environment Detection";
      after = [ "graphical-session.target" ];
      wantedBy = [ "jasper-companion.service" ];
      
      serviceConfig = {
        Type = "oneshot";
        ExecStart = "${cfg.package}/bin/jasper-detect-desktop";
        StandardOutput = "journal";
      };
    };
    
    # Assertions and warnings for user guidance
    assertions = [
      {
        assertion = cfg.user != "";
        message = "services.jasperCompanion.user must be set to your main user account";
      }
      {
        assertion = cfg.autoDetectDesktop -> (hasGnome || hasCosmic || hasWaybar || cfg.forceEnableFrontends != []);
        message = ''
          Jasper auto-detection is enabled but no supported desktop environments were detected.
          Either:
          1. Enable a supported desktop (GNOME, COSMIC, Waybar)
          2. Set forceEnableFrontends to specify manual frontends
          3. Disable autoDetectDesktop for terminal-only usage
        '';
      }
    ];
    
    warnings = 
      optional (!cfg.autoDetectDesktop && cfg.forceEnableFrontends == []) ''
        Jasper Companion is configured with manual frontend control but no frontends specified.
        Set forceEnableFrontends or enable autoDetectDesktop for desktop integration.
      '' ++
      optional (cfg.enableDevelopmentMode && !hasGnome) ''
        Development mode is enabled but GNOME is not configured. Some development features may not work.
      '' ++
      optional (builtins.elem "kde" cfg.forceEnableFrontends) ''
        KDE frontend is specified but not yet implemented. It will be ignored.
      '';
  };
  
  meta = {
    maintainers = with lib.maintainers; [ ];
  };
}