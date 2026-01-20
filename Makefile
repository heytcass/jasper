# Jasper Companion - Ubuntu/Debian Build System
# Compatible with Ubuntu 25.11 and other Debian-based systems

.PHONY: all build dev test install uninstall install-extension uninstall-extension clean check-deps help

# Build configuration
CARGO := cargo
CARGO_FLAGS := --release
DEBUG_FLAGS :=
PREFIX := /usr/local
BINDIR := $(PREFIX)/bin
DATADIR := $(PREFIX)/share
SYSTEMD_USER_DIR := /etc/systemd/user
DBUS_SERVICES_DIR := $(DATADIR)/dbus-1/services

# Extension configuration
EXTENSION_UUID := jasper-companion@heytcass.github
EXTENSION_SRC := gnome-extension
EXTENSION_INSTALL_DIR := $(HOME)/.local/share/gnome-shell/extensions/$(EXTENSION_UUID)

# Project paths
DAEMON_BIN := target/release/jasper-companion-daemon
DAEMON_DEBUG_BIN := target/debug/jasper-companion-daemon

# Default target
all: build

## Help
help:
	@echo "Jasper Companion - Ubuntu Build System"
	@echo ""
	@echo "Available targets:"
	@echo "  make build              - Build release binary"
	@echo "  make dev                - Build debug binary"
	@echo "  make test               - Run tests"
	@echo "  make install            - Install daemon system-wide (requires sudo)"
	@echo "  make uninstall          - Uninstall daemon (requires sudo)"
	@echo "  make install-extension  - Install GNOME Shell extension"
	@echo "  make uninstall-extension- Uninstall GNOME Shell extension"
	@echo "  make clean              - Clean build artifacts"
	@echo "  make check-deps         - Check system dependencies"
	@echo ""
	@echo "Installation directories:"
	@echo "  PREFIX=$(PREFIX)"
	@echo "  BINDIR=$(BINDIR)"
	@echo "  SYSTEMD_USER_DIR=$(SYSTEMD_USER_DIR)"
	@echo "  DBUS_SERVICES_DIR=$(DBUS_SERVICES_DIR)"

## Check system dependencies
check-deps:
	@echo "Checking system dependencies..."
	@command -v cargo >/dev/null 2>&1 || { echo "Error: Rust/Cargo not found. Install from https://rustup.rs/"; exit 1; }
	@command -v pkg-config >/dev/null 2>&1 || { echo "Error: pkg-config not found. Run: sudo apt-get install pkg-config"; exit 1; }
	@pkg-config --exists libssl || { echo "Error: libssl-dev not found. Run: sudo apt-get install libssl-dev"; exit 1; }
	@pkg-config --exists sqlite3 || { echo "Error: libsqlite3-dev not found. Run: sudo apt-get install libsqlite3-dev"; exit 1; }
	@pkg-config --exists dbus-1 || { echo "Error: libdbus-1-dev not found. Run: sudo apt-get install libdbus-1-dev"; exit 1; }
	@echo "All dependencies satisfied ✓"

## Build release binary
build: check-deps
	@echo "Building Jasper Companion daemon (release mode)..."
	cd daemon && $(CARGO) build $(CARGO_FLAGS)
	@echo "Build complete: $(DAEMON_BIN)"

## Build debug binary
dev: check-deps
	@echo "Building Jasper Companion daemon (debug mode)..."
	cd daemon && $(CARGO) build $(DEBUG_FLAGS)
	@echo "Build complete: $(DAEMON_DEBUG_BIN)"

## Run tests
test: check-deps
	@echo "Running tests..."
	cd daemon && $(CARGO) test

## Install daemon and services (requires sudo)
install: build
	@echo "Installing Jasper Companion..."

	# Install binary
	@echo "Installing daemon binary to $(BINDIR)..."
	install -D -m 755 $(DAEMON_BIN) $(BINDIR)/jasper-companion-daemon

	# Install D-Bus service file
	@echo "Installing D-Bus service file..."
	install -d $(DBUS_SERVICES_DIR)
	sed 's|@BINDIR@|$(BINDIR)|g' ubuntu/org.jasper.Daemon.service.in | install -D -m 644 /dev/stdin $(DBUS_SERVICES_DIR)/org.jasper.Daemon.service

	# Install systemd user service
	@echo "Installing systemd user service..."
	install -d $(SYSTEMD_USER_DIR)
	sed 's|@BINDIR@|$(BINDIR)|g' ubuntu/jasper-companion.service.in | install -D -m 644 /dev/stdin $(SYSTEMD_USER_DIR)/jasper-companion.service

	@echo ""
	@echo "Installation complete! ✓"
	@echo ""
	@echo "Next steps:"
	@echo "  1. Set API key: jasper-companion-daemon set-api-key <your-key>"
	@echo "  2. Enable service: systemctl --user enable jasper-companion"
	@echo "  3. Start service: systemctl --user start jasper-companion"
	@echo "  4. Install extension: make install-extension (for GNOME users)"
	@echo ""
	@echo "Note: You may need to reload systemd: systemctl --user daemon-reload"

## Uninstall daemon and services (requires sudo)
uninstall:
	@echo "Uninstalling Jasper Companion..."
	@echo "Note: This does not remove user data in ~/.config/jasper-companion or ~/.local/share/jasper-companion"

	# Stop service if running
	-systemctl --user stop jasper-companion 2>/dev/null || true
	-systemctl --user disable jasper-companion 2>/dev/null || true

	# Remove files
	rm -f $(BINDIR)/jasper-companion-daemon
	rm -f $(DBUS_SERVICES_DIR)/org.jasper.Daemon.service
	rm -f $(SYSTEMD_USER_DIR)/jasper-companion.service

	@echo "Uninstallation complete ✓"
	@echo "To remove user data: rm -rf ~/.config/jasper-companion ~/.local/share/jasper-companion"

## Install GNOME Shell extension
install-extension:
	@echo "Installing GNOME Shell extension..."

	# Create extension directory
	install -d $(EXTENSION_INSTALL_DIR)

	# Copy extension files
	cp $(EXTENSION_SRC)/extension.js $(EXTENSION_INSTALL_DIR)/
	cp $(EXTENSION_SRC)/metadata.json $(EXTENSION_INSTALL_DIR)/

	# Update version in metadata if needed
	@if [ -f $(EXTENSION_SRC)/metadata.json ]; then \
		cp $(EXTENSION_SRC)/metadata.json $(EXTENSION_INSTALL_DIR)/metadata.json; \
	fi

	@echo "Extension installed to: $(EXTENSION_INSTALL_DIR)"
	@echo ""
	@echo "Next steps:"
	@echo "  1. Restart GNOME Shell:"
	@echo "     - X11: Press Alt+F2, type 'r', press Enter"
	@echo "     - Wayland: Log out and log back in"
	@echo "  2. Enable extension: gnome-extensions enable $(EXTENSION_UUID)"
	@echo ""
	@echo "Check status: gnome-extensions info $(EXTENSION_UUID)"

## Uninstall GNOME Shell extension
uninstall-extension:
	@echo "Uninstalling GNOME Shell extension..."

	# Disable extension if enabled
	-gnome-extensions disable $(EXTENSION_UUID) 2>/dev/null || true

	# Remove extension directory
	rm -rf $(EXTENSION_INSTALL_DIR)

	@echo "Extension uninstalled ✓"
	@echo "Restart GNOME Shell to complete removal"

## Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	cd daemon && $(CARGO) clean
	rm -rf target/
	@echo "Clean complete ✓"

## Development helpers
.PHONY: run run-debug logs restart reload-extension

run: build
	@echo "Running daemon directly (not via systemd)..."
	$(DAEMON_BIN) start

run-debug: dev
	@echo "Running daemon in debug mode..."
	RUST_LOG=debug $(DAEMON_DEBUG_BIN) start

logs:
	@echo "Showing daemon logs (Ctrl+C to exit)..."
	journalctl --user -u jasper-companion -f

restart:
	@echo "Restarting daemon..."
	systemctl --user restart jasper-companion
	@echo "Daemon restarted ✓"

reload-extension:
	@echo "Note: GNOME Shell extension requires shell restart"
	@echo "X11: Press Alt+F2, type 'r', press Enter"
	@echo "Wayland: Log out and log back in"
