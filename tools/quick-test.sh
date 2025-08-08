#!/usr/bin/env bash

# Quick test script for rapid Jasper development iteration

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKUP_DIR="$SCRIPT_DIR/.dev-backups"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${BLUE}[quick-test]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[quick-test]${NC} $1"
}

error() {
    echo -e "${RED}[quick-test]${NC} $1"
}

success() {
    echo -e "${GREEN}[quick-test]${NC} $1"
}

# Check if in dev mode
check_dev_mode() {
    if [ ! -f "$BACKUP_DIR/.dev-mode-active" ]; then
        error "Not in development mode!"
        echo "Run './dev-mode.sh start' first"
        exit 1
    fi
}

# Quick build and test
quick_build() {
    log "Building Jasper..."
    cd "$SCRIPT_DIR"
    cargo build
    success "Build complete"
}

# Test Jasper output
test_jasper() {
    log "Testing Jasper output..."
    
    # Build first
    quick_build
    
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "JSON output:"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    ./waybar-jasper.sh | jq . || ./waybar-jasper.sh
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    
    success "Jasper test complete"
}

# Reload waybar quickly
reload_waybar() {
    log "Reloading waybar..."
    pkill waybar || true
    sleep 0.5
    waybar &
    success "Waybar reloaded"
}

# Full test cycle
full_test() {
    log "Running full test cycle..."
    
    # 1. Build
    quick_build
    
    # 2. Test output
    log "Testing JSON output..."
    ./waybar-jasper.sh > /tmp/jasper-test.json
    
    # 3. Validate JSON
    if command -v jq > /dev/null; then
        if jq . /tmp/jasper-test.json > /dev/null 2>&1; then
            success "✅ JSON is valid"
        else
            error "❌ JSON is invalid!"
            cat /tmp/jasper-test.json
            exit 1
        fi
    else
        warn "jq not found, skipping JSON validation"
    fi
    
    # 4. Show output
    log "JSON output:"
    cat /tmp/jasper-test.json | jq . || cat /tmp/jasper-test.json
    
    # 5. Reload waybar
    reload_waybar
    
    success "Full test cycle complete!"
}

# CSS live editing mode
css_watch() {
    log "Starting CSS live editing mode..."
    log "Watching waybar/style.css for changes..."
    
    if ! command -v entr > /dev/null; then
        error "entr not found. Install with: nix-shell -p entr"
        exit 1
    fi
    
    # Watch for CSS changes and reload waybar
    echo "$SCRIPT_DIR/waybar/style.css" | entr -s 'echo "CSS changed, reloading waybar..." && pkill waybar || true && sleep 0.5 && waybar &'
}

# Show current status
show_status() {
    if [ -f "$BACKUP_DIR/.dev-mode-active" ]; then
        success "Development mode is ACTIVE"
        
        # Check if build exists
        if [ -f "$SCRIPT_DIR/target/debug/jasper-companion-daemon" ]; then
            success "✅ Local build exists"
        else
            warn "⚠️  Local build missing - run 'cargo build'"
        fi
        
        # Check if waybar is running
        if pgrep waybar > /dev/null; then
            success "✅ Waybar is running"
        else
            warn "⚠️  Waybar is not running"
        fi
        
        # Show last modification times
        log "Last modified:"
        log "  Rust code: $(stat -c %y daemon/src/waybar_formatter.rs 2>/dev/null || echo 'unknown')"
        log "  CSS: $(stat -c %y waybar/style.css 2>/dev/null || echo 'unknown')"
        log "  Config: $(stat -c %y waybar/config.json 2>/dev/null || echo 'unknown')"
    else
        error "Development mode is INACTIVE"
        echo "Run './dev-mode.sh start' to begin development"
    fi
}

case "${1:-help}" in
    build)
        check_dev_mode
        quick_build
        ;;
    test)
        check_dev_mode
        test_jasper
        ;;
    reload)
        check_dev_mode
        reload_waybar
        ;;
    full)
        check_dev_mode
        full_test
        ;;
    css)
        check_dev_mode
        css_watch
        ;;
    status)
        show_status
        ;;
    help|--help|-h)
        echo "Jasper Quick Test Script"
        echo "Usage: $0 {build|test|reload|full|css|status|help}"
        echo
        echo "Commands:"
        echo "  build   - Quick cargo build"
        echo "  test    - Build and test Jasper JSON output"
        echo "  reload  - Reload waybar quickly"
        echo "  full    - Full test cycle (build + test + reload)"
        echo "  css     - Live CSS editing mode (auto-reload on changes)"
        echo "  status  - Show current development status"
        echo "  help    - Show this help message"
        echo
        echo "Note: Must be in development mode first (./dev-mode.sh start)"
        ;;
    *)
        error "Unknown command: $1"
        echo "Use '$0 help' for usage information"
        exit 1
        ;;
esac