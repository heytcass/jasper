{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.jasperCompanion;
in
{
  options.services.jasperCompanion = {
    enable = mkEnableOption "Jasper Companion personal assistant";
    
    package = mkOption {
      type = types.package;
      default = pkgs.jasper-companion;
      description = "Jasper Companion package to use";
    };
    
    user = mkOption {
      type = types.str;
      description = "User under which Jasper Companion runs";
    };
    
    enableGnomeExtension = mkOption {
      type = types.bool;
      default = false;
      description = "Enable the GNOME Shell extension";
    };
  };
  
  config = mkIf cfg.enable {
    # User systemd service (runs as user, not system service)
    systemd.user.services.jasper-companion = {
      description = "Jasper Companion Personal Assistant";
      after = [ "graphical-session.target" ];
      wantedBy = [ "default.target" ];
      
      serviceConfig = {
        Type = "simple";
        ExecStart = "${cfg.package}/bin/jasper-companion-daemon";
        Restart = "always";
        RestartSec = 5;
        
        # Security hardening for user service
        PrivateTmp = true;
        ProtectSystem = "strict";
        ProtectHome = false;  # Need access to user config
        ReadWritePaths = [
          "%h/.config/jasper-companion"
          "%h/.local/share/jasper-companion"
        ];
      };
    };
    
    # D-Bus service file for user session
    # This will be included in the package
    environment.systemPackages = [ cfg.package ] ++ 
      (optionals cfg.enableGnomeExtension [ pkgs.jasper-companion-gnome-extension ]);
  };
}