# Ubuntu Support for Jasper Companion

This directory contains Ubuntu-specific installation and development files for Jasper Companion.

## Quick Start

```bash
# 1. Install dependencies
./ubuntu/install-deps.sh

# 2. Build
make build

# 3. Install
sudo make install

# 4. Configure
jasper-companion-daemon set-api-key <your-anthropic-api-key>

# 5. Start
systemctl --user enable --now jasper-companion

# 6. Install GNOME extension (optional)
make install-extension
```

## Files in this Directory

- **install-deps.sh** - Automated dependency installation for Ubuntu/Debian
- **dev-helper.sh** - Development workflow helper script
- **jasper-companion.service.in** - systemd user service template
- **org.jasper.Daemon.service.in** - D-Bus service file template
- **README.md** - This file

## Documentation

For complete setup and usage instructions, see:
- **[docs/UBUNTU_SETUP.md](../docs/UBUNTU_SETUP.md)** - Complete Ubuntu setup guide
- **[README.md](../README.md)** - General project overview
- **[docs/DEVELOPMENT.md](../docs/DEVELOPMENT.md)** - Architecture documentation

## System Requirements

- Ubuntu 25.11 or later (or compatible Debian-based distribution)
- Rust 1.70+
- GNOME Shell 47+ (for extension) or Waybar (for tiling WMs)

## Build System

The Ubuntu build uses standard tools instead of Nix:
- **Makefile** - Main build system (in project root)
- **cargo** - Rust package manager
- **systemd** - Service management
- **apt-get** - Package installation

## Development Workflow

```bash
# Use the development helper
./ubuntu/dev-helper.sh status           # Check status
./ubuntu/dev-helper.sh build-debug      # Build debug binary
./ubuntu/dev-helper.sh install-dev      # Install debug build
./ubuntu/dev-helper.sh restart          # Restart daemon
./ubuntu/dev-helper.sh logs             # Watch logs

# Quick development cycle
./ubuntu/dev-helper.sh quick            # Build + install + restart
```

## Differences from NixOS

The NixOS version uses:
- Nix flakes for reproducible builds
- NixOS modules for declarative configuration
- Unified package with auto-detection
- Development mode scripts that manage symlinks

The Ubuntu version uses:
- Standard Makefile
- Manual service management
- Separate installation steps
- Simplified development scripts

## Installation Paths

| Component | Path |
|-----------|------|
| Daemon binary | `/usr/local/bin/jasper-companion` |
| systemd service | `/etc/systemd/user/jasper-companion.service` |
| D-Bus service | `/usr/share/dbus-1/services/org.jasper.Daemon.service` |
| GNOME extension | `~/.local/share/gnome-shell/extensions/jasper-companion@heytcass.github/` |
| Configuration | `~/.config/jasper-companion/config.toml` |
| Database | `~/.local/share/jasper-companion/jasper.db` |

## Getting Help

If you encounter issues:
1. Check **docs/UBUNTU_SETUP.md** troubleshooting section
2. Run `./ubuntu/dev-helper.sh status` to check system status
3. View logs with `journalctl --user -u jasper-companion-daemon -f`
4. Open an issue at https://github.com/heytcass/jasper/issues
