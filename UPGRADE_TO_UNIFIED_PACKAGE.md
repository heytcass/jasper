# Upgrade to Unified Jasper Package

## Changes Made to Your NixOS Configuration

I've updated your NixOS configuration to use the new unified Jasper package with automatic desktop environment detection. Here's what was changed:

### 1. Updated `/home/tom/.nixos/systems/gti/default.nix`

**Added Unified Module Import:**
```nix
# Import Jasper unified module with auto-detection
jasper.nixosModules.unified
```

**Enhanced Package Overlay:**
```nix
nixpkgs.overlays = [
  (final: prev: {
    # Keep dev extension for development workflow
    jasper-gnome-extension-dev = jasper.packages.${prev.system}.gnome-extension-dev;
    # Use unified package as default
    jasper-companion-unified = jasper.packages.${prev.system}.default;
    # Individual components still available if needed
    jasper-companion = jasper.packages.${prev.system}.daemon;
    jasper-gnome-extension = jasper.packages.${prev.system}.gnome-extension;
  })
];
```

**Added Unified Service Configuration:**
```nix
# Configure Jasper Companion with unified package
services.jasperCompanion = {
  enable = true;
  user = "tom";
  package = pkgs.jasper-companion-unified;
  
  # Enable automatic desktop environment detection
  autoDetectDesktop = true;
  
  # Optional: Force specific frontends (leave empty for auto-detection)
  # forceEnableFrontends = [ "gnome" "waybar" ];
  
  # Optional: Additional daemon configuration
  extraConfig = {
    # Custom environment variables can be added here if needed
  };
};
```

### 2. Updated `/home/tom/.nixos/modules/desktop.nix`

**Removed Manual Package Installation:**
- Removed `pkgs.jasper-companion` from `gnomePackages` (now handled by service)
- Kept `pkgs.jasper-gnome-extension-dev` for development workflow

### 3. Updated `/home/tom/projects/jasper/flake.nix`

**Added Unified Module Export:**
```nix
nixosModules = {
  # Legacy module (kept for backward compatibility)
  default = import ./nix/module.nix;
  # New unified module with auto-detection
  unified = import ./nix/unified-module.nix;
};
```

## What the Unified Package Provides

After rebuilding, you'll get:

✅ **Enhanced Daemon** with desktop environment detection  
✅ **Automatic GNOME Extension** activation (production version)  
✅ **Desktop Detection Utilities** (`jasper-detect-desktop`)  
✅ **GNOME Extension Activator** (`jasper-gnome-extension-activate`)  
✅ **Unified Wrapper** (`jasper-companion`) with auto-detection  
✅ **Cross-Desktop D-Bus Integration**  
✅ **Configuration Templates** for different environments  

## How to Apply the Changes

### Step 1: Commit Jasper Changes
```bash
cd /home/tom/projects/jasper
git add -A
git commit -m "Implement unified package with desktop environment detection

- Add desktop detection module with fallback strategies
- Create unified package structure with conditional components  
- Add enhanced NixOS module with auto-detection
- Update flake to export unified module and packages

🤖 Generated with Claude Code

Co-Authored-By: Claude <noreply@anthropic.com>"
```

### Step 2: Commit NixOS Configuration Changes
```bash
cd /home/tom/.nixos
git add -A
git commit -m "Upgrade Jasper to unified package with auto-detection

- Replace manual package installation with unified service
- Enable automatic desktop environment detection
- Configure GNOME extension auto-activation
- Keep development extension for workflow compatibility

🤖 Generated with Claude Code

Co-Authored-By: Claude <noreply@anthropic.com>"
```

### Step 3: Rebuild Your System
```bash
cd /home/tom/.nixos
sudo nixos-rebuild switch --flake .#gti
```

## Expected Results After Rebuild

### New System Binaries
```bash
# These will be available after rebuild:
/run/current-system/sw/bin/jasper-companion           # Unified wrapper with auto-detection
/run/current-system/sw/bin/jasper-detect-desktop      # Desktop environment detection utility
/run/current-system/sw/bin/jasper-gnome-extension-activate  # GNOME extension activator
```

### Automatic Services
- **Jasper Companion Service**: `systemctl --user status jasper-companion`
- **GNOME Extension**: Auto-enabled in GNOME Shell panel
- **D-Bus Integration**: Cross-desktop notifications working

### Verification Commands
```bash
# Test desktop detection
jasper-detect-desktop

# Check service status  
systemctl --user status jasper-companion

# Test functionality
jasper-companion waybar
jasper-companion detect-desktop

# Check GNOME extension
gnome-extensions list | grep jasper
```

## Rollback Instructions (If Needed)

If you need to rollback to the previous configuration:

```bash
cd /home/tom/.nixos
git log --oneline -5  # Find the commit before the changes
git checkout <previous-commit-hash>
sudo nixos-rebuild switch --flake .#gti
```

## Development Workflow

Your development workflow remains unchanged:
- Development extension (`jasper-dev-v3@tom.local`) still available
- `./tools/dev-mode.sh` and `./tools/extension-dev.sh` work as before
- Local development builds in `/home/tom/projects/jasper/target/debug/`

The unified package adds production capabilities while preserving development tools.

## Troubleshooting

### If GNOME Extension Doesn't Appear
```bash
# Check if extension is installed
ls -la /run/current-system/sw/share/gnome-shell/extensions/ | grep jasper

# Manually activate if needed
jasper-gnome-extension-activate

# Check extension status
gnome-extensions list | grep jasper
gnome-extensions info jasper@tom.local
```

### If Desktop Detection Isn't Working
```bash
# Run diagnostic
jasper-detect-desktop

# Check logs
journalctl --user -u jasper-companion -f
```

### If Service Won't Start
```bash
# Check service status
systemctl --user status jasper-companion

# View logs
journalctl --user -u jasper-companion -n 50
```

## Benefits You'll Get

1. **Simplified Management**: Single service configuration instead of manual packages
2. **Automatic Integration**: GNOME extension and D-Bus automatically configured  
3. **Desktop Awareness**: System adapts to your desktop environment automatically
4. **Enhanced Debugging**: New diagnostic utilities for troubleshooting
5. **Future-Proof**: Ready for additional desktop environment support

The unified package maintains all existing functionality while adding intelligent desktop integration!