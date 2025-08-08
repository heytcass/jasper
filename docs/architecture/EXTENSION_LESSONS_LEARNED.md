# GNOME Shell Extension Development Lessons Learned

## Critical Success Factors

### 1. NixOS GNOME Extension Development Workflow
- **MUST** use system-wide installation via NixOS configuration
- **MUST** add extension to NixOS modules/desktop.nix
- **MUST** create nixpkgs overlay in systems/gti/default.nix
- **MUST** update flake lock after every code change
- **MUST** rebuild NixOS and restart session for changes to take effect

### 2. GNOME Shell 48 API Changes
- **ES6 Modules Required**: Use `import` statements, not legacy `imports.`
- **No Constructor**: Don't use constructor() in extension class - causes module loading errors
- **Actor API Changes**: Use `add_child()`/`remove_child()`, not `add_actor()`/`remove_actor()`
- **PanelMenu.Button Events**: Hover/click events on PanelMenu.Button don't work reliably on Wayland

### 3. Extension Caching Issues
- GNOME Shell aggressively caches extensions
- Multiple cache locations can conflict:
  - `/run/current-system/sw/share/gnome-shell/extensions/` (NixOS system)
  - `/usr/share/gnome-shell/extensions/` (system-wide)
  - `~/.local/share/gnome-shell/extensions/` (user)
- **Solution**: Increment UUID version (v1→v2→v3) to force cache invalidation
- **Always** check which path GNOME Shell is using: `gnome-extensions info <uuid> | grep Path`

### 4. Working Solution: Popup Menu Pattern
```javascript
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

// In enable():
this._item = new PopupMenu.PopupMenuItem('');
this._item.label.clutter_text.set_line_wrap(true);
this._item.label.style = 'max-width: 300px;';
this._indicator.menu.addMenuItem(this._item);
```

### 5. D-Bus Integration
- Daemon service name: `jasper-companion-daemon.service` (not `jasper-companion.service`)
- D-Bus returns: `("JSON_STRING",)` format - needs careful parsing
- Escape handling: `\n` → newline, `\"` → quote, `\\` → backslash

## Development Workflow

### Quick Development Cycle
1. Edit extension code
2. `git add && git cc-commit-msg "message"`
3. `cd ~/.nixos && nix flake lock --update-input jasper`
4. `sudo nixos-rebuild switch --flake .#gti`
5. Logout/login (required on Wayland)

### Debugging
- Logs: `journalctl --user -f | grep Jasper`
- GNOME errors: `journalctl --user | grep "JS ERROR"`
- Looking Glass: Alt+F2 → `lg` → Extensions tab

## Common Pitfalls to Avoid

1. **DON'T** use tooltip approaches - they don't work reliably
2. **DON'T** rely on hover events with PanelMenu.Button
3. **DON'T** use the development install script for final deployment
4. **DON'T** mix `/usr/share` and Nix store installations
5. **DON'T** forget to logout/login on Wayland after changes

## Daemon Integration Notes

- Service unit: `~/.config/systemd/user/jasper-companion-daemon.service`
- Logs: `journalctl --user -u jasper-companion-daemon`
- D-Bus test: `gdbus call --session --dest org.personal.CompanionAI --object-path /org/personal/CompanionAI/Companion --method org.personal.CompanionAI.Companion1.GetFormattedInsights "gnome"`

## Final Working Architecture

```
GNOME Shell Panel
    ↓
PanelMenu.Button (shows emoji icon)
    ↓ (click)
PopupMenu (shows AI insight text)
    ↓ (every 5 seconds)
D-Bus call to daemon for cached insights
    ↓
Rust daemon returns pre-analyzed JSON
    ↓
(AI analysis happens only on daemon startup)
Returns JSON with icon + insight
```

## Required Files for NixOS Integration

1. `flake.nix` - Must export `gnome-extension-dev` package
2. `~/.nixos/modules/desktop.nix` - Add extension to gnomePackages
3. `~/.nixos/systems/gti/default.nix` - Add nixpkgs overlay
4. Extension must be in Nix store, not `/usr/share`

## Success Indicators

✅ Extension shows in `gnome-extensions list`
✅ State is ACTIVE in `gnome-extensions info`
✅ Path shows `/run/current-system/sw/share/gnome-shell/extensions/`
✅ Clicking icon shows popup menu with AI insights
✅ Icon updates based on calendar events (🎂 for birthdays)
✅ Daemon is running and responding to D-Bus calls

## Time Estimate for Future Changes

With this documentation: 30 minutes
Without this documentation: 5+ hours (as we experienced)