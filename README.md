# Jasper - Personal AI Assistant

> **📦 ARCHIVED (2026-07-11).** This daemon is retired; the repo is preserved as
> a donor archive. Jasper — the name, the voice, and the doctrine (one
> glanceable insight, significance-gated thinking, family-calendar ownership
> awareness) — lives on as a lane inside
> [claudeos](https://github.com/heytcass/claudeos): dumb collectors + one
> `claude -p` call + a Quickshell bar face, replacing the standalone Rust
> daemon, its D-Bus service, and all four frontends. See claudeos
> `docs/PHILOSOPHY.md` ("On Jasper specifically" — *take the thinking, not the
> daemon*) and `modules/apps/jasper.nix`.

Jasper is a proactive, intelligent companion for your desktop that leverages Claude Sonnet 4 to analyze your calendar and context data, providing personalized insights through multiple frontend options including a native GNOME Shell extension and Waybar integration.

## ✨ Features

### 🤖 AI-Powered Analysis
- **Smart Conflict Detection**: Identifies scheduling conflicts and overcommitted days
- **Context-Aware Insights**: Analyzes calendar patterns and suggests optimizations
- **Travel & Preparation Alerts**: Reminds you about travel time and event preparation
- **Urgency-Based Prioritization**: Ranks insights by importance and time sensitivity

### 📅 Calendar Integration  
- **Google Calendar Sync**: Real-time synchronization with multiple calendars
- **Multi-Calendar Support**: Handles personal, family, and work calendars simultaneously
- **Owner Recognition**: Identifies calendar owners for better coordination insights
- **OAuth2 Authentication**: Secure, industry-standard authentication

### 🔒 Privacy & Security
- **Data Sanitization**: Removes PII before AI analysis while preserving context
- **SOPS Integration**: Encrypted secret management for API keys
- **Configurable Privacy**: Control what information is analyzed
- **Local Processing**: Calendar data processed locally before AI analysis

### 🔔 Smart Desktop Notifications
- **Native D-Bus Integration**: Direct desktop notification system communication
- **Frontend-Agnostic**: Works seamlessly with GNOME 48, mako, dunst, and any freedesktop.org-compliant notification daemon
- **Rich Notifications**: Categories, urgency levels, and desktop integration hints
- **Auto-Detection**: Automatically selects best notification method (D-Bus or fallback)
- **Intelligent Delivery**: New AI insights trigger instant notifications
- **Configurable**: Fine-tune notification preferences and timing

### 🎨 Desktop Integration

#### GNOME Shell Extension (Primary)
- **Panel Integration**: Native GNOME Shell panel button with emoji indicators
- **Popup Menu**: Click to view full AI insights and manually refresh
- **Auto-Updates**: Refreshes insights automatically every 5 seconds
- **System Theming**: Follows GNOME Shell visual design
- **Notification Integration**: Seamless notification grouping in GNOME 48+

#### Waybar Module (Alternative)
- **Status Bar Display**: Clean, themed status bar integration
- **Visual Indicators**: Color-coded urgency levels and emoji icons
- **Rich Tooltips**: Detailed insights on hover
- **Manual Refresh**: Click to force refresh insights
- **Stylix Theming**: Automatically matches your system theme

### 🔧 Extensible Architecture
- **Modular Context Sources**: Obsidian notes, weather, tasks (planned)
- **D-Bus Interface**: Standard Linux IPC for frontend communication
- **Native Notifications**: Direct integration with desktop notification systems
- **Command Pattern**: Clean CLI interface with multiple operations
- **Service Layer**: Organized business logic for easy extension
- **Error Recovery**: Circuit breaker patterns and retry mechanisms

## 🏗️ Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Frontends     │    │  Rust Daemon     │    │  Data Sources   │
│                 │◄───┤                  │◄───┤                 │
│ • GNOME Shell   │    │ • AI Analysis    │    │ • Google Cal    │
│ • Waybar        │    │ • D-Bus Service  │    │ • Obsidian      │
│ • Notifications │    │ • Context Mgmt   │    │ • Weather       │
│ • CLI Tools     │    │ • Data Sanitize  │    │ • Tasks         │
│ • Future UIs    │    │ • Notify Engine  │    │ • Context APIs  │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                        │                        │
         ▼                        ▼                        ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   User Actions  │    │  Claude Sonnet 4 │    │ Configuration   │
│ • Click for menu│    │ • Pattern Analysis│   │ • TOML Config   │
│ • Manual refresh│    │ • Insights Gen   │    │ • SOPS Secrets  │
│ • Auto notify   │    │ • Context Aware  │    │ • OAuth Tokens  │
│ • Smart alerts  │    │ • Real-time Proc │    │ • Notify Prefs  │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

## 🚀 Quick Start

### Platform Support

Jasper supports multiple platforms:

- **Ubuntu/Debian**: Full support with standard build tools → See **[docs/UBUNTU_SETUP.md](docs/UBUNTU_SETUP.md)**
- **NixOS**: Declarative configuration with Nix flakes → See installation instructions below
- **Other Linux**: Should work with manual dependency installation

**For Ubuntu 25.11 users**: We recommend following the **[Ubuntu Setup Guide](docs/UBUNTU_SETUP.md)** for the smoothest installation experience.

### Prerequisites
- **Rust**: Latest stable version
- **Claude API Key**: From [Anthropic Console](https://console.anthropic.com)
- **Google Calendar API**: OAuth2 credentials from [Google Cloud Console](https://console.cloud.google.com)
- **GNOME Shell** (for extension) or **Waybar** (for status bar integration)

### Installation

#### Ubuntu/Debian Quick Start

```bash
# 1. Install dependencies
./ubuntu/install-deps.sh

# 2. Build and install
make build
sudo make install

# 3. Configure
jasper-companion-daemon set-api-key <your-anthropic-api-key>

# 4. Start
systemctl --user enable --now jasper-companion

# 5. Install GNOME extension (optional)
make install-extension
```

For complete Ubuntu setup instructions, see **[docs/UBUNTU_SETUP.md](docs/UBUNTU_SETUP.md)**.

#### NixOS / Manual Installation

1. **Clone and build**:
   ```bash
   git clone https://github.com/heytcass/jasper.git
   cd jasper
   cargo build --release
   ```

2. **Set up Claude API key**:
   ```bash
   ./target/release/jasper-companion-daemon set-api-key sk-ant-your-api-key-here
   ```

3. **Configure Google Calendar OAuth2**:
   Create `~/.config/jasper-companion/config.toml`:
   ```toml
   [google_calendar]
   enabled = true
   client_id = "your-client-id.apps.googleusercontent.com"
   client_secret = "your-client-secret"
   redirect_uri = "http://localhost:8080/auth/callback"
   calendar_ids = ["primary"]
   ```

4. **Authenticate with Google**:
   ```bash
   ./target/release/jasper-companion-daemon auth-google
   ```

5. **Test the system**:
   ```bash
   ./target/release/jasper-companion-daemon waybar
   ```

### Frontend Setup

#### Option 1: GNOME Shell Extension (Recommended)

For NixOS users, the extension is available as a Nix package:

```nix
# Add to your NixOS configuration
environment.systemPackages = [
  jasper.packages.x86_64-linux.gnome-extension
];
```

For manual installation:
1. Copy `gnome-extension/` contents to `~/.local/share/gnome-shell/extensions/jasper@tom.local/`
2. Restart GNOME Shell (Alt+F2, type `r`, press Enter)
3. Enable the extension: `gnome-extensions enable jasper@tom.local`
4. Click the panel icon to view AI insights

See [EXTENSION_DEVELOPMENT.md](EXTENSION_DEVELOPMENT.md) for detailed setup and development instructions.

#### Option 2: Waybar Integration

Add to your Waybar config (`~/.config/waybar/config`):
```json
{
  "modules-center": ["custom/jasper"],
  "custom/jasper": {
    "format": "{}",
    "tooltip": true,
    "interval": 900,
    "exec": "/path/to/jasper/tools/waybar-jasper.sh",
    "return-type": "json",
    "signal": 8,
    "on-click": "notify-send 'Jasper' 'Refreshing...' && pkill -RTMIN+8 waybar"
  }
}
```

Copy the provided styles to `~/.config/waybar/style.css` or reference the provided `waybar/style.css`.

## 📋 CLI Commands

```bash
# Authentication & Setup
jasper-companion-daemon auth-google          # Authenticate with Google Calendar
jasper-companion-daemon set-api-key KEY     # Set Claude API key

# Calendar Operations  
jasper-companion-daemon sync-test           # Test calendar synchronization
jasper-companion-daemon test-calendar       # Full calendar integration test
jasper-companion-daemon add-test-events     # Add demo events for testing

# Frontend Integration
jasper-companion-daemon waybar              # Output JSON for Waybar
jasper-companion-daemon start               # Start D-Bus daemon for GNOME extension

# Maintenance
jasper-companion-daemon clear-cache         # Clear AI cache and context state
jasper-companion-daemon clean-database      # Remove test data from database
jasper-companion-daemon test-notification   # Test notification system

# Daemon Management
jasper-companion-daemon status              # Check daemon status  
jasper-companion-daemon stop                # Stop daemon
```

## ⚙️ Configuration

### Basic Configuration
Jasper uses TOML configuration at `~/.config/jasper-companion/config.toml`:

```toml
[general]
planning_horizon_days = 7      # Days ahead to analyze
timezone = "America/New_York"  # Your timezone

[ai]
provider = "anthropic"
model = "claude-sonnet-4-5"
api_key = ""                   # Set via CLI command

[google_calendar]
enabled = true
client_id = "your-id.apps.googleusercontent.com"
client_secret = ""             # Or use SOPS
calendar_ids = ["primary", "work@company.com"]

[insights]
enable_travel_prep = true      # Travel preparation alerts
enable_overcommitment_warnings = true
high_urgency_days = 2          # Days ahead for high urgency
max_insights_per_day = 10

[notifications]
enabled = true                 # Enable desktop notifications
notify_new_insights = true     # Notify when AI generates new insights
notify_context_changes = false # Notify on context updates (less noisy)
notification_timeout = 5000    # Notification timeout in milliseconds
preferred_method = "auto"      # auto, dbus, notify-send
app_name = "Jasper"           # Application name for notifications

[privacy]
sanitize_pii = true           # Remove personal info before AI
log_sanitized_data = false    # Debug sanitization
```

### Context Sources (Extensible)
```toml
[context_sources.obsidian]
enabled = true
vault_path = "~/Documents/Obsidian Vault"
daily_notes_folder = "Daily"
parse_tasks = true

[context_sources.weather]  
enabled = true
location = "New York, NY"
api_key = ""                  # OpenWeatherMap API key

[context_sources.tasks]
enabled = false               # Planned: Todoist integration
```

### SOPS Secret Management
For production deployments, use SOPS for encrypted secrets:

```yaml
# ~/.nixos/secrets/secrets.yaml (encrypted)
anthropic_api_key: sk-ant-your-key
google_client_secret: your-secret
openweather_api_key: your-key
```

## 🔧 Advanced Usage

### NixOS Integration
Jasper includes full NixOS module support:

```nix
# configuration.nix or home.nix
{
  programs.waybar.jasper.enable = true;
  services.jasper-companion-daemon = {
    enable = true;
    user = "youruser";
    configFile = ./jasper-config.toml;
  };
  
  # Add GNOME extension
  environment.systemPackages = [
    jasper.packages.x86_64-linux.gnome-extension
  ];
}
```

### Development Mode
For rapid development and testing:

#### Backend Development
```bash
./tools/dev-mode.sh start          # Enter development mode
./tools/quick-test.sh full         # Build + test + reload Waybar
./tools/quick-test.sh css          # Live CSS editing
./tools/dev-mode.sh stop           # Exit development mode
```

#### Extension Development
```bash
./tools/extension-dev.sh status    # Check extension status
./tools/extension-dev.sh install   # Install development extension
./tools/extension-dev.sh uninstall # Remove development extension
```

See [docs/EXTENSION_DEVELOPMENT.md](docs/EXTENSION_DEVELOPMENT.md) for comprehensive extension development guide.

## 🐛 Troubleshooting

### Common Issues

**Authentication Errors:**
```bash
jasper-companion-daemon auth-google  # Re-authenticate
```

**No Insights Displayed:**
```bash
jasper-companion-daemon test-calendar    # Verify calendar sync
jasper-companion-daemon clear-cache      # Clear AI cache
```

**GNOME Extension Not Working:**
```bash
gnome-extensions enable jasper@tom.local # Enable extension
journalctl --user -f | grep Jasper      # Check logs
systemctl --user restart jasper-companion-daemon # Restart daemon
```

**Waybar Not Updating:**
```bash
pkill -RTMIN+8 waybar       # Force refresh signal
./tools/waybar-jasper.sh          # Test output directly
```

### Debug Mode
Enable debug logging:
```bash
jasper-companion-daemon --debug waybar
journalctl --user -u jasper-companion-daemon -f
```

## 🤝 Contributing

Jasper uses a specialized development workflow optimized for NixOS environments:

- **Backend Development**: See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for Rust daemon development
- **Extension Development**: See [docs/EXTENSION_DEVELOPMENT.md](docs/EXTENSION_DEVELOPMENT.md) for GNOME Shell extension work
- **Lessons Learned**: See [docs/architecture/EXTENSION_LESSONS_LEARNED.md](docs/architecture/EXTENSION_LESSONS_LEARNED.md) for development insights

### Quick Contributor Setup
```bash
git clone https://github.com/heytcass/jasper.git
cd jasper
nix develop                  # Enter development shell

# For daemon development
./tools/dev-mode.sh start         # Start development mode
cargo test                  # Run tests

# For extension development  
./tools/extension-dev.sh install    # Install development extension
```

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- **Anthropic Claude**: AI analysis engine
- **Google Calendar API**: Calendar integration
- **GNOME Shell**: Native desktop integration platform
- **Waybar**: Desktop status bar framework
- **NixOS**: Reproducible system configuration
- **Stylix**: System theming integration