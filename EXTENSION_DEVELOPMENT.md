# GNOME Shell Extension Development Guide

**Problem Solved**: Extension code changes not taking effect despite logout/login cycles due to NixOS caching and installation conflicts.

**Solution**: Bulletproof development workflow using system-wide installation with unique development UUIDs.

## Quick Start (TL;DR)

```bash
# 1. Edit gnome-extension/extension.js
# 2. Run development workflow
./scripts/extension-dev.sh install
# 3. Check GNOME Shell panel (may require logout/login on Wayland)
# 4. Check logs: tail ~/.jasper-extension-dev.log
```

## Root Cause Analysis

### Why Extension Development Was "Absolute Hell"

1. **NixOS Extension Loading**: User-level extensions in `~/.local/share/gnome-shell/extensions/` don't execute JavaScript on NixOS
2. **Extension Caching**: GNOME Shell aggressively caches extension code by UUID
3. **Multiple Versions**: Production and development extensions with same UUID caused conflicts
4. **Invisible Failures**: Extensions showed as "ACTIVE" but JavaScript never executed
5. **No Feedback Loop**: No logging to verify code execution

### The NixOS Reality

On NixOS, working extensions are installed at:
- **System path**: `/run/current-system/sw/share/gnome-shell/extensions/`
- **Symlinks to**: `/nix/store/.../share/gnome-shell/extensions/`
- **User directory**: `~/.local/share/gnome-shell/extensions/` (doesn't work reliably)

## Development Workflow

### 1. Architecture Overview

```
┌─────────────────────┐    ┌─────────────────────┐    ┌─────────────────────┐
│  Development UUID   │    │   System Symlink    │    │   GNOME Shell       │
│ jasper-dev-v1@      │───▶│ /run/current-sys/   │───▶│   Loads & Executes  │
│ tom.local           │    │ sw/share/gnome...   │    │   JavaScript        │
└─────────────────────┘    └─────────────────────┘    └─────────────────────┘
```

**Key Principles:**
- **Unique development UUID** prevents conflicts with production
- **System-wide installation** bypasses NixOS user-extension restrictions  
- **Observable logging** provides execution verification
- **Cache busting** through UUID versioning when needed

### 2. Extension Development Script

The `./scripts/extension-dev.sh` script provides a complete development workflow:

```bash
./scripts/extension-dev.sh install    # Complete cycle: cleanup, build, install, enable, test
./scripts/extension-dev.sh status     # Show extension and daemon status
./scripts/extension-dev.sh uninstall  # Remove development extension
./scripts/extension-dev.sh cleanup    # Remove ALL jasper extensions
```

#### What `install` Does:

1. **Cleanup**: Removes all existing jasper extensions (prevents conflicts)
2. **Build**: `nix build .#gnome-extension-dev` creates development package
3. **Install**: Symlinks to `/run/current-system/sw/share/gnome-shell/extensions/`
4. **Enable**: `gnome-extensions enable jasper-dev-v1@tom.local`
5. **Verify**: Checks installation, extension status, D-Bus communication
6. **Test**: Attempts daemon communication and logs results

### 3. Development Files Modified

**Primary**: `gnome-extension/extension.js`
- Main extension logic
- Now includes comprehensive logging via `logMessage()`
- Logs to: `~/.jasper-extension-dev.log`, console, and system journal

**Configuration**: `gnome-extension/metadata.json`
- Auto-updated by build process with development UUID
- Name changes to "Jasper AI Insights (Development)"

**Build System**: 
- `flake.nix` - Contains `packages.gnome-extension-dev` 
- `scripts/extension-dev.sh` - Development workflow manager

### 4. Verification & Debugging

#### Execution Verification
The development extension includes comprehensive logging:

```bash
# Watch logs in real-time
tail -f ~/.jasper-extension-dev.log

# Check recent logs
tail ~/.jasper-extension-dev.log

# System journal logs
journalctl | grep jasper-extension
```

**Expected log entries when working:**
```
2024-08-07T13:45:01.234Z: Jasper extension init() called
2024-08-07T13:45:01.456Z: Jasper extension enable() called - creating UI elements  
2024-08-07T13:45:01.678Z: Jasper extension UI created and added to panel
2024-08-07T13:45:03.890Z: refreshInsights() called - attempting D-Bus communication
```

#### Status Checks
```bash
./scripts/extension-dev.sh status
```

Shows:
- Extension status (enabled/disabled/active)
- Installation locations (system/user directories)  
- Daemon status
- D-Bus connectivity

#### Panel Verification
Look for extension icon in GNOME Shell top panel:
- **🔄** - Loading/refreshing data
- **📅** - Fallback (daemon not available) 
- **Other emoji** - AI-generated insights from daemon

### 5. Common Issues & Solutions

#### Extension Not Visible After Install
**Symptoms**: `gnome-extensions list` doesn't show extension
**Solution**: 
```bash
# Check if properly installed
./scripts/extension-dev.sh status

# On Wayland, may need logout/login
# On X11, Alt+F2, r should work
```

#### Extension Shows as ACTIVE but No Panel Icon
**Symptoms**: Extension enabled but no visible UI
**Cause**: JavaScript not executing (old NixOS issue)
**Solution**: Ensure system-wide installation via script

#### Code Changes Not Taking Effect  
**Symptoms**: Edit `extension.js` but no changes visible
**Solutions**:
1. Re-run full installation: `./scripts/extension-dev.sh install`
2. If persistent, increment version: `./scripts/extension-dev.sh increment-version`
3. Check logs for execution: `tail ~/.jasper-extension-dev.log`

#### No Log File Generated
**Symptoms**: `~/.jasper-extension-dev.log` doesn't exist
**Cause**: Extension JavaScript not executing at all
**Solutions**:
1. Verify system-wide installation: `./scripts/extension-dev.sh status`
2. Check GNOME Shell version compatibility (supports 45-48)
3. Try logout/login cycle

#### Sudo Password Prompts
**Symptoms**: Script asks for password multiple times
**Explanation**: System-wide installation requires root access to create symlinks
**Solutions**:
1. Enter password when prompted (normal behavior)
2. Configure sudo timeout: `sudo visudo` add `Defaults timestamp_timeout=30`

### 6. Version Management & Cache Busting

When extension changes aren't taking effect despite reinstallation:

#### Increment Development Version
```bash
./scripts/extension-dev.sh increment-version
```

This provides instructions to:
1. Update `EXTENSION_DEV_UUID` in `scripts/extension-dev.sh` 
2. Update UUID in `flake.nix` build configuration
3. Reinstall with new UUID (forces GNOME Shell to treat as new extension)

**Example progression**:
- `jasper-dev-v1@tom.local` → `jasper-dev-v2@tom.local` → `jasper-dev-v3@tom.local`

#### When to Increment:
- Persistent caching issues
- Major extension changes not taking effect
- GNOME Shell seems "stuck" on old version
- Starting fresh development session

### 7. Production Deployment Path

When development is complete:

1. **Test thoroughly** with development extension
2. **Copy changes** to production extension code
3. **Update production package** in `flake.nix`  
4. **Deploy via NixOS module** (not manual installation)
5. **Clean up development** extension

```bash
# Remove development extension
./scripts/extension-dev.sh uninstall

# Production deployment through NixOS config
cd ~/.nixos
sudo nixos-rebuild switch --flake .#gti
```

## Integration with Existing Workflow

### Daemon Development Integration

The extension development workflow complements the existing Rust daemon development:

```bash
# Combined development workflow
./dev-mode.sh start                    # Start daemon development
./scripts/extension-dev.sh install     # Install extension development

# Make changes to both:
# - daemon/src/*.rs (Rust code)  
# - gnome-extension/extension.js (JavaScript code)

# Test changes:
./quick-test.sh full                   # Test daemon
./scripts/extension-dev.sh status      # Test extension

# When complete:
./dev-mode.sh stop                     # Stop daemon development
./scripts/extension-dev.sh uninstall   # Remove extension development
```

### Files Modified in Development Mode

**Daemon Development** (`./dev-mode.sh`):
- `~/.config/waybar/` - Overridden with development configs
- `daemon/src/` - Direct Rust development

**Extension Development** (`./scripts/extension-dev.sh`):  
- `/run/current-system/sw/share/gnome-shell/extensions/` - System extension
- `gnome-extension/` - Direct JavaScript development

**No Conflicts**: Both can run simultaneously for full-stack development.

## Agent Instructions for Future Development

### Pre-Development Checklist

Before starting extension development work:

1. **Check current status**: `./scripts/extension-dev.sh status`
2. **Verify daemon**: `./dev-mode.sh status` (start if needed)
3. **Clean slate** (if needed): `./scripts/extension-dev.sh cleanup`

### Development Cycle

For each extension code change:

1. **Edit code**: Modify `gnome-extension/extension.js`
2. **Install**: `./scripts/extension-dev.sh install`  
3. **Verify**: Check logs `tail ~/.jasper-extension-dev.log`
4. **Test UI**: Look for extension icon in GNOME panel
5. **Iterate**: Repeat until satisfied

### Success Verification

After each change, verify these indicators:

- **Build Success**: Script shows "Extension built successfully"
- **Install Success**: Script shows "Extension symlinked to NixOS system directory"  
- **Enable Success**: Script shows "Extension enabled successfully"
- **Execution Logs**: Log file shows init(), enable(), and refresh calls
- **Panel Icon**: Visible emoji icon in GNOME Shell panel
- **D-Bus Communication**: Script shows successful daemon communication

### Error Recovery

When things go wrong:

1. **Check status first**: `./scripts/extension-dev.sh status`
2. **Review logs**: `tail ~/.jasper-extension-dev.log`
3. **Clean slate**: `./scripts/extension-dev.sh cleanup` then `install`
4. **Version increment**: If caching issues persist
5. **Manual testing**: Use `test-symlink-approach.sh` for step-by-step debugging

### Integration with Other Agents

**For Claude Code sessions**:
- Always run status check before starting
- Document any UUID version changes made
- Leave extension in clean state when done
- Update CLAUDE.md with any new workflow discoveries

**For collaborative development**:
- Extension development is independent of daemon development
- Both can run simultaneously
- Changes to either don't affect the other
- Production deployment requires coordination of both components

## Troubleshooting Reference

### Symptoms → Diagnosis → Solution

**Extension not detected by GNOME Shell**
→ User-level installation on NixOS
→ Use system-wide installation via script

**Code changes not visible**  
→ Extension caching/UUID conflicts
→ Re-run `./scripts/extension-dev.sh install`

**Panel icon missing despite "ACTIVE" status**
→ JavaScript not executing  
→ Verify system installation + check logs

**No log file generated**
→ Extension not running at all
→ Check installation paths via status command

**Sudo prompts during development**
→ System-wide installation requirement
→ Normal behavior on NixOS, enter password

**Persistent caching issues**
→ GNOME Shell UUID caching
→ Increment development version number

## Success Metrics

A successful development workflow provides:

1. **100% Reliability**: Code changes always take effect after script run
2. **Clear Feedback**: Immediate success/failure indication at each step  
3. **Observable Execution**: Logging confirms JavaScript is actually running
4. **Conflict-Free**: Development never interferes with production
5. **Fast Iteration**: Single command from code change to running extension
6. **Debuggable**: Clear logs and status information for troubleshooting

This workflow transforms the "absolute hell" of extension development on NixOS into a predictable, reliable process that guarantees your code changes will be loaded and executed.