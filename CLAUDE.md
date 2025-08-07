# Claude Code Instructions for Jasper Development

## ⚠️ CRITICAL: Use Development Mode

This project requires a special development workflow to avoid slow NixOS rebuilds.

### Before Any Development Work:

1. **Read the development guides**: 
   - **Backend/Waybar**: Read `DEVELOPMENT.md` completely
   - **GNOME Extension**: Read `EXTENSION_DEVELOPMENT.md` for extension work
2. **Check current status**: Run `./dev-mode.sh status`
3. **Enter development mode**: Run `./dev-mode.sh start` if not already active  
4. **For extension work**: Run `./scripts/extension-dev.sh status`
5. **Verify setup**: Run `./quick-test.sh status` to confirm everything is working

### Development Workflow:

#### Backend/Waybar Development:
```bash
# Start daemon development
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

#### GNOME Extension Development:
```bash
# Check extension status
./scripts/extension-dev.sh status

# Make changes to gnome-extension/extension.js

# Install and test extension
./scripts/extension-dev.sh install

# Check logs for execution verification
tail ~/.jasper-extension-dev.log

# When done with session
./scripts/extension-dev.sh uninstall
```

#### Combined Development (Backend + Extension):
```bash
# Start both development modes
./dev-mode.sh start
./scripts/extension-dev.sh install

# Make changes to both:
# - daemon/src/*.rs AND gnome-extension/extension.js

# Test both
./quick-test.sh full                    # Test backend
./scripts/extension-dev.sh status       # Test extension

# Stop both when done
./scripts/extension-dev.sh uninstall
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
- `gnome-extension/extension.js` - GNOME Shell extension logic
- `gnome-extension/metadata.json` - Extension metadata (auto-updated)

### DO NOT MODIFY:

- Files in `/home/tom/.nixos/` (production NixOS config)
- Files in `~/.config/waybar/` (managed by dev-mode.sh)
- Files in `/run/current-system/sw/share/gnome-shell/extensions/` (managed by extension-dev.sh)

### Testing Commands:

#### Backend/Waybar Testing:
```bash
./waybar-jasper.sh           # Test JSON output
./quick-test.sh test         # Build and test
./quick-test.sh reload       # Reload waybar
./quick-test.sh full         # Complete test cycle
```

#### Extension Testing:
```bash
./scripts/extension-dev.sh install    # Full extension cycle
./scripts/extension-dev.sh status     # Check extension status
./scripts/extension-dev.sh test-dbus   # Test D-Bus communication
tail ~/.jasper-extension-dev.log       # Check execution logs
```

### Error Recovery:

#### Backend Issues:
- If waybar isn't updating: `./quick-test.sh reload`
- If build fails: `cargo build` to see errors (may need `nix develop` first)
- If confused about state: `./dev-mode.sh status`

#### Extension Issues:
- If extension not visible: `./scripts/extension-dev.sh status` and check installation
- If code changes not taking effect: Re-run `./scripts/extension-dev.sh install`
- If persistent caching: `./scripts/extension-dev.sh increment-version`
- If no logs generated: Extension not executing, check system installation
- If confused about extension state: `./scripts/extension-dev.sh status`

### Expected Behavior:

#### Backend Development:
- Waybar will briefly disappear when entering/exiting development mode (this is normal)
- You may need to be in a nix shell (`nix develop`) for cargo commands to work
- The development system handles NixOS symlinks automatically

#### Extension Development:
- Extension installation requires sudo password (system-wide installation)
- On Wayland, extension changes may require logout/login to be visible
- Extension logs to `~/.jasper-extension-dev.log` for verification
- Panel icon should appear: 🔄 (loading), 📅 (daemon offline), or other emoji (working)

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
- `DEVELOPMENT.md` - Complete backend architecture and contributor guide
- `EXTENSION_DEVELOPMENT.md` - GNOME extension development workflow (READ THIS for extension work)
- `waybar/README.md` - Waybar integration setup

## Important Notes:

- The development system uses local builds and configs
- Production uses NixOS-managed configuration  
- Always exit development modes when done:
  - Backend: `./dev-mode.sh stop`
  - Extension: `./scripts/extension-dev.sh uninstall`
- Changes are only persisted to NixOS config manually after development

## Critical Success Patterns:

### ✅ Extension Development Success Indicators:
1. **Script shows**: "Extension symlinked to NixOS system directory"
2. **Status shows**: Extension enabled and ACTIVE
3. **Logs exist**: `~/.jasper-extension-dev.log` contains execution messages
4. **Panel visible**: Extension icon appears in GNOME Shell panel
5. **D-Bus working**: Script shows successful daemon communication

### ❌ Extension Development Failure Signs:
1. **User-level install**: Extension only in `~/.local/share/gnome-shell/extensions/`
2. **No logs**: Missing `~/.jasper-extension-dev.log` means JavaScript not executing  
3. **"Extension does not exist"**: GNOME Shell can't see extension (installation failed)
4. **ACTIVE but no panel icon**: JavaScript running but UI creation failed
5. **Persistent caching**: Code changes don't appear despite reinstallation

**When extension development isn't working**: Always check `EXTENSION_DEVELOPMENT.md` troubleshooting section first.