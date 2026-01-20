#!/usr/bin/env bash

# Jasper Development Helper for Ubuntu
# Simplified development workflow without NixOS dependencies

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_ROOT="$SCRIPT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${BLUE}[jasper-dev]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[jasper-dev]${NC} $1"
}

error() {
    echo -e "${RED}[jasper-dev]${NC} $1"
}

success() {
    echo -e "${GREEN}[jasper-dev]${NC} $1"
}

check_dependencies() {
    local missing=0

    if ! command -v cargo >/dev/null 2>&1; then
        error "Rust/Cargo not found. Install from https://rustup.rs/"
        missing=1
    fi

    if ! command -v pkg-config >/dev/null 2>&1; then
        error "pkg-config not found. Run: sudo apt-get install pkg-config"
        missing=1
    fi

    if [ $missing -eq 1 ]; then
        error "Missing dependencies. Run: ./ubuntu/install-deps.sh"
        return 1
    fi

    return 0
}

build() {
    log "Building Jasper daemon..."
    cd "$PROJECT_ROOT/daemon"

    if [ "${1:-release}" = "debug" ]; then
        cargo build
        success "Debug build complete: target/debug/jasper-companion-daemon"
    else
        cargo build --release
        success "Release build complete: target/release/jasper-companion-daemon"
    fi
}

build_debug() {
    build debug
}

test_build() {
    log "Running tests..."
    cd "$PROJECT_ROOT/daemon"
    cargo test
    success "Tests passed ✓"
}

run_daemon() {
    log "Running daemon directly (not via systemd)..."
    cd "$PROJECT_ROOT"

    if [ -f "target/debug/jasper-companion-daemon" ]; then
        RUST_LOG=debug target/debug/jasper-companion-daemon start
    elif [ -f "target/release/jasper-companion-daemon" ]; then
        target/release/jasper-companion-daemon start
    else
        error "No build found. Run: $0 build"
        return 1
    fi
}

test_waybar() {
    log "Testing Waybar JSON output..."
    cd "$PROJECT_ROOT"

    if [ -f "target/debug/jasper-companion-daemon" ]; then
        target/debug/jasper-companion-daemon waybar
    elif [ -f "target/release/jasper-companion-daemon" ]; then
        target/release/jasper-companion-daemon waybar
    else
        error "No build found. Run: $0 build"
        return 1
    fi
}

install_dev() {
    log "Installing development build..."
    cd "$PROJECT_ROOT"

    if [ ! -f "target/debug/jasper-companion-daemon" ]; then
        warn "Debug build not found, building first..."
        build debug
    fi

    log "Copying to /usr/local/bin (requires sudo)..."
    sudo cp target/debug/jasper-companion-daemon /usr/local/bin/jasper-companion-daemon

    success "Development build installed to /usr/local/bin/jasper-companion-daemon"
    log "Restart daemon: systemctl --user restart jasper-companion"
}

restart_daemon() {
    log "Restarting daemon..."
    systemctl --user restart jasper-companion
    success "Daemon restarted ✓"

    log "Checking status..."
    systemctl --user status jasper-companion --no-pager -l
}

watch_logs() {
    log "Watching daemon logs (Ctrl+C to exit)..."
    journalctl --user -u jasper-companion -f
}

install_extension() {
    log "Installing GNOME Shell extension..."
    cd "$PROJECT_ROOT"
    make install-extension
}

reload_extension() {
    log "Note: GNOME Shell extension requires shell restart"
    echo ""
    echo "To reload extension:"
    echo "  X11: Press Alt+F2, type 'r', press Enter"
    echo "  Wayland: Log out and log back in"
    echo ""
    echo "Or use: make install-extension"
}

status() {
    log "Jasper Development Status"
    echo ""

    # Check builds
    if [ -f "$PROJECT_ROOT/target/release/jasper-companion-daemon" ]; then
        success "✓ Release build exists"
    else
        warn "✗ No release build"
    fi

    if [ -f "$PROJECT_ROOT/target/debug/jasper-companion-daemon" ]; then
        success "✓ Debug build exists"
    else
        warn "✗ No debug build"
    fi

    # Check installation
    if [ -f "/usr/local/bin/jasper-companion-daemon" ]; then
        success "✓ Installed to /usr/local/bin"
    else
        warn "✗ Not installed system-wide"
    fi

    # Check daemon service
    if systemctl --user is-active jasper-companion >/dev/null 2>&1; then
        success "✓ Daemon service is running"
    else
        warn "✗ Daemon service is not running"
    fi

    # Check extension
    if [ -d "$HOME/.local/share/gnome-shell/extensions/jasper-companion@heytcass.github" ]; then
        success "✓ GNOME extension installed"
        if command -v gnome-extensions >/dev/null 2>&1; then
            if gnome-extensions info jasper-companion@heytcass.github >/dev/null 2>&1; then
                if gnome-extensions list --enabled | grep -q "jasper-companion@heytcass.github"; then
                    success "  ✓ Extension enabled"
                else
                    warn "  ✗ Extension disabled"
                fi
            fi
        fi
    else
        log "  No GNOME extension installed"
    fi

    echo ""
}

clean() {
    log "Cleaning build artifacts..."
    cd "$PROJECT_ROOT/daemon"
    cargo clean
    success "Clean complete ✓"
}

quick_cycle() {
    log "Quick development cycle: build + install + restart"
    build debug || return 1
    install_dev || return 1
    restart_daemon || return 1
    success "Development cycle complete!"
}

show_help() {
    echo "Jasper Development Helper for Ubuntu"
    echo "Usage: $0 <command>"
    echo ""
    echo "Build Commands:"
    echo "  build           - Build release binary"
    echo "  build-debug     - Build debug binary"
    echo "  test            - Run tests"
    echo "  clean           - Clean build artifacts"
    echo ""
    echo "Development Commands:"
    echo "  install-dev     - Install debug build to system (requires sudo)"
    echo "  run             - Run daemon directly (not via systemd)"
    echo "  restart         - Restart systemd service"
    echo "  logs            - Watch daemon logs"
    echo "  quick           - Quick cycle: build + install + restart"
    echo ""
    echo "Frontend Commands:"
    echo "  test-waybar     - Test Waybar JSON output"
    echo "  install-ext     - Install GNOME Shell extension"
    echo "  reload-ext      - Show instructions to reload extension"
    echo ""
    echo "Info Commands:"
    echo "  status          - Show development status"
    echo "  help            - Show this help"
    echo ""
    echo "Quick Development Workflow:"
    echo "  1. Edit Rust code in daemon/src/"
    echo "  2. Run: $0 quick"
    echo "  3. Check: $0 logs"
    echo ""
    echo "For full documentation, see: docs/UBUNTU_SETUP.md"
}

# Main command dispatcher
case "${1:-help}" in
    build)
        check_dependencies && build release
        ;;
    build-debug)
        check_dependencies && build debug
        ;;
    test)
        check_dependencies && test_build
        ;;
    run)
        check_dependencies && run_daemon
        ;;
    test-waybar)
        check_dependencies && test_waybar
        ;;
    install-dev)
        check_dependencies && install_dev
        ;;
    restart)
        restart_daemon
        ;;
    logs)
        watch_logs
        ;;
    install-ext|install-extension)
        install_extension
        ;;
    reload-ext|reload-extension)
        reload_extension
        ;;
    status)
        status
        ;;
    clean)
        clean
        ;;
    quick)
        check_dependencies && quick_cycle
        ;;
    help|--help|-h)
        show_help
        ;;
    *)
        error "Unknown command: $1"
        echo ""
        show_help
        exit 1
        ;;
esac
