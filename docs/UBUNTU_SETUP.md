# Jasper Companion - Ubuntu 25.11 Setup Guide

This guide covers installation and setup of Jasper Companion on Ubuntu 25.11 (and other Debian-based systems).

## Overview

Jasper Companion is an AI-powered personal assistant that integrates with:
- **GNOME Shell** (panel extension)
- **Waybar** (for tiling window managers)

The system consists of:
1. **Rust Daemon** - Backend service that runs continuously
2. **D-Bus Service** - Inter-process communication
3. **GNOME Extension** (optional) - Panel UI for GNOME
4. **Waybar Module** (optional) - Status bar integration

## Quick Start

```bash
# 1. Install dependencies
./ubuntu/install-deps.sh

# 2. Build the daemon
make build

# 3. Install system-wide
sudo make install

# 4. Configure API keys
jasper-companion-daemon set-api-key <your-anthropic-api-key>

# 5. Start the daemon
systemctl --user enable --now jasper-companion

# 6. Install GNOME extension (if using GNOME)
make install-extension
```

## System Requirements

- **OS**: Ubuntu 25.11 or later (or Debian-based distro)
- **Desktop**: GNOME Shell 47+ or Waybar-compatible WM
- **Rust**: 1.70+ (installed automatically)
- **Storage**: ~100MB for daemon + database
- **Network**: Internet access for AI API calls

## Installation Steps

### 1. Install System Dependencies

```bash
# Install build dependencies
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  libsqlite3-dev \
  libdbus-1-dev \
  curl \
  git

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

For **GNOME** users:
```bash
sudo apt-get install -y gnome-shell-extensions
```

For **Waybar** users:
```bash
sudo apt-get install -y waybar
```

### 2. Build the Daemon

```bash
# Clone repository (if you haven't already)
cd ~/jasper

# Build the daemon
make build

# This compiles the Rust daemon with optimizations
# Binary will be in target/release/jasper-companion-daemon
```

### 3. Install System-Wide

```bash
# Install daemon, D-Bus service, and systemd unit
sudo make install

# This installs:
# - /usr/local/bin/jasper-companion-daemon (daemon binary)
# - /usr/share/dbus-1/services/org.jasper.Daemon.service (D-Bus service)
# - /etc/systemd/user/jasper-companion.service (systemd user service)
```

### 4. Configure Jasper

#### Set API Key

```bash
# Set Anthropic API key
jasper-companion-daemon set-api-key <your-anthropic-api-key>

# Or set as environment variable
export ANTHROPIC_API_KEY="sk-ant-..."
```

#### Configure Google Calendar (Optional)

```bash
# Edit configuration file
nano ~/.config/jasper-companion/config.toml
```

Add your Google Calendar credentials:
```toml
[google_calendar]
enabled = true
client_id = "your-client-id.apps.googleusercontent.com"
client_secret = "your-client-secret"
calendar_ids = ["primary"]
```

Then authenticate:
```bash
jasper-companion-daemon auth google login
```

#### Configuration File Reference

Default location: `~/.config/jasper-companion/config.toml`

```toml
[general]
planning_horizon_days = 7
analysis_interval = 30  # minutes
timezone = "America/New_York"

[ai]
provider = "anthropic"
model = "claude-sonnet-4-5"
api_key = ""  # or use ANTHROPIC_API_KEY env var

[google_calendar]
enabled = true
client_id = ""
client_secret = ""
calendar_ids = ["primary"]

[obsidian]
enabled = false
vault_path = ""

[notifications]
enabled = true
notify_new_insights = true
preferred_method = "auto"  # auto, dbus, notify-send
```

### 5. Start the Daemon

```bash
# Enable and start the service
systemctl --user enable jasper-companion
systemctl --user start jasper-companion

# Check status
systemctl --user status jasper-companion

# View logs
journalctl --user -u jasper-companion -f
```

### 6. Install Frontend (Choose One)

#### Option A: GNOME Shell Extension

```bash
# Install extension
make install-extension

# Restart GNOME Shell
# - X11: Press Alt+F2, type 'r', press Enter
# - Wayland: Log out and log back in

# Enable extension
gnome-extensions enable jasper-companion@heytcass.github

# Check status
gnome-extensions info jasper-companion@heytcass.github
```

The extension will appear in your top panel as an emoji indicator.

#### Option B: Waybar Integration

Add to your `~/.config/waybar/config`:

```json
{
  "modules-right": ["custom/jasper", "..."],

  "custom/jasper": {
    "exec": "/usr/local/bin/jasper-companion-daemon waybar",
    "return-type": "json",
    "interval": 900,
    "signal": 8,
    "format": "{icon} {text}",
    "format-icons": {
      "urgent": "🔴",
      "warning": "🟡",
      "normal": "🟢"
    },
    "on-click": "pkill -SIGRTMIN+8 waybar"
  }
}
```

Add to your `~/.config/waybar/style.css`:

```css
#custom-jasper {
  padding: 0 10px;
  font-family: "Noto Sans";
}

#custom-jasper.urgent {
  color: #ff6b6b;
  animation: blink 1s linear infinite;
}

@keyframes blink {
  50% { opacity: 0.5; }
}
```

Restart Waybar:
```bash
killall waybar && waybar &
```

## Usage

### CLI Commands

```bash
# Start daemon (usually via systemd)
jasper-companion-daemon start

# Check daemon status
jasper-companion-daemon status

# Get Waybar JSON output
jasper-companion-daemon waybar

# Set API key
jasper-companion-daemon set-api-key <key>

# Google Calendar authentication
jasper-companion-daemon auth google login
jasper-companion-daemon auth google test
jasper-companion-daemon auth google logout

# View current insight
jasper-companion-daemon get-insight
```

### GNOME Extension Usage

- **Panel Icon**: Shows emoji indicator of current status
- **Click Icon**: Opens popup menu with latest insights
- **Auto-Refresh**: Updates every 5 seconds
- **Notifications**: Desktop notifications for new insights (optional)

### Waybar Module Usage

- **Status Display**: Shows emoji + preview text
- **Click Action**: Triggers manual refresh
- **Color Coding**: Urgent (red), warning (yellow), normal (green)
- **Auto-Update**: Every 15 minutes (configurable)

## Development Setup

### Build for Development

```bash
# Build in debug mode
make dev

# Run directly (without systemd)
cargo run -- start

# Run with verbose logging
RUST_LOG=debug cargo run -- start

# Run tests
make test
```

### Development Scripts

The original NixOS development scripts are in `tools/` but are not compatible with Ubuntu. For Ubuntu development:

```bash
# Build and test
make build
make test

# Install locally for testing
sudo make install

# Restart daemon after changes
systemctl --user restart jasper-companion

# Watch logs
journalctl --user -u jasper-companion -f

# For extension development
make install-extension
# Then restart GNOME Shell (Alt+F2, 'r' on X11)
```

### Extension Development

```bash
# Edit extension code
nano gnome-extension/extension.js

# Reinstall extension
make install-extension

# Restart GNOME Shell
# X11: Alt+F2, type 'r', Enter
# Wayland: Log out/in

# View extension logs
journalctl -f | grep -i jasper
# Or check systemd journal
journalctl --user -xe | grep -i jasper
```

## Troubleshooting

### Daemon Won't Start

```bash
# Check systemd status
systemctl --user status jasper-companion

# View detailed logs
journalctl --user -u jasper-companion -n 50

# Common issues:
# - Missing API key: jasper-companion-daemon set-api-key <key>
# - Configuration error: check ~/.config/jasper-companion/config.toml
# - D-Bus issues: check dbus-daemon is running
```

### GNOME Extension Not Visible

```bash
# Check extension status
gnome-extensions list
gnome-extensions info jasper-companion@heytcass.github

# Enable if disabled
gnome-extensions enable jasper-companion@heytcass.github

# Check installation location
ls -la ~/.local/share/gnome-shell/extensions/jasper-companion@heytcass.github/

# View extension errors
journalctl -f /usr/bin/gnome-shell
```

### Waybar Module Not Updating

```bash
# Test JSON output manually
/usr/local/bin/jasper-companion-daemon waybar

# Check Waybar logs
journalctl --user -u waybar -f

# Verify module configuration in ~/.config/waybar/config

# Manual refresh
pkill -SIGRTMIN+8 waybar
```

### D-Bus Communication Issues

```bash
# Check D-Bus service is registered
busctl --user list | grep jasper

# Test D-Bus interface
busctl --user call org.jasper.Daemon \
  /org/jasper/Daemon \
  org.jasper.Daemon1 \
  GetLatestInsight

# Monitor D-Bus signals
dbus-monitor --session "interface='org.jasper.Daemon1'"
```

### Database Issues

```bash
# Database location
ls -lh ~/.local/share/jasper-companion/app_data.db

# Check database integrity
sqlite3 ~/.local/share/jasper-companion/app_data.db "PRAGMA integrity_check;"

# Reset database (WARNING: deletes all data)
rm ~/.local/share/jasper-companion/app_data.db
systemctl --user restart jasper-companion
```

## Uninstallation

```bash
# Stop and disable service
systemctl --user stop jasper-companion
systemctl --user disable jasper-companion

# Remove installed files
sudo make uninstall

# Remove extension
make uninstall-extension

# Remove user data (optional)
rm -rf ~/.config/jasper-companion-daemon
rm -rf ~/.local/share/jasper-companion

# Remove Rust toolchain (optional)
rustup self uninstall
```

## Architecture Notes

### File Locations

| Component | Location |
|-----------|----------|
| Daemon binary | `/usr/local/bin/jasper-companion-daemon` |
| D-Bus service | `/usr/share/dbus-1/services/org.jasper.Daemon.service` |
| systemd service | `/etc/systemd/user/jasper-companion.service` |
| GNOME extension | `~/.local/share/gnome-shell/extensions/jasper-companion@heytcass.github/` |
| Configuration | `~/.config/jasper-companion/config.toml` |
| Database | `~/.local/share/jasper-companion/app_data.db` |
| Secrets | `~/.config/jasper-companion/encrypted_secrets.age` |
| Logs | `journalctl --user -u jasper-companion` |

### D-Bus Interface

Service: `org.jasper.Daemon`
Object: `/org/jasper/Daemon`
Interface: `org.jasper.Daemon1`

Methods:
- `GetLatestInsight() → (i64, s, s, s)` - Returns (id, emoji, preview, full_text)
- `GetInsightById(i64) → (i64, s, s, s)` - Get specific insight
- `RegisterFrontend(s, u) → b` - Register frontend (id, pid)
- `Heartbeat(s) → b` - Frontend heartbeat

Signals:
- `InsightUpdated(i64, s, s)` - New insight available (id, emoji, preview)

## Differences from NixOS

On NixOS, Jasper used:
- Nix flakes for build management
- NixOS modules for system integration
- Declarative configuration in `configuration.nix`
- Automatic desktop environment detection
- Unified package with auto-configuration

On Ubuntu:
- Standard `make` build system
- Manual systemd service management
- Configuration via `~/.config/jasper-companion/config.toml`
- Manual frontend selection (GNOME or Waybar)
- Separate installation steps

## Getting Help

- **Issues**: https://github.com/heytcass/jasper/issues
- **Documentation**: `docs/` directory
- **Development Guide**: `docs/DEVELOPMENT.md` (NixOS-focused, adapt for Ubuntu)
- **Extension Guide**: `docs/EXTENSION_DEVELOPMENT.md`

## Next Steps

After installation:
1. Configure your preferred context sources (Google Calendar, Obsidian)
2. Customize analysis intervals in `config.toml`
3. Set up desktop notifications if desired
4. Explore CLI commands for direct access
5. Consider contributing Ubuntu-specific improvements

For advanced configuration and development, see:
- `docs/DEVELOPMENT.md` - Architecture and code structure
- `README.md` - General project overview
- `waybar/README.md` - Waybar integration details
