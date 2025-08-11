# Unified Jasper Package with Desktop Environment Detection
# Based on research of nixpkgs patterns and desktop detection best practices

{ lib
, pkgs
, system
, jasperDaemon ? pkgs.callPackage ../daemon { }
, jasperGnomeExtension ? pkgs.callPackage ./gnome-extension.nix { }
, writeShellScript
, symlinkJoin
}:

let
  # Desktop detection utility script
  desktopDetectScript = writeShellScript "jasper-detect-desktop" ''
    # Desktop environment detection utility for Jasper
    # Based on XDG standards and modern desktop detection practices
    
    # Primary detection via XDG environment variables
    if [ -n "$XDG_CURRENT_DESKTOP" ]; then
      echo "XDG_CURRENT_DESKTOP: $XDG_CURRENT_DESKTOP"
    elif [ -n "$XDG_SESSION_DESKTOP" ]; then
      echo "XDG_SESSION_DESKTOP: $XDG_SESSION_DESKTOP"
    elif [ -n "$DESKTOP_SESSION" ]; then
      echo "DESKTOP_SESSION: $DESKTOP_SESSION"
    fi
    
    # Session type detection
    if [ -n "$WAYLAND_DISPLAY" ]; then
      echo "Session Type: Wayland"
    elif [ -n "$DISPLAY" ]; then
      echo "Session Type: X11"
    else
      echo "Session Type: Unknown"
    fi
    
    # Process-based detection
    echo "Running Processes:"
    pgrep -l waybar && echo "  - Waybar: Running" || echo "  - Waybar: Not running"
    pgrep -l gnome-shell && echo "  - GNOME Shell: Running" || echo "  - GNOME Shell: Not running"
    pgrep -l plasmashell && echo "  - KDE Plasma: Running" || echo "  - KDE Plasma: Not running"
    pgrep -l sway && echo "  - Sway: Running" || echo "  - Sway: Not running"
    pgrep -l Hyprland && echo "  - Hyprland: Running" || echo "  - Hyprland: Not running"
    
    # Configuration detection
    echo "Available Configurations:"
    [ -d "$HOME/.config/waybar" ] && echo "  - Waybar config: Found" || echo "  - Waybar config: Not found"
    [ -d "/usr/share/gnome-shell/extensions" ] && echo "  - GNOME extensions dir: Found" || echo "  - GNOME extensions dir: Not found"
    
    # Command availability
    echo "Available Commands:"
    command -v mako >/dev/null && echo "  - mako: Available" || echo "  - mako: Not available"
    command -v dunst >/dev/null && echo "  - dunst: Available" || echo "  - dunst: Not available"
    command -v waybar >/dev/null && echo "  - waybar: Available" || echo "  - waybar: Not available"
    
    echo ""
    echo "For detailed detection, use: ${jasperDaemon}/bin/jasper-companion-daemon detect-desktop"
  '';

  # GNOME extension auto-activation script
  gnomeExtensionActivator = writeShellScript "jasper-gnome-extension-activate" ''
    # Auto-activate GNOME extension if GNOME is detected
    # Based on NixOS GNOME extension patterns
    
    if [ "$XDG_CURRENT_DESKTOP" = "GNOME" ] || echo "$XDG_CURRENT_DESKTOP" | grep -qi gnome; then
      echo "GNOME detected, attempting to activate Jasper extension..."
      
      # Check if extension is installed
      if [ -d "${jasperGnomeExtension}/share/gnome-shell/extensions/jasper@tom.local" ]; then
        echo "Extension found at system location"
        
        # Enable extension using gnome-extensions tool if available
        if command -v gnome-extensions >/dev/null; then
          gnome-extensions enable jasper@tom.local 2>/dev/null && \
            echo "Extension enabled successfully" || \
            echo "Extension enable failed (may need manual activation)"
        else
          echo "gnome-extensions command not available"
          echo "Extension can be enabled manually in GNOME Extensions app"
        fi
      else
        echo "Extension not found - installation may have failed"
      fi
    else
      echo "GNOME not detected, skipping extension activation"
    fi
  '';

  # Conditional D-Bus service file for cross-desktop notifications
  dbusServiceFile = pkgs.writeTextFile {
    name = "org.jasper.Companion.service";
    text = ''
      [D-Bus Service]
      Name=org.jasper.Companion
      Exec=${jasperDaemon}/bin/jasper-companion-daemon start
      SystemdService=jasper-companion.service
    '';
    destination = "/share/dbus-1/services/org.jasper.Companion.service";
  };

  # Desktop entry for GUI integration
  desktopEntry = pkgs.makeDesktopItem {
    name = "jasper-companion";
    desktopName = "Jasper AI Companion";
    comment = "Personal Digital Assistant with AI-generated insights";
    exec = "${jasperDaemon}/bin/jasper-companion-daemon start";
    icon = "calendar"; # Use system calendar icon as fallback
    categories = [ "Office" "Calendar" "Utility" ];
    terminal = false;
    type = "Application";
    startupNotify = true;
  };

in

symlinkJoin {
  name = "jasper-companion-unified";
  version = jasperDaemon.version or "0.2.0";
  
  paths = [
    jasperDaemon
    jasperGnomeExtension  # Always include, activation is conditional
    dbusServiceFile
    desktopEntry
  ];
  
  nativeBuildInputs = [ pkgs.makeWrapper ];
  
  # Post-build processing for unified package
  postBuild = ''
    # Create utility scripts directory
    mkdir -p $out/bin
    
    # Add desktop detection utility
    cp ${desktopDetectScript} $out/bin/jasper-detect-desktop
    chmod +x $out/bin/jasper-detect-desktop
    
    # Add GNOME extension activator
    cp ${gnomeExtensionActivator} $out/bin/jasper-gnome-extension-activate  
    chmod +x $out/bin/jasper-gnome-extension-activate
    
    # Create wrapper script for daemon with desktop detection
    makeWrapper ${jasperDaemon}/bin/jasper-companion-daemon \
      $out/bin/jasper-companion \
      --prefix PATH : ${lib.makeBinPath [ pkgs.gnome-shell pkgs.procps pkgs.which ]} \
      --set JASPER_UNIFIED_PACKAGE "1" \
      --set JASPER_EXTENSION_PATH "$out/share/gnome-shell/extensions/jasper@tom.local"
    
    # Ensure GNOME extension is properly linked
    if [ -d "${jasperGnomeExtension}/share/gnome-shell/extensions" ]; then
      echo "GNOME extension included in unified package"
      # The extension is already symlinked via symlinkJoin paths
    else
      echo "Warning: GNOME extension not found in component package"
    fi
    
    # Create configuration templates directory
    mkdir -p $out/share/jasper-companion/templates
    
    # Waybar configuration template
    cat > $out/share/jasper-companion/templates/waybar-jasper.json <<'EOF'
{
  "modules-left": [],
  "modules-center": [],
  "modules-right": ["custom/jasper"],
  
  "custom/jasper": {
    "exec": "jasper-companion waybar",
    "return-type": "json",
    "interval": 30,
    "restart-interval": 1,
    "tooltip": true,
    "on-click": "jasper-companion status"
  }
}
EOF
    
    # Create desktop integration info file
    cat > $out/share/jasper-companion/integration-info.txt <<EOF
Jasper Companion Unified Package
===============================

This package includes desktop environment detection and automatic integration.

Supported Frontends:
- GNOME Shell Extension (auto-activated on GNOME)
- Waybar Integration (JSON output)
- D-Bus Notifications (cross-desktop)
- Terminal Interface (always available)

Utilities:
- jasper-detect-desktop: Show desktop environment detection
- jasper-gnome-extension-activate: Manually activate GNOME extension
- jasper-companion: Main daemon wrapper with desktop detection

Configuration Templates:
- Waybar: $out/share/jasper-companion/templates/waybar-jasper.json

For support documentation, see: https://github.com/heytcass/jasper/docs/
EOF

    # Set up proper permissions
    chmod -R +r $out/share/jasper-companion/
  '';
  
  passthru = {
    # Expose component packages for advanced users
    daemon = jasperDaemon;
    gnomeExtension = jasperGnomeExtension;
    
    # Desktop detection capabilities
    supportsGnome = true;
    supportsWaybar = true;
    supportsKde = false; # Future enhancement
    
    # Extension UUID for NixOS module integration
    extensionUuid = jasperGnomeExtension.passthru.extensionUuid or "jasper@tom.local";
  };
  
  meta = with lib; {
    description = "Unified Jasper Companion with automatic desktop environment detection";
    longDescription = ''
      A unified package for Jasper Companion that automatically detects the desktop
      environment and enables appropriate integrations. Supports GNOME Shell extensions,
      Waybar status bar integration, and cross-desktop D-Bus notifications.
      
      Key features:
      - Automatic desktop environment detection
      - GNOME Shell extension with auto-activation
      - Waybar JSON output for tiling window managers
      - Cross-desktop notification support
      - Fallback terminal interface
      
      This package consolidates multiple Jasper components into a single installation
      that adapts to the user's desktop environment automatically.
    '';
    homepage = "https://github.com/heytcass/jasper";
    license = licenses.mit;
    maintainers = [ ];
    platforms = platforms.linux;
    
    # Categories for package discovery
    categories = [ "office" "productivity" "calendar" "ai" ];
  };
}