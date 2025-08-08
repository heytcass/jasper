# Jasper - Personal AI Assistant

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

### 🎨 Desktop Integration

#### GNOME Shell Extension (Primary)
- **Panel Integration**: Native GNOME Shell panel button with emoji indicators
- **Popup Menu**: Click to view full AI insights and manually refresh
- **Auto-Updates**: Refreshes insights automatically every 5 seconds
- **System Theming**: Follows GNOME Shell visual design

#### Waybar Module (Alternative)
- **Status Bar Display**: Clean, themed status bar integration
- **Visual Indicators**: Color-coded urgency levels and emoji icons
- **Rich Tooltips**: Detailed insights on hover
- **Manual Refresh**: Click to force refresh insights
- **Stylix Theming**: Automatically matches your system theme

### 🔧 Extensible Architecture
- **Modular Context Sources**: Obsidian notes, weather, tasks (planned)
- **D-Bus Interface**: Standard Linux IPC for frontend communication
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
│ • CLI Tools     │    │ • Context Mgmt   │    │ • Weather       │
│ • Future UIs    │    │ • Data Sanitize  │    │ • Tasks         │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                        │                        │
         ▼                        ▼                        ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   User Actions  │    │  Claude Sonnet 4 │    │ Configuration   │
│ • Click for menu│    │ • Pattern Analysis│   │ • TOML Config   │
│ • Manual refresh│    │ • Insights Gen   │    │ • SOPS Secrets  │
│ • Notifications │    │ • Context Aware  │    │ • OAuth Tokens  │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

## 🚀 Quick Start

### Prerequisites
- **Rust**: Latest stable version
- **Claude API Key**: From [Anthropic Console](https://console.anthropic.com)
- **Google Calendar API**: OAuth2 credentials from [Google Cloud Console](https://console.cloud.google.com)
- **GNOME Shell** (for extension) or **Waybar** (for status bar integration)

### Installation

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
    "exec": "/path/to/jasper/waybar-jasper.sh",
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
model = "claude-sonnet-4-20250514" 
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
  services.jasper-companion = {
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
./dev-mode.sh start          # Enter development mode
./quick-test.sh full         # Build + test + reload Waybar
./quick-test.sh css          # Live CSS editing
./dev-mode.sh stop           # Exit development mode
```

#### Extension Development
```bash
./scripts/extension-dev.sh status    # Check extension status
./scripts/extension-dev.sh install   # Install development extension
./scripts/extension-dev.sh uninstall # Remove development extension
```

See [EXTENSION_DEVELOPMENT.md](EXTENSION_DEVELOPMENT.md) for comprehensive extension development guide.

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
./waybar-jasper.sh          # Test output directly
```

### Debug Mode
Enable debug logging:
```bash
jasper-companion-daemon --debug waybar
journalctl --user -u jasper-companion-daemon -f
```

## 🤝 Contributing

Jasper uses a specialized development workflow optimized for NixOS environments:

- **Backend Development**: See [DEVELOPMENT.md](DEVELOPMENT.md) for Rust daemon development
- **Extension Development**: See [EXTENSION_DEVELOPMENT.md](EXTENSION_DEVELOPMENT.md) for GNOME Shell extension work
- **Lessons Learned**: See [docs/EXTENSION_LESSONS_LEARNED.md](docs/EXTENSION_LESSONS_LEARNED.md) for development insights

### Quick Contributor Setup
```bash
git clone https://github.com/heytcass/jasper.git
cd jasper
nix develop                  # Enter development shell

# For daemon development
./dev-mode.sh start         # Start development mode
cargo test                  # Run tests

# For extension development  
./scripts/extension-dev.sh install    # Install development extension
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