#!/usr/bin/env bash

# GNOME Extension Development Script for Jasper
# Provides guaranteed extension code loading through cache busting and system-wide installation

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Extension UUIDs
EXTENSION_DEV_UUID="jasper-dev-v3@tom.local"
EXTENSION_PROD_UUID="jasper@tom.local"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Comprehensive cleanup of all jasper extensions
cleanup_all_extensions() {
    log_info "Cleaning up all existing Jasper extensions..."
    
    # Find all jasper extension UUIDs
    local jasper_extensions=$(gnome-extensions list 2>/dev/null | grep -E "jasper.*@tom\.local" || true)
    
    if [ -n "$jasper_extensions" ]; then
        log_info "Found existing extensions: $jasper_extensions"
        
        for extension_uuid in $jasper_extensions; do
            log_info "Disabling extension: $extension_uuid"
            gnome-extensions disable "$extension_uuid" 2>/dev/null || log_warning "Could not disable $extension_uuid"
            
            log_info "Uninstalling extension: $extension_uuid"
            gnome-extensions uninstall "$extension_uuid" 2>/dev/null || log_warning "Could not uninstall $extension_uuid"
        done
    else
        log_info "No existing jasper extensions found"
    fi
    
    # Remove from both user and system directories
    log_info "Removing extension directories..."
    sudo rm -rf "/usr/share/gnome-shell/extensions/jasper"*"@tom.local" 2>/dev/null || true
    rm -rf "$HOME/.local/share/gnome-shell/extensions/jasper"*"@tom.local" 2>/dev/null || true
    
    log_success "Extension cleanup completed"
}

# Build development extension
build_extension() {
    log_info "Building development extension..."
    
    cd "$PROJECT_DIR"
    
    # Build the development extension package
    if nix build .#gnome-extension-dev -o result-gnome-dev; then
        log_success "Extension built successfully"
        return 0
    else
        log_error "Extension build failed"
        return 1
    fi
}

# Install extension system-wide (bypasses user-level caching issues)
install_extension() {
    log_info "Installing development extension system-wide..."
    
    local extension_path="result-gnome-dev/share/gnome-shell/extensions/$EXTENSION_DEV_UUID"
    
    if [ ! -d "$extension_path" ]; then
        log_error "Built extension not found at: $extension_path"
        return 1
    fi
    
    # On NixOS, system extensions are symlinked from /run/current-system/sw/share/gnome-shell/extensions/
    local nixos_extension_dir="/run/current-system/sw/share/gnome-shell/extensions"
    
    if [ -d "$nixos_extension_dir" ]; then
        log_info "Installing extension to NixOS system extension directory..."
        log_info "This requires sudo access - please enter your password when prompted"
        
        if sudo ln -sfn "$(realpath "$extension_path")" "$nixos_extension_dir/$EXTENSION_DEV_UUID"; then
            log_success "Extension symlinked to NixOS system directory"
        else
            log_error "Failed to create symlink in NixOS system directory"
            log_warning "Falling back to direct copy approach"
            
            # Create the directory if it doesn't exist and try direct copy
            sudo mkdir -p "/usr/share/gnome-shell/extensions"
            if sudo cp -r "$extension_path" "/usr/share/gnome-shell/extensions/"; then
                log_success "Extension copied to /usr/share/gnome-shell/extensions/"
            else
                log_error "Failed to copy extension to system directory"
                return 1
            fi
        fi
    else
        log_warning "NixOS system extension directory not found, trying traditional system install..."
        
        # Traditional system directory installation
        log_info "Creating system extension directory and installing..."
        sudo mkdir -p "/usr/share/gnome-shell/extensions"
        
        if sudo cp -r "$extension_path" "/usr/share/gnome-shell/extensions/"; then
            log_success "Extension copied to system directory"
        else
            log_error "Failed to copy extension to system directory"
            return 1
        fi
    fi
    
    # Verify installation (check all possible locations)
    if [ -L "/run/current-system/sw/share/gnome-shell/extensions/$EXTENSION_DEV_UUID" ] || 
       [ -d "/run/current-system/sw/share/gnome-shell/extensions/$EXTENSION_DEV_UUID" ]; then
        log_success "Extension installed to NixOS system directory"
        return 0
    elif [ -d "/usr/share/gnome-shell/extensions/$EXTENSION_DEV_UUID" ]; then
        log_success "Extension installed to system directory"
        return 0
    else
        log_error "Extension installation verification failed"
        return 1
    fi
}

# Enable extension and verify it loads
enable_and_verify() {
    log_info "Enabling development extension..."
    
    # Try to refresh GNOME Shell extension cache
    log_info "Refreshing extension cache..."
    killall -SIGUSR1 gnome-shell 2>/dev/null || log_warning "Could not send refresh signal to GNOME Shell"
    sleep 1
    
    # Enable the extension
    if gnome-extensions enable "$EXTENSION_DEV_UUID"; then
        log_success "Extension enabled successfully"
    else
        log_error "Failed to enable extension - extension may not be visible to GNOME Shell"
        log_warning "On NixOS, system-wide installation may be required"
        return 1
    fi
    
    # Wait a moment for GNOME Shell to load the extension
    sleep 2
    
    # Verify extension is active
    local extension_status=$(gnome-extensions info "$EXTENSION_DEV_UUID" 2>/dev/null | grep "State:" || true)
    
    if echo "$extension_status" | grep -q "ACTIVE"; then
        log_success "Extension is ACTIVE: $extension_status"
    else
        log_warning "Extension status: $extension_status"
        log_warning "Extension may not be properly loaded"
    fi
    
    return 0
}

# Test D-Bus communication
test_dbus_communication() {
    log_info "Testing D-Bus communication..."
    
    # Check if daemon is running
    if ! systemctl --user is-active jasper-companion >/dev/null 2>&1; then
        log_warning "Jasper daemon is not running. Starting it..."
        systemctl --user start jasper-companion || log_warning "Could not start daemon"
        sleep 2
    fi
    
    # Test D-Bus call
    local dbus_result
    if dbus_result=$(timeout 10 gdbus call --session --dest org.personal.CompanionAI \
        --object-path /org/personal/CompanionAI/Companion \
        --method org.personal.CompanionAI.Companion1.GetFormattedInsights "gnome" 2>/dev/null); then
        log_success "D-Bus communication working: ${dbus_result:0:100}..."
    else
        log_warning "D-Bus communication failed or timed out"
        log_warning "This may be normal if daemon is still starting up"
    fi
}

# Show extension status and verification
show_status() {
    log_info "Extension Development Status:"
    echo "========================================"
    
    # Show extension info
    echo "Development Extension:"
    local dev_info=$(gnome-extensions info "$EXTENSION_DEV_UUID" 2>/dev/null || echo "Not installed")
    echo "  UUID: $EXTENSION_DEV_UUID"
    echo "  Status: $dev_info"
    echo
    
    echo "Production Extension:"
    local prod_info=$(gnome-extensions info "$EXTENSION_PROD_UUID" 2>/dev/null || echo "Not installed")
    echo "  UUID: $EXTENSION_PROD_UUID"  
    echo "  Status: $prod_info"
    echo
    
    # Show daemon status
    echo "Daemon Status:"
    if systemctl --user is-active jasper-companion >/dev/null 2>&1; then
        echo "  Jasper Daemon: Running"
    else
        echo "  Jasper Daemon: Not running"
    fi
    echo
    
    # Show file system locations
    echo "Installation Locations:"
    if [ -L "/run/current-system/sw/share/gnome-shell/extensions/$EXTENSION_DEV_UUID" ]; then
        echo "  NixOS System: /run/current-system/sw/share/gnome-shell/extensions/$EXTENSION_DEV_UUID (✓ symlink)"
        echo "    → $(readlink "/run/current-system/sw/share/gnome-shell/extensions/$EXTENSION_DEV_UUID")"
    elif [ -d "/run/current-system/sw/share/gnome-shell/extensions/$EXTENSION_DEV_UUID" ]; then
        echo "  NixOS System: /run/current-system/sw/share/gnome-shell/extensions/$EXTENSION_DEV_UUID (✓ directory)"
    else
        echo "  NixOS System: /run/current-system/sw/share/gnome-shell/extensions/$EXTENSION_DEV_UUID (✗ missing)"
    fi
    
    if [ -d "/usr/share/gnome-shell/extensions/$EXTENSION_DEV_UUID" ]; then
        echo "  System: /usr/share/gnome-shell/extensions/$EXTENSION_DEV_UUID (✓ exists)"
    else
        echo "  System: /usr/share/gnome-shell/extensions/$EXTENSION_DEV_UUID (✗ missing)"
    fi
    
    if [ -d "$HOME/.local/share/gnome-shell/extensions/$EXTENSION_DEV_UUID" ]; then
        echo "  User: ~/.local/share/gnome-shell/extensions/$EXTENSION_DEV_UUID (✓ exists)"
    else
        echo "  User: ~/.local/share/gnome-shell/extensions/$EXTENSION_DEV_UUID (✗ missing)"
    fi
    echo
}

# Full development cycle: clean, build, install, enable, test
install_dev() {
    log_info "Starting full development extension installation..."
    echo "========================================"
    
    cleanup_all_extensions
    
    if ! build_extension; then
        log_error "Build failed. Aborting installation."
        return 1
    fi
    
    if ! install_extension; then
        log_error "Installation failed. Aborting."
        return 1
    fi
    
    if ! enable_and_verify; then
        log_error "Enable/verification failed."
        return 1
    fi
    
    test_dbus_communication
    
    echo
    log_success "Development extension installation completed!"
    log_info "The extension should now be active in your GNOME Shell panel"
    log_warning "On Wayland, you may need to logout/login to see extension changes"
    log_info "Check the panel for a 🔄 or 📅 icon from Jasper"
    
    show_status
}

# Uninstall development extension
uninstall_dev() {
    log_info "Uninstalling development extension..."
    
    gnome-extensions disable "$EXTENSION_DEV_UUID" 2>/dev/null || log_warning "Could not disable extension"
    gnome-extensions uninstall "$EXTENSION_DEV_UUID" 2>/dev/null || log_warning "Could not uninstall extension"
    sudo rm -rf "/usr/share/gnome-shell/extensions/$EXTENSION_DEV_UUID" 2>/dev/null || true
    
    log_success "Development extension uninstalled"
}

# Increment development version (for cache busting)
increment_version() {
    local current_version="${EXTENSION_DEV_UUID//jasper-dev-v/}"
    current_version="${current_version//@tom.local/}"
    local next_version=$((current_version + 1))
    local new_uuid="jasper-dev-v${next_version}@tom.local"
    
    log_info "Incrementing development version from v$current_version to v$next_version"
    log_warning "This requires manual update of the UUID in flake.nix"
    log_warning "Current UUID: $EXTENSION_DEV_UUID"
    log_warning "New UUID needed: $new_uuid"
    
    echo "To increment version:"
    echo "1. Edit flake.nix and replace all instances of '$EXTENSION_DEV_UUID' with '$new_uuid'"
    echo "2. Update this script's EXTENSION_DEV_UUID variable to '$new_uuid'"
    echo "3. Run './scripts/extension-dev.sh install' with the new version"
}

# Show help
show_help() {
    echo "GNOME Extension Development Script for Jasper"
    echo "============================================="
    echo
    echo "Usage: $0 {install|uninstall|status|test-dbus|cleanup|increment-version|help}"
    echo
    echo "Commands:"
    echo "  install           - Full development cycle: cleanup, build, install, enable, test"
    echo "  uninstall         - Remove development extension"
    echo "  status            - Show current extension and daemon status"  
    echo "  test-dbus         - Test D-Bus communication with daemon"
    echo "  cleanup           - Remove all jasper extensions (dev + production)"
    echo "  increment-version - Instructions for version increments (cache busting)"
    echo "  help              - Show this help message"
    echo
    echo "Development Workflow:"
    echo "1. Edit gnome-extension/extension.js"
    echo "2. Run: ./scripts/extension-dev.sh install"
    echo "3. Check GNOME Shell panel for changes"
    echo "4. If no changes visible, try increment-version for cache busting"
    echo
    echo "Troubleshooting:"
    echo "- Extension shows as ACTIVE but no panel icon: Try logout/login (Wayland limitation)"
    echo "- Changes not visible: Use increment-version to force cache invalidation"
    echo "- Build failures: Check 'nix build .#gnome-extension-dev' output"
    echo "- Permission errors: Ensure sudo access for system-wide installation"
}

# Main command dispatch
case "${1:-help}" in
    install) 
        install_dev 
        ;;
    uninstall) 
        uninstall_dev 
        ;;
    status) 
        show_status 
        ;;
    test-dbus)
        test_dbus_communication
        ;;
    cleanup)
        cleanup_all_extensions
        ;;
    increment-version)
        increment_version
        ;;
    help|*)
        show_help
        ;;
esac