#!/bin/bash
# Jasper Companion - Ubuntu Dependency Installation Script
# For Ubuntu 25.11 and other Debian-based systems

set -e

echo "==================================="
echo "Jasper Companion Dependency Installer"
echo "==================================="
echo ""

# Check if running as root
if [ "$EUID" -eq 0 ]; then
    echo "Error: Do not run this script as root/sudo"
    echo "Run as normal user: ./ubuntu/install-deps.sh"
    exit 1
fi

# Detect package manager
if command -v apt-get >/dev/null 2>&1; then
    PKG_MANAGER="apt-get"
elif command -v apt >/dev/null 2>&1; then
    PKG_MANAGER="apt"
else
    echo "Error: Could not find apt or apt-get package manager"
    echo "This script is designed for Debian/Ubuntu systems"
    exit 1
fi

echo "Detected package manager: $PKG_MANAGER"
echo ""

# Update package lists
echo "Updating package lists..."
sudo $PKG_MANAGER update

echo ""
echo "Installing build dependencies..."
echo ""

# Install build dependencies
BUILD_DEPS=(
    "build-essential"
    "pkg-config"
    "libssl-dev"
    "libsqlite3-dev"
    "libdbus-1-dev"
    "curl"
    "git"
)

for dep in "${BUILD_DEPS[@]}"; do
    echo "Installing $dep..."
    sudo $PKG_MANAGER install -y "$dep"
done

# Check if Rust is installed
if ! command -v cargo >/dev/null 2>&1; then
    echo ""
    echo "Rust is not installed. Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

    # Source cargo env
    source "$HOME/.cargo/env"

    echo "Rust installed successfully ✓"
else
    echo ""
    echo "Rust is already installed ✓"
    rustc --version
    cargo --version
fi

# Ask about optional dependencies
echo ""
echo "==================================="
echo "Optional Frontend Dependencies"
echo "==================================="
echo ""

# Check for GNOME
if [ "$XDG_CURRENT_DESKTOP" = "GNOME" ] || command -v gnome-shell >/dev/null 2>&1; then
    echo "GNOME desktop environment detected."
    read -p "Install GNOME Shell extension support? [Y/n] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Nn]$ ]]; then
        echo "Installing GNOME Shell extensions support..."
        sudo $PKG_MANAGER install -y gnome-shell-extensions
        echo "GNOME extension support installed ✓"
    fi
fi

# Check for Waybar
if command -v waybar >/dev/null 2>&1; then
    echo ""
    echo "Waybar is already installed ✓"
else
    echo ""
    read -p "Install Waybar for tiling window managers? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "Installing Waybar..."
        sudo $PKG_MANAGER install -y waybar
        echo "Waybar installed ✓"
    fi
fi

echo ""
echo "==================================="
echo "Dependency Installation Complete!"
echo "==================================="
echo ""
echo "Next steps:"
echo "  1. Build daemon: make build"
echo "  2. Install: sudo make install"
echo "  3. See docs/UBUNTU_SETUP.md for full setup guide"
echo ""
