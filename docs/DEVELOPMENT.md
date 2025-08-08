# Jasper Development Guide

This guide covers the development workflow, architecture, and contribution guidelines for Jasper.

## Overview

Jasper is a personal AI assistant that analyzes calendar data using Claude Sonnet 4 and displays insights through Waybar. The project features:

- **Rust Backend**: High-performance daemon with modular architecture
- **AI Integration**: Claude Sonnet 4 for intelligent calendar analysis
- **Desktop Integration**: Waybar module with theming support
- **NixOS Optimization**: Specialized development workflow for fast iteration

## Project Structure

```
jasper/
├── daemon/src/                 # Rust daemon source
│   ├── main.rs                # CLI entry point
│   ├── commands/              # Command pattern implementation
│   │   ├── auth.rs           # Authentication commands
│   │   ├── calendar.rs       # Calendar operations
│   │   ├── daemon_ops.rs     # Daemon management
│   │   └── waybar.rs         # Waybar output
│   ├── services/             # Business logic layer
│   │   ├── companion.rs      # Main service orchestration
│   │   ├── calendar.rs       # Calendar service
│   │   ├── insight.rs        # AI analysis service
│   │   └── notification.rs   # Notification system
│   ├── context_sources/      # Pluggable data sources
│   │   ├── calendar.rs       # Google Calendar integration
│   │   ├── obsidian.rs       # Obsidian vault parsing
│   │   ├── weather.rs        # Weather context
│   │   └── tasks.rs          # Task management
│   ├── correlation_engine.rs # AI analysis orchestration
│   ├── waybar_formatter.rs   # JSON output formatting
│   ├── data_sanitizer.rs     # PII removal for privacy
│   ├── google_calendar.rs    # Google Calendar API client
│   ├── database.rs           # SQLite database layer
│   ├── config.rs             # Configuration management
│   ├── sops_integration.rs   # Encrypted secrets management
│   └── errors.rs             # Structured error handling
├── waybar/                   # Desktop integration
│   ├── config.json          # Waybar module configuration
│   ├── style.css            # CSS with Stylix theming
│   └── README.md            # Waybar setup guide
├── dev-mode.sh              # Development environment manager
├── quick-test.sh            # Fast iteration testing
├── waybar-jasper.sh         # Production wrapper script
├── flake.nix               # Nix development environment
├── module.nix              # NixOS module definition
└── Cargo.toml              # Rust dependencies
```

## Development Workflow

### 1. Entering Development Mode

```bash
# Start development mode (backs up NixOS configs, installs dev configs)
./dev-mode.sh start

# Check status
./dev-mode.sh status
```

**What this does:**
- Backs up NixOS-managed waybar configs 
- Copies `waybar/config.json` → `~/.config/waybar/config`
- Copies `waybar/style.css` → `~/.config/waybar/style.css`
- Builds the project with `cargo build`
- Restarts waybar with development config
- Enables fast iteration without NixOS rebuilds

### 2. Development Iteration Patterns

#### A. Rust Code Changes
```bash
# Make changes to daemon/src/*.rs
# Build and test
cargo build
./quick-test.sh test
```

#### B. CSS Style Changes
```bash
# Edit waybar/style.css
# Quick reload
./quick-test.sh reload
# OR use live editing mode
./quick-test.sh css  # Auto-reloads on file changes
```

#### C. Waybar Config Changes
```bash
# Edit waybar/config.json
# Reload waybar
./quick-test.sh reload
```

#### D. Full Test Cycle
```bash
# Build + test + reload in one command
./quick-test.sh full
```

### 3. Testing Commands

```bash
# Test JSON output
./waybar-jasper.sh

# Test simple output
./waybar-jasper.sh --simple

# Quick build and test
./quick-test.sh test

# View build status
./quick-test.sh status
```

### 4. Exiting Development Mode

```bash
# Restore NixOS-managed configs
./dev-mode.sh stop
```

**What this does:**
- Restores original waybar configs from `.dev-backups/`
- Restarts waybar with NixOS configuration
- Removes `.dev-backups/.dev-mode-active` flag

## File Structure for AI Agents

### Key Files to Modify During Development

1. **Rust Code** (`daemon/src/`)
   - `waybar_formatter.rs` - JSON output formatting logic
   - `main.rs` - CLI interface and waybar command
   - `correlation_engine.rs` - AI insight generation

2. **Development Configs** (`waybar/`)
   - `config.json` - Waybar module configuration
   - `style.css` - Styling with Stylix variables

3. **Scripts** (root directory)
   - `waybar-jasper.sh` - Production wrapper script
   - `dev-mode.sh` - Development mode management
   - `quick-test.sh` - Fast iteration testing

### Files to NOT Modify

1. **NixOS Configs** (`/home/tom/.nixos/`)
   - These are production configs
   - Only update after development is complete

2. **System Waybar Configs** (`~/.config/waybar/`)
   - These are overridden during development mode
   - Automatically managed by dev-mode.sh

## Common Development Tasks

### Adding New Urgency Levels

1. **Update Rust enum** in `waybar_formatter.rs`:
```rust
fn get_urgency_styling(&self, urgency: i32) -> (&'static str, &'static str) {
    match urgency {
        9..=10 => ("🚨", "critical"),
        7..=8 => ("⚠️", "warning"),
        5..=6 => ("💡", "info"),
        3..=4 => ("📝", "low"),
        1..=2 => ("📋", "minimal"),
        _ => ("📅", "clear"),
    }
}
```

2. **Add CSS styling** in `waybar/style.css`:
```css
#custom-jasper.new-level {
  border-color: @base0X;
  color: @base0X;
}
```

3. **Test changes**:
```bash
cargo build
./quick-test.sh full
```

### Modifying Tooltip Content

1. **Edit `create_tooltip()`** in `waybar_formatter.rs`
2. **Test immediately**:
```bash
./quick-test.sh test
```

### Changing Refresh Intervals

1. **Edit `waybar/config.json`**:
```json
"custom/jasper": {
  "interval": 180,  // 3 minutes instead of 5
  ...
}
```

2. **Reload waybar**:
```bash
./quick-test.sh reload
```

## Stylix Integration

All CSS uses Stylix variables for theming:
- `@base00` to `@base0F` - Base16 color palette
- `@font-family` - System font family
- `@font-size` - System font size

**Example usage:**
```css
#custom-jasper.critical {
  border-color: @base08;  /* Red */
  color: @base08;
}

#custom-jasper.warning {
  border-color: @base0A;  /* Yellow */
  color: @base0A;
}
```

## Error Handling

### Common Issues and Solutions

1. **"Not in development mode"**
   - Run `./dev-mode.sh start` first

2. **Waybar not updating**
   - Run `./quick-test.sh reload`

3. **JSON parsing errors**
   - Run `./quick-test.sh test` to see raw output
   - Check for syntax errors in waybar_formatter.rs

4. **Build failures**
   - Run `cargo build` directly to see errors
   - Check for missing dependencies

### Debug Commands

```bash
# Check current mode
./dev-mode.sh status

# Test raw output
./waybar-jasper.sh | jq .

# View waybar logs
journalctl -f -u waybar

# Check if waybar is running
pgrep waybar
```

## Production Integration

When development is complete:

1. **Exit development mode**:
```bash
./dev-mode.sh stop
```

2. **Copy configs to NixOS** (manual process):
   - `waybar/config.json` → `/home/tom/.nixos/home/tom/home.nix` (waybar settings)
   - `waybar/style.css` → `/home/tom/.nixos/home/tom/home.nix` (waybar style)

3. **Rebuild system**:
```bash
cd /home/tom/.nixos
sudo nixos-rebuild switch
```

## AI Agent Instructions

### For Code Changes:
1. Always check if in development mode: `./dev-mode.sh status`
2. If not in dev mode, run: `./dev-mode.sh start`
3. Make changes to appropriate files
4. Test with: `./quick-test.sh full`
5. Iterate until satisfied

### For CSS/Config Changes:
1. Edit `waybar/style.css` or `waybar/config.json`
2. Reload with: `./quick-test.sh reload`
3. Test appearance in waybar

### For Final Integration:
1. Exit development mode: `./dev-mode.sh stop`
2. Document changes for user to integrate into NixOS

This workflow ensures fast iteration while maintaining the integrity of the production NixOS configuration.

## Architecture Deep Dive

### Command Pattern Implementation

Jasper uses the Command pattern for clean CLI interface organization:

```rust
// daemon/src/commands/mod.rs
pub trait Command {
    async fn execute(&mut self, context: &CommandContext) -> Result<()>;
}

pub struct CommandContext {
    pub config: Arc<RwLock<Config>>,
    pub database: Database, 
    pub correlation_engine: CorrelationEngine,
    pub debug: bool,
    pub test_mode: bool,
}
```

**Command Categories:**
- `auth.rs`: Authentication (Google OAuth2, API keys)
- `calendar.rs`: Calendar operations (sync, test, demo data)
- `daemon_ops.rs`: Daemon management (start, status, stop)
- `waybar.rs`: Waybar output formatting

### Service Layer Architecture

Business logic is organized into services for maintainability:

```rust
// daemon/src/services/
companion.rs     // Main orchestration service
calendar.rs      // Google Calendar operations  
insight.rs       // AI analysis coordination
notification.rs  // Desktop notification system
```

### Context Sources (Plugin System)

Jasper has an extensible context source system:

```rust
pub trait ContextSource: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    async fn fetch_context(&self, start: DateTime<Utc>, end: DateTime<Utc>) 
        -> Result<ContextData>;
    fn is_enabled(&self) -> bool;
}
```

**Implemented Sources:**
- **Calendar**: Google Calendar API integration
- **Obsidian**: Markdown vault parsing with task extraction
- **Weather**: OpenWeatherMap API integration  
- **Tasks**: Todoist integration (planned)

### Data Flow

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│ Context Sources │───▶│ Data Sanitizer  │───▶│ Claude Sonnet 4 │
│ (Calendar, etc) │    │ (PII Removal)   │    │ (AI Analysis)   │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│ SQLite Database │    │ Privacy Config  │    │ Insights JSON   │
│ (Event Storage) │    │ (PII Patterns)  │    │ (Waybar Output) │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

### Privacy & Security

1. **Data Sanitization**: `data_sanitizer.rs` removes PII using regex patterns
2. **SOPS Integration**: Encrypted secrets management for production
3. **OAuth2 Flow**: Standard Google Calendar authentication
4. **Local Processing**: All data processing happens locally

## Configuration Architecture

### Hierarchical Config System

```toml
# ~/.config/jasper-companion/config.toml
[general]        # Global settings
[ai]             # AI provider configuration  
[google_calendar] # Calendar integration
[insights]       # Analysis preferences
[privacy]        # Data sanitization
[context_sources] # Plugin configurations
[notifications]  # Desktop notifications
```

### SOPS Secret Management

For production deployments:
```yaml
# ~/.nixos/secrets/secrets.yaml (encrypted)
anthropic_api_key: ENC[AES256_GCM,data:...,iv:...,tag:...]
google_client_secret: ENC[AES256_GCM,data:...,iv:...,tag:...]
```

## Testing Strategy

### Unit Tests
```bash
cargo test                    # All tests
cargo test --package daemon   # Daemon tests only  
cargo test correlation_engine # Specific module
```

### Integration Tests
```bash
./quick-test.sh test          # Build + waybar output test
./quick-test.sh full          # Full cycle test
jasper-companion-daemon test-calendar  # Calendar integration
```

### Development Testing
```bash
# Add demo events
jasper-companion-daemon add-test-events

# Test with demo data
jasper-companion-daemon waybar

# Clean up test data
jasper-companion-daemon clean-database
```

## Contributing Guidelines

### Code Style
- **Rust**: Follow `rustfmt` defaults
- **Modules**: Keep under 500 lines when possible
- **Error Handling**: Use structured `JasperError` types
- **Async**: Use `tokio` for async operations
- **Documentation**: Document public APIs

### Commit Guidelines
```bash
# Good commit messages:
git commit -m "feat: add weather context source"
git commit -m "fix: handle calendar auth expiration"  
git commit -m "docs: update configuration examples"
git commit -m "refactor: extract notification service"
```

### Adding New Context Sources

1. **Create module** in `daemon/src/context_sources/your_source.rs`
2. **Implement trait**:
```rust
pub struct YourSource {
    config: YourConfig,
}

impl ContextSource for YourSource {
    fn id(&self) -> &str { "your_source" }
    fn name(&self) -> &str { "Your Source Name" }
    async fn fetch_context(&self, start: DateTime<Utc>, end: DateTime<Utc>) 
        -> Result<ContextData> {
        // Implementation here
    }
    fn is_enabled(&self) -> bool { self.config.enabled }
}
```
3. **Add configuration** to `Config` struct
4. **Register in manager** (`context_sources/mod.rs`)
5. **Add tests** and documentation

### Adding New Commands

1. **Create command struct** in appropriate `commands/*.rs` file
2. **Implement Command trait**:
```rust
pub struct YourCommand {
    pub some_parameter: String,
}

impl Command for YourCommand {
    async fn execute(&mut self, context: &CommandContext) -> Result<()> {
        // Implementation here
    }
}
```
3. **Add to CLI enum** in `main.rs`
4. **Add match arm** in command dispatch
5. **Add tests** and help text

### Performance Guidelines
- **Database**: Use transactions for bulk operations
- **HTTP**: Implement retry with exponential backoff
- **Memory**: Pre-allocate collections when size is known
- **Async**: Avoid blocking operations in async contexts

### Documentation Standards
- **README**: User-focused quick start
- **DEVELOPMENT**: Contributor-focused architecture
- **Code Comments**: Explain why, not what
- **Examples**: Include working code samples

## Release Process

1. **Update version** in `Cargo.toml`
2. **Run full tests**: `cargo test && ./quick-test.sh full`
3. **Update CHANGELOG.md** with new features/fixes
4. **Tag release**: `git tag v1.x.x`  
5. **Build release**: `cargo build --release`
6. **Update NixOS module** if needed

## Troubleshooting Development

### Common Issues

**"Command not found: cargo"**
```bash
nix develop  # Enter development shell
```

**"Permission denied" on scripts**
```bash
chmod +x dev-mode.sh quick-test.sh waybar-jasper.sh
```

**"Database locked" errors**
```bash
pkill jasper-companion-daemon  # Kill any running instances
```

**Waybar not reloading**
```bash
pkill waybar && waybar & disown  # Force restart
```

### Debug Environment Variables

```bash
export RUST_LOG=debug              # Enable debug logging
export RUST_BACKTRACE=1            # Stack traces on panic
export DATABASE_URL=sqlite:./dev.db # Override database path
```

This development guide ensures contributors can effectively work with Jasper's architecture while maintaining code quality and system stability.