# GNOME Extension Development on NixOS - Pain Points

## The Problem
NixOS + GNOME Shell = Development Hell for Extensions

Every single code change requires:
1. Update extension files
2. Change UUID to bypass cache (or disable/enable)
3. **Logout and login** (kills entire session!)
4. Wait for GNOME to discover extension
5. Hope it works, or repeat entire cycle

## Why This Happens
- GNOME Shell aggressively caches extension code
- NixOS's immutable approach conflicts with dynamic development
- No hot reload for extensions like other platforms
- `gnome-extensions disable/enable` doesn't actually reload code

## Potential Solutions

### 1. Nested GNOME Session (Recommended)
```bash
dbus-run-session -- gnome-shell --nested --wayland
```
- Pros: Isolated environment, see errors immediately
- Cons: Different from production environment

### 2. Development Script
Create a script that:
- Increments UUID automatically
- Copies files
- Logs errors from journalctl
- Provides clear feedback

### 3. Use GNOME Builder or Looking Glass
- Alt+F2, 'lg' for Looking Glass console
- Can see extension errors in real-time
- Still requires logout for code changes

### 4. Consider Alternative Approach
- Develop on a VM with standard Linux first
- Port to NixOS once working
- Use the Rust backend for most logic

## Current Workflow (Painful but Works)
1. Make changes
2. Increment UUID in metadata.json
3. Copy to new extension directory
4. Logout/Login
5. Enable new extension
6. Check logs: `journalctl --user -b | grep jasper`

## The Real Solution
The Jasper architecture is actually smart here - put all the complex logic in the Rust daemon (which can be reloaded easily) and keep the GNOME extension as a thin display layer. This minimizes the painful logout/login cycles.