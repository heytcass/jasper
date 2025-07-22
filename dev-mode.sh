#!/usr/bin/env bash

# Jasper Development Mode Script
# Toggles between NixOS-managed and development Waybar configs for fast iteration

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WAYBAR_CONFIG_DIR="$HOME/.config/waybar"
BACKUP_DIR="$SCRIPT_DIR/.dev-backups"
DEV_CONFIG_DIR="$SCRIPT_DIR/waybar"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${BLUE}[dev-mode]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[dev-mode]${NC} $1"
}

error() {
    echo -e "${RED}[dev-mode]${NC} $1"
}

success() {
    echo -e "${GREEN}[dev-mode]${NC} $1"
}

check_waybar_running() {
    if pgrep -f waybar > /dev/null; then
        return 0
    else
        return 1
    fi
}

start_dev_mode() {
    log "Starting Jasper development mode..."
    
    # Check if already in dev mode
    if [ -f "$BACKUP_DIR/.dev-mode-active" ]; then
        warn "Already in development mode. Use 'stop' to exit first."
        return 1
    fi
    
    # Create backup directory
    mkdir -p "$BACKUP_DIR"
    
    # Stop waybar if running
    if check_waybar_running; then
        log "Stopping waybar..."
        pkill waybar || true
        sleep 1
    fi
    
    # Backup existing configs (handle symlinks)
    log "Backing up NixOS-managed configs..."
    if [ -f "$WAYBAR_CONFIG_DIR/config" ]; then
        # If it's a symlink, save the target path for restoration
        if [ -L "$WAYBAR_CONFIG_DIR/config" ]; then
            readlink "$WAYBAR_CONFIG_DIR/config" > "$BACKUP_DIR/config.symlink"
        fi
        # Copy the actual content
        cp "$WAYBAR_CONFIG_DIR/config" "$BACKUP_DIR/config.backup"
    fi
    if [ -f "$WAYBAR_CONFIG_DIR/style.css" ]; then
        # If it's a symlink, save the target path for restoration
        if [ -L "$WAYBAR_CONFIG_DIR/style.css" ]; then
            readlink "$WAYBAR_CONFIG_DIR/style.css" > "$BACKUP_DIR/style.css.symlink"
        fi
        # Copy the actual content
        cp "$WAYBAR_CONFIG_DIR/style.css" "$BACKUP_DIR/style.css.backup"
    fi
    
    # Remove existing files/symlinks and install development configs
    log "Installing development configs..."
    rm -f "$WAYBAR_CONFIG_DIR/config" "$WAYBAR_CONFIG_DIR/style.css"
    cp "$DEV_CONFIG_DIR/config.json" "$WAYBAR_CONFIG_DIR/config"
    cp "$DEV_CONFIG_DIR/style.css" "$WAYBAR_CONFIG_DIR/style.css"
    
    # Mark as active
    touch "$BACKUP_DIR/.dev-mode-active"
    
    # Build the project
    log "Building Jasper daemon..."
    cd "$SCRIPT_DIR"
    if command -v cargo > /dev/null; then
        cargo build
    else
        warn "Cargo not found. Run 'nix develop' to enter development shell first."
        warn "Or build manually with: nix develop --command cargo build"
    fi
    
    # Start waybar with development config
    log "Starting waybar with development config..."
    waybar &
    
    success "Development mode active!"
    success "✅ Waybar using development configs"
    success "✅ Local Jasper build active"
    success "✅ Ready for fast iteration"
    echo
    success "Development workflow:"
    echo "  • Rust changes: cargo build (automatic pickup)"
    echo "  • CSS changes: edit waybar/style.css → ./dev-mode.sh reload"
    echo "  • Config changes: edit waybar/config.json → ./dev-mode.sh reload"
    echo "  • Test Jasper: ./waybar-jasper.sh"
    echo "  • Exit dev mode: ./dev-mode.sh stop"
}

stop_dev_mode() {
    log "Stopping Jasper development mode..."
    
    # Check if in dev mode
    if [ ! -f "$BACKUP_DIR/.dev-mode-active" ]; then
        warn "Not in development mode."
        return 1
    fi
    
    # Stop waybar
    if check_waybar_running; then
        log "Stopping waybar..."
        pkill waybar || true
        sleep 1
    fi
    
    # Restore original configs (handle symlinks)
    log "Restoring NixOS-managed configs..."
    rm -f "$WAYBAR_CONFIG_DIR/config" "$WAYBAR_CONFIG_DIR/style.css"
    
    # Restore config
    if [ -f "$BACKUP_DIR/config.symlink" ]; then
        # Restore as symlink
        ln -s "$(cat "$BACKUP_DIR/config.symlink")" "$WAYBAR_CONFIG_DIR/config"
    elif [ -f "$BACKUP_DIR/config.backup" ]; then
        # Restore as regular file
        cp "$BACKUP_DIR/config.backup" "$WAYBAR_CONFIG_DIR/config"
    fi
    
    # Restore style.css
    if [ -f "$BACKUP_DIR/style.css.symlink" ]; then
        # Restore as symlink
        ln -s "$(cat "$BACKUP_DIR/style.css.symlink")" "$WAYBAR_CONFIG_DIR/style.css"
    elif [ -f "$BACKUP_DIR/style.css.backup" ]; then
        # Restore as regular file
        cp "$BACKUP_DIR/style.css.backup" "$WAYBAR_CONFIG_DIR/style.css"
    fi
    
    # Remove dev mode marker
    rm -f "$BACKUP_DIR/.dev-mode-active"
    
    # Start waybar with NixOS config
    log "Starting waybar with NixOS config..."
    waybar &
    
    success "Development mode stopped!"
    success "✅ NixOS-managed configs restored"
    success "✅ Waybar restarted with system config"
}

reload_waybar() {
    log "Reloading waybar..."
    
    # Check if in dev mode
    if [ ! -f "$BACKUP_DIR/.dev-mode-active" ]; then
        warn "Not in development mode. Use 'start' first."
        return 1
    fi
    
    # Stop waybar
    if check_waybar_running; then
        pkill waybar || true
        sleep 1
    fi
    
    # Start waybar again
    waybar &
    
    success "Waybar reloaded with development config!"
}

test_jasper() {
    log "Testing Jasper output..."
    
    # Build first
    cd "$SCRIPT_DIR"
    cargo build
    
    # Test the output
    echo "JSON output:"
    ./waybar-jasper.sh
    echo
    echo "Simple output:"
    ./waybar-jasper.sh --simple 2>/dev/null || echo "Simple mode not available"
}

status() {
    if [ -f "$BACKUP_DIR/.dev-mode-active" ]; then
        success "Development mode is ACTIVE"
        success "✅ Using development configs"
        success "✅ Local build: $(ls -la $SCRIPT_DIR/target/debug/jasper-companion-daemon 2>/dev/null || echo 'not built')"
        if check_waybar_running; then
            success "✅ Waybar is running"
        else
            warn "⚠️  Waybar is not running"
        fi
    else
        log "Development mode is INACTIVE"
        log "📋 Using NixOS-managed configs"
        if check_waybar_running; then
            log "✅ Waybar is running (NixOS config)"
        else
            warn "⚠️  Waybar is not running"
        fi
    fi
}

case "${1:-help}" in
    start)
        start_dev_mode
        ;;
    stop)
        stop_dev_mode
        ;;
    reload)
        reload_waybar
        ;;
    test)
        test_jasper
        ;;
    status)
        status
        ;;
    help|--help|-h)
        echo "Jasper Development Mode"
        echo "Usage: $0 {start|stop|reload|test|status|help}"
        echo
        echo "Commands:"
        echo "  start   - Enter development mode (use local configs)"
        echo "  stop    - Exit development mode (restore NixOS configs)"
        echo "  reload  - Restart waybar with current development config"
        echo "  test    - Test Jasper output"
        echo "  status  - Show current mode status"
        echo "  help    - Show this help message"
        ;;
    *)
        error "Unknown command: $1"
        echo "Use '$0 help' for usage information"
        exit 1
        ;;
esac