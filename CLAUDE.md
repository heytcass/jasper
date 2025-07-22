# Claude Code Instructions for Jasper Development

## ⚠️ CRITICAL: Use Development Mode

This project requires a special development workflow to avoid slow NixOS rebuilds.

### Before Any Development Work:

1. **Read the development guide**: Read `/home/tom/git/jasper/DEVELOPMENT.md` completely
2. **Check current status**: Run `./dev-mode.sh status`
3. **Enter development mode**: Run `./dev-mode.sh start` if not already active
4. **Verify setup**: Run `./quick-test.sh status` to confirm everything is working

### Development Workflow:

```bash
# Always start here
./dev-mode.sh start

# Make your changes to:
# - daemon/src/*.rs (Rust code)
# - waybar/style.css (CSS styling)
# - waybar/config.json (Waybar configuration)

# Test your changes
./quick-test.sh full

# For CSS changes, use live editing
./quick-test.sh css

# When done with session
./dev-mode.sh stop
```

### Key Files to Modify:

**Rust Backend:**
- `daemon/src/commands/` - CLI command implementations
- `daemon/src/services/` - Business logic layer
- `daemon/src/context_sources/` - Data source plugins
- `daemon/src/waybar_formatter.rs` - JSON output formatting
- `daemon/src/correlation_engine.rs` - AI analysis orchestration
- `daemon/src/config.rs` - Configuration management

**Frontend Integration:**
- `waybar/style.css` - Styling (uses Stylix variables)
- `waybar/config.json` - Waybar module configuration

### DO NOT MODIFY:

- Files in `/home/tom/.nixos/` (production NixOS config)
- Files in `~/.config/waybar/` (managed by dev-mode.sh)

### Testing Commands:

```bash
./waybar-jasper.sh           # Test JSON output
./quick-test.sh test         # Build and test
./quick-test.sh reload       # Reload waybar
./quick-test.sh full         # Complete test cycle
```

### Error Recovery:

- If waybar isn't updating: `./quick-test.sh reload`
- If build fails: `cargo build` to see errors (may need `nix develop` first)
- If confused about state: `./dev-mode.sh status`

### Expected Behavior:

- Waybar will briefly disappear when entering/exiting development mode (this is normal)
- You may need to be in a nix shell (`nix develop`) for cargo commands to work
- The development system handles NixOS symlinks automatically

### NixOS Rebuild Conflicts:

- If you run `nixos-rebuild switch` while in development mode, it will fail with "would be clobbered"
- **Solution**: Exit development mode first (`./dev-mode.sh stop`), then rebuild, then restart development mode
- This is by design - it prevents NixOS from overwriting your development work

### Architecture Overview:

**Command Pattern:** CLI commands are organized in `daemon/src/commands/`
- `auth.rs` - Google OAuth2 and API key management
- `calendar.rs` - Calendar sync and testing operations
- `daemon_ops.rs` - Daemon lifecycle management
- `waybar.rs` - Waybar JSON output formatting

**Service Layer:** Business logic in `daemon/src/services/`
- `companion.rs` - Main orchestration service
- `calendar.rs` - Google Calendar operations
- `insight.rs` - AI analysis coordination
- `notification.rs` - Desktop notification system

**Context Sources:** Extensible data sources in `daemon/src/context_sources/`
- `calendar.rs` - Google Calendar integration
- `obsidian.rs` - Markdown vault parsing
- `weather.rs` - Weather context
- `tasks.rs` - Task management (planned)

### Documentation:

- `README.md` - User-focused quick start guide
- `DEVELOPMENT.md` - Complete architecture and contributor guide
- `waybar/README.md` - Waybar integration setup

## Important Notes:

- The development system uses local builds and configs
- Production uses NixOS-managed configuration
- Always exit development mode when done: `./dev-mode.sh stop`
- Changes are only persisted to NixOS config manually after development