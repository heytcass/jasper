# COSMIC Desktop Integration Plan

## Overview

Add a COSMIC desktop panel applet and companion settings application to Jasper, enabling native integration with the COSMIC desktop environment alongside the existing GNOME Shell extension and Waybar frontends.

The COSMIC applet communicates with the existing `jasper-companion-daemon` over D-Bus -- the daemon requires **zero modifications**. All new code lives in two new workspace crates.

---

## Architecture

```
┌──────────────────────────────────────────────────────┐
│  COSMIC Panel                                        │
│  ┌────────────────────────────────────────────────┐  │
│  │  jasper-cosmic-applet                          │  │
│  │                                                │  │
│  │  Panel icon: emoji from latest insight         │  │
│  │  Popup: full insight + quick toggles           │  │
│  │  "Jasper Settings..." button ──────────────────┼──┼──► jasper-cosmic-settings
│  │                                                │  │
│  │  Own config: ~/.config/cosmic/                 │  │
│  │    com.system76.CosmicAppletJasper/v1/         │  │
│  └───────────────┬────────────────────────────────┘  │
└──────────────────┼───────────────────────────────────┘
                   │ D-Bus (org.jasper.Daemon1)
┌──────────────────▼───────────────────────────────────┐
│  jasper-companion-daemon (UNCHANGED)                 │
│                                                      │
│  Config: ~/.config/jasper-companion/config.toml      │
│  Database: ~/.local/share/jasper-companion/jasper.db  │
└──────────────────────────────────────────────────────┘
```

### Two config stores, each owning their domain

| Store | Format | Location | Owns |
|-------|--------|----------|------|
| cosmic-config | RON | `~/.config/cosmic/com.system76.CosmicAppletJasper/v1/` | Applet presentation preferences |
| config.toml | TOML | `~/.config/jasper-companion/config.toml` | Daemon behavior, AI, calendars, context sources |

---

## Workspace Changes

### New Crates

```
jasper/
├── Cargo.toml                          # Add new workspace members
├── daemon/                             # UNCHANGED
├── cosmic-applet/                      # NEW - Phase 1
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs                     # Entry point: cosmic::applet::run
│   │   ├── app.rs                      # Application trait implementation
│   │   ├── config.rs                   # CosmicConfigEntry for applet prefs
│   │   ├── dbus_client.rs              # zbus proxy for daemon communication
│   │   └── i18n.rs                     # Localization support
│   ├── i18n/
│   │   └── en/
│   │       └── cosmic_applet_jasper.ftl
│   ├── data/
│   │   └── com.system76.CosmicAppletJasper.desktop
│   └── resources/
│       └── icons/                      # Applet icons (fallback when no emoji)
├── cosmic-settings/                    # NEW - Phase 3
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs                     # Entry point: cosmic::app::run
│   │   ├── app.rs                      # Full Application with nav sidebar
│   │   ├── pages/
│   │   │   ├── general.rs              # Planning horizon, analysis interval
│   │   │   ├── ai.rs                   # Provider, model, temperature
│   │   │   ├── personality.rs          # Persona, formality, humor
│   │   │   ├── calendar.rs             # Google Calendar setup + OAuth flow
│   │   │   ├── context_sources.rs      # Obsidian, weather, tasks
│   │   │   └── notifications.rs        # Notification preferences
│   │   ├── config_bridge.rs            # Read/write daemon's config.toml
│   │   └── i18n.rs
│   └── i18n/
│       └── en/
├── gnome-extension/                    # UNCHANGED
├── nix/
│   ├── cosmic-applet.nix               # NEW - Nix package for applet
│   ├── cosmic-settings.nix             # NEW - Nix package for settings app
│   ├── unified-package.nix             # MODIFIED - Add COSMIC support
│   └── unified-module.nix              # MODIFIED - Add COSMIC detection
└── ...
```

### Cargo.toml Changes

```toml
[workspace]
members = [
    "daemon",
    "cosmic-applet",       # Phase 1
    "cosmic-settings",     # Phase 3
]
resolver = "2"

[workspace.dependencies]
# ... existing deps unchanged ...

# COSMIC (shared by applet + settings)
zbus = "4.0"  # already present
tokio = { version = "1.0", features = ["rt-multi-thread", "macros", "fs", "time", "signal", "sync"] }  # already present
serde = { version = "1.0", features = ["derive"] }  # already present
chrono = { version = "0.4", features = ["serde"] }   # already present
tracing = "0.1"  # already present
```

---

## Phase 1: Panel Applet (MVP)

**Goal**: Working COSMIC panel applet that displays insights from the daemon, with a popup showing the full insight and a refresh button.

### 1.1 Create `cosmic-applet/`

#### `cosmic-applet/Cargo.toml`

```toml
[package]
name = "jasper-cosmic-applet"
version = "0.2.0"
edition = "2021"
license = "MIT"
description = "COSMIC panel applet for Jasper AI companion"

[[bin]]
name = "jasper-cosmic-applet"
path = "src/main.rs"

[dependencies]
# COSMIC toolkit
libcosmic = { git = "https://github.com/pop-os/libcosmic", default-features = false, features = [
    "applet",
    "dbus-config",
    "tokio",
    "wayland",
] }

# D-Bus client (same version as daemon)
zbus.workspace = true

# Async runtime
tokio.workspace = true
futures-util = "0.3"

# Serialization
serde.workspace = true

# Logging
tracing.workspace = true

# i18n
i18n-embed = { version = "0.16", features = ["fluent-system", "desktop-requester"] }
i18n-embed-fl = "0.10"
rust-embed = "8.7"
```

#### `cosmic-applet/src/config.rs`

```rust
use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};
use serde::{Deserialize, Serialize};

pub const CONFIG_ID: &str = "com.system76.CosmicAppletJasper";

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, CosmicConfigEntry)]
#[version = 1]
pub struct JasperAppletConfig {
    /// Show truncated insight text next to emoji in panel
    pub show_text_in_panel: bool,

    /// Max characters to show in panel (when show_text_in_panel is true)
    pub panel_text_max_chars: u32,

    /// How often to poll the daemon via D-Bus (seconds)
    pub poll_interval_secs: u32,

    /// Suppress insight notifications while applet is visible
    pub quiet_mode: bool,
}

// Defaults that make sense for a panel applet
impl JasperAppletConfig {
    pub fn defaults() -> Self {
        Self {
            show_text_in_panel: false,      // emoji only by default
            panel_text_max_chars: 30,
            poll_interval_secs: 10,
            quiet_mode: false,
        }
    }
}
```

#### `cosmic-applet/src/dbus_client.rs`

```rust
use zbus::proxy;

/// D-Bus proxy matching the daemon's org.jasper.Daemon1 interface.
/// Generates JasperDaemonProxy and JasperDaemonProxyBlocking types.
#[proxy(
    interface = "org.jasper.Daemon1",
    default_service = "org.jasper.Daemon",
    default_path = "/org/jasper/Daemon"
)]
trait JasperDaemon {
    fn get_latest_insight(&self) -> zbus::Result<(i64, String, String, String)>;
    fn register_frontend(&self, frontend_id: &str, pid: i32) -> zbus::Result<bool>;
    fn unregister_frontend(&self, frontend_id: &str) -> zbus::Result<bool>;
    fn heartbeat(&self, frontend_id: &str) -> zbus::Result<bool>;
    fn force_refresh(&self) -> zbus::Result<bool>;
    fn get_status(&self) -> zbus::Result<(bool, u32, i64)>;

    #[zbus(signal)]
    fn insight_updated(&self, insight_id: i64, emoji: String, preview: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn daemon_stopping(&self) -> zbus::Result<()>;
}
```

#### `cosmic-applet/src/app.rs` (Application trait)

**State struct:**

```rust
pub struct JasperApplet {
    core: cosmic::app::Core,
    popup: Option<window::Id>,

    // Insight state (from D-Bus)
    current_insight_id: i64,
    current_emoji: String,
    current_text: String,
    context_hash: String,
    daemon_online: bool,

    // Config
    config: JasperAppletConfig,
}
```

**Message enum:**

```rust
pub enum Message {
    // Popup lifecycle
    TogglePopup,
    PopupClosed(window::Id),

    // D-Bus data
    InsightReceived(i64, String, String, String),
    DaemonOffline,
    DaemonStatusReceived(bool, u32, i64),

    // User actions
    ForceRefresh,
    RefreshComplete(bool),

    // Config
    ConfigChanged(JasperAppletConfig),
    ToggleShowTextInPanel(cosmic::widget::segmented_button::Entity, bool),
    ToggleQuietMode(cosmic::widget::segmented_button::Entity, bool),

    // Background
    PollTick,
    HeartbeatTick,
}
```

**Key trait methods:**

- `init()`: Load config, set initial state, register frontend via D-Bus
- `view()`: Render panel icon button (emoji, optionally + truncated text)
- `view_window()`: Render popup with:
  - Full insight text (wrapped, scrollable if long)
  - Separator
  - "Quiet Mode" toggle (writes to cosmic-config)
  - "Show text in panel" toggle (writes to cosmic-config)
  - Separator
  - "Jasper Settings..." button (launches `jasper-cosmic-settings`)
  - "Refresh" button (calls `ForceRefresh()` D-Bus)
- `update()`: Handle all messages, D-Bus calls, config persistence
- `subscription()`: Returns batch of:
  - `cosmic::iced::time::every(poll_interval)` -> `PollTick`
  - `cosmic::iced::time::every(5 seconds)` -> `HeartbeatTick`
  - `self.core.watch_config(APP_ID)` -> `ConfigChanged`

**Popup sizing** (following official applet patterns):

```rust
fn view_window(&self, id: window::Id) -> cosmic::Element<Message> {
    // 300px min width, up to 400px, height auto
    self.core.applet.popup_container(content).into()
}
```

#### `cosmic-applet/src/main.rs`

```rust
mod app;
mod config;
mod dbus_client;
mod i18n;

fn main() -> cosmic::iced::Result {
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    i18n::init(&requested_languages);
    cosmic::applet::run::<app::JasperApplet>(())
}
```

#### Desktop file: `cosmic-applet/data/com.system76.CosmicAppletJasper.desktop`

```ini
[Desktop Entry]
Name=Jasper AI Insights
Comment=AI-generated calendar insights for COSMIC panel
Exec=jasper-cosmic-applet
Icon=jasper-companion
Type=Application
NoDisplay=true
X-CosmicApplet=true
```

### 1.2 D-Bus Client Implementation

The applet acts as a D-Bus client using `zbus`. Implementation notes:

- **Connection**: Create `zbus::Connection::session()` at init, store in app state
- **Proxy**: Use generated `JasperDaemonProxy` from the `#[proxy]` macro
- **Error handling**: If proxy call fails, set `daemon_online = false` and show fallback UI
- **Frontend registration**: Register as `"cosmic-applet"` with PID on init, heartbeat every 5s
- **Signal subscription**: Listen for `InsightUpdated` signal in addition to polling (belt and suspenders)
- **Cleanup**: Unregister frontend on applet destruction (`on_close`)

### 1.3 Build & Test

```bash
# Build applet (requires COSMIC development libraries)
cd cosmic-applet && cargo build

# Run in COSMIC session for testing
cargo run

# Install to system for panel to discover
sudo install -Dm755 target/release/jasper-cosmic-applet /usr/local/bin/
sudo install -Dm644 data/com.system76.CosmicAppletJasper.desktop \
    /usr/share/applications/com.system76.CosmicAppletJasper.desktop
```

### 1.4 Deliverables

- [ ] `cosmic-applet/` crate compiles against libcosmic
- [ ] Panel icon shows current insight emoji
- [ ] Popup shows full insight text
- [ ] Refresh button triggers `ForceRefresh()` over D-Bus
- [ ] Applet gracefully handles daemon being offline
- [ ] Config persists via cosmic-config (poll interval, panel text toggle)

---

## Phase 2: Applet Polish & Quick Settings

**Goal**: Add in-popup settings, improve UX, handle edge cases.

### 2.1 In-Popup Quick Settings

Add animated toggles directly in the popup (matching the pattern from `cosmic-applet-audio` and `cosmic-applet-notifications`):

```rust
// In view_window()
let quiet_mode = padded_control(anim!(
    QUIET_MODE,
    &self.timeline,
    fl!("quiet-mode"),
    self.config.quiet_mode,
    Message::ToggleQuietMode,
));

let show_text = padded_control(anim!(
    SHOW_TEXT,
    &self.timeline,
    fl!("show-text-in-panel"),
    self.config.show_text_in_panel,
    Message::ToggleShowTextInPanel,
));
```

### 2.2 Urgency Indicator

Color-code the panel icon background based on insight urgency:
- High urgency (days <= `high_urgency_days`): accent color
- Medium urgency: warning color
- Low urgency: default

This requires the daemon to expose urgency in the D-Bus response. **Options**:
- Parse urgency from the insight text client-side (fragile)
- Add an urgency field to `GetLatestInsight()` return value (daemon change, backward compatible if we extend the tuple)
- Use a separate `GetInsightUrgency(id)` method

**Recommendation**: Defer urgency coloring to Phase 3 when the settings app can expose the configuration. For Phase 2, keep the emoji as the sole indicator.

### 2.3 Tooltip Support

When hovering over the panel icon, show a tooltip with:
- Insight preview text (first ~100 chars)
- Time since last update
- Daemon status (online/offline, # active frontends)

### 2.4 Deliverables

- [ ] Animated toggle controls in popup
- [ ] Config changes persist and take effect immediately (via subscription)
- [ ] Tooltip on panel hover
- [ ] Smooth animations for popup open/close
- [ ] Localization strings for all UI text

---

## Phase 3: Companion Settings Application

**Goal**: Full COSMIC application for configuring all daemon settings via a GUI.

### 3.1 Create `cosmic-settings/`

This is a full `cosmic::Application` (not an applet) with a navigation sidebar and form-based pages.

#### `cosmic-settings/Cargo.toml`

```toml
[package]
name = "jasper-cosmic-settings"
version = "0.2.0"
edition = "2021"

[[bin]]
name = "jasper-cosmic-settings"
path = "src/main.rs"

[dependencies]
libcosmic = { git = "https://github.com/pop-os/libcosmic", features = [
    "dbus-config",
    "multi-window",
    "single-instance",
    "tokio",
    "wayland",
    "wgpu",
] }
zbus.workspace = true
tokio.workspace = true
serde.workspace = true
toml.workspace = true
tracing.workspace = true
futures-util = "0.3"
i18n-embed = { version = "0.16", features = ["fluent-system", "desktop-requester"] }
i18n-embed-fl = "0.10"
rust-embed = "8.7"
open = "5"
```

### 3.2 Navigation Structure

```
┌─────────────────┬──────────────────────────────────┐
│ Navigation      │ Content                          │
│                 │                                  │
│ > General       │  Planning Horizon                │
│   AI Provider   │  [===7=======] days              │
│   Personality   │                                  │
│   Calendar      │  Analysis Interval               │
│   Context       │  [===30======] minutes            │
│   Notifications │                                  │
│                 │  Timezone                         │
│                 │  [America/Detroit ▾]              │
│                 │                                  │
│                 │  Log Level                        │
│                 │  [info ▾]                         │
└─────────────────┴──────────────────────────────────┘
```

**Pages map to daemon config sections:**

| Page | Config Section | Key Controls |
|------|---------------|-------------|
| General | `[general]` | Sliders, dropdowns |
| AI Provider | `[ai]` | Provider radio (Anthropic/OpenAI), model text input, temperature slider, API key secure input |
| Personality | `[personality]` | Text inputs (user_title, persona_reference), radio buttons (formality, humor_level), multiline (assistant_persona) |
| Calendar | `[google_calendar]` | OAuth flow button, calendar ID list, sync interval slider |
| Context Sources | `[context_sources]` | Section toggles (Obsidian/Weather/Tasks), path pickers, nested settings |
| Notifications | `[notifications]` | Toggles for each notification type, urgency threshold slider, quiet hours time pickers |

### 3.3 Config Bridge

The settings app reads and writes `~/.config/jasper-companion/config.toml` directly:

```rust
// cosmic-settings/src/config_bridge.rs

pub fn load_daemon_config() -> Result<DaemonConfig> {
    let path = dirs::config_dir()
        .unwrap()
        .join("jasper-companion")
        .join("config.toml");
    let content = std::fs::read_to_string(&path)?;
    Ok(toml::from_str(&content)?)
}

pub fn save_daemon_config(config: &DaemonConfig) -> Result<()> {
    let path = dirs::config_dir()
        .unwrap()
        .join("jasper-companion")
        .join("config.toml");
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    Ok(())
}
```

After saving, the settings app calls `ForceRefresh()` over D-Bus so the daemon picks up the new config.

**Note**: The daemon already watches its config file via `notify` and reloads on changes. The D-Bus call is a courtesy to trigger an immediate re-analysis with the new settings.

### 3.4 Deliverables

- [ ] Settings app opens from applet popup button
- [ ] All daemon config sections editable via GUI
- [ ] Changes persist to config.toml
- [ ] Daemon reloads config on save
- [ ] OAuth flow for Google Calendar works from settings app
- [ ] API key fields use secure input (masked text)
- [ ] Single-instance enforcement (only one settings window at a time)

---

## Phase 4: NixOS Integration

**Goal**: Package the applet and settings app for NixOS, update desktop detection and unified module.

### 4.1 Nix Package: `nix/cosmic-applet.nix`

```nix
{ lib, pkgs, rustPlatform, libcosmic, ... }:

rustPlatform.buildRustPackage {
  pname = "jasper-cosmic-applet";
  version = "0.2.0";

  src = ../.;
  cargoBuildFlags = [ "-p" "jasper-cosmic-applet" ];

  buildInputs = with pkgs; [
    dbus
    libcosmic       # or build from git as an input
    wayland
    libxkbcommon
    vulkan-loader   # if using wgpu feature
  ];

  nativeBuildInputs = with pkgs; [
    pkg-config
    cmake           # needed by some libcosmic deps
  ];

  postInstall = ''
    install -Dm644 cosmic-applet/data/com.system76.CosmicAppletJasper.desktop \
      $out/share/applications/com.system76.CosmicAppletJasper.desktop
  '';
}
```

### 4.2 Update `nix/unified-package.nix`

Add COSMIC applet to the `paths` list and passthru:

```nix
# New input parameter
jasperCosmicApplet ? pkgs.callPackage ./cosmic-applet.nix { }

# Add to paths
paths = [
    jasperDaemon
    jasperGnomeExtension
    jasperCosmicApplet      # NEW
    dbusServiceFile
    desktopEntry
];

# Update passthru
passthru = {
    daemon = jasperDaemon;
    gnomeExtension = jasperGnomeExtension;
    cosmicApplet = jasperCosmicApplet;  # NEW

    supportsGnome = true;
    supportsWaybar = true;
    supportsCosmic = true;              # NEW
    supportsKde = false;
};
```

### 4.3 Update `nix/unified-module.nix`

Add COSMIC detection alongside GNOME and Waybar:

```nix
# Detection
hasCosmic = config.services.desktopManager.cosmic.enable or false;

# Frontend list
detectedFrontends =
    optionals hasGnome [ "gnome" ] ++
    optionals hasCosmic [ "cosmic" ] ++    # NEW
    optionals hasKde [ "kde" ] ++
    optionals hasWaybar [ "waybar" ];

# Frontend enum
forceEnableFrontends = mkOption {
    type = types.listOf (types.enum [ "gnome" "cosmic" "waybar" "kde" "terminal" ]);
    # ...
};

# COSMIC-specific configuration
systemd.user.services.jasper-cosmic-applet = mkIf (builtins.elem "cosmic" activeFrontends) {
    description = "Jasper COSMIC Panel Applet";
    after = [ "cosmic-panel.service" ];
    wantedBy = [ "cosmic-session.target" ];

    serviceConfig = {
        Type = "simple";
        ExecStart = "${cfg.package}/bin/jasper-cosmic-applet";
        Restart = "on-failure";
        RestartSec = 3;
    };
};
```

### 4.4 Update desktop detection script

Add COSMIC detection to `jasper-detect-desktop`:

```bash
pgrep -l cosmic-panel && echo "  - COSMIC Panel: Running" || echo "  - COSMIC Panel: Not running"
pgrep -l cosmic-comp && echo "  - COSMIC Compositor: Running" || echo "  - COSMIC Compositor: Not running"
```

### 4.5 Update `flake.nix`

Add new package outputs:

```nix
packages.cosmic-applet = pkgs.callPackage ./nix/cosmic-applet.nix { };
packages.cosmic-settings = pkgs.callPackage ./nix/cosmic-settings.nix { };
```

Add COSMIC development dependencies to devShell:

```nix
buildInputs = with pkgs; [
    # ... existing ...
    # COSMIC development
    wayland
    libxkbcommon
    vulkan-loader
];
```

### 4.6 Deliverables

- [ ] `nix build .#cosmic-applet` succeeds
- [ ] `nix build .#cosmic-settings` succeeds
- [ ] `nix build` (unified) includes COSMIC applet
- [ ] NixOS module auto-detects COSMIC and launches applet
- [ ] Desktop detection script identifies COSMIC sessions
- [ ] Applet `.desktop` file installed to correct location

---

## Phase 5: Ubuntu/Makefile Support

**Goal**: Add Makefile targets for building and installing the COSMIC components on Ubuntu/Debian.

### 5.1 Makefile Additions

```makefile
# COSMIC applet
build-cosmic-applet:
    cargo build --release -p jasper-cosmic-applet

install-cosmic-applet: build-cosmic-applet
    sudo install -Dm755 target/release/jasper-cosmic-applet /usr/local/bin/
    sudo install -Dm644 cosmic-applet/data/com.system76.CosmicAppletJasper.desktop \
        /usr/share/applications/com.system76.CosmicAppletJasper.desktop

# COSMIC settings app
build-cosmic-settings:
    cargo build --release -p jasper-cosmic-settings

install-cosmic-settings: build-cosmic-settings
    sudo install -Dm755 target/release/jasper-cosmic-settings /usr/local/bin/
    sudo install -Dm644 cosmic-settings/data/com.system76.CosmicJasperSettings.desktop \
        /usr/share/applications/com.system76.CosmicJasperSettings.desktop

# Combined
install-cosmic: install-cosmic-applet install-cosmic-settings
uninstall-cosmic:
    sudo rm -f /usr/local/bin/jasper-cosmic-applet
    sudo rm -f /usr/local/bin/jasper-cosmic-settings
    sudo rm -f /usr/share/applications/com.system76.CosmicAppletJasper.desktop
    sudo rm -f /usr/share/applications/com.system76.CosmicJasperSettings.desktop
```

### 5.2 Dependency Checks

Update `ubuntu/install-deps.sh` to optionally install COSMIC development libraries:

```bash
# Optional: COSMIC desktop development
if [ "$INSTALL_COSMIC_DEPS" = "1" ]; then
    sudo apt install -y \
        libwayland-dev \
        libxkbcommon-dev \
        libvulkan-dev \
        cmake
fi
```

### 5.3 Deliverables

- [ ] `make build-cosmic-applet` works on Pop!_OS / Ubuntu with COSMIC
- [ ] `make install-cosmic` installs both applet and settings app
- [ ] `make uninstall-cosmic` cleans up
- [ ] Dependency script handles optional COSMIC libs

---

## Phase 6: Documentation & Desktop Detection

### 6.1 Documentation Updates

- **`docs/COSMIC_DEVELOPMENT.md`**: Development workflow for the COSMIC applet (analogous to `EXTENSION_DEVELOPMENT.md` for GNOME)
- **`CLAUDE.md`**: Add COSMIC section with build/test commands
- **`README.md`**: Add COSMIC to supported desktop environments
- **`docs/DESKTOP_DETECTION_DESIGN.md`**: Update with COSMIC detection logic
- **`cosmic-applet/README.md`**: Applet-specific setup instructions

### 6.2 Desktop Detection Updates

Update the daemon's desktop detection (if it exists as runtime logic) to recognize COSMIC:

```rust
// Detection via environment
if env::var("XDG_CURRENT_DESKTOP").map(|d| d.contains("COSMIC")).unwrap_or(false) {
    detected.push(Frontend::Cosmic);
}

// Detection via process
if is_process_running("cosmic-panel") || is_process_running("cosmic-comp") {
    detected.push(Frontend::Cosmic);
}
```

### 6.3 Deliverables

- [ ] COSMIC development docs written
- [ ] CLAUDE.md updated with COSMIC workflow
- [ ] README reflects COSMIC support
- [ ] Detection scripts and code recognize COSMIC

---

## Implementation Order & Dependencies

```
Phase 1 ──► Phase 2 ──► Phase 3
  │                        │
  └──► Phase 4 ◄───────────┘
         │
         └──► Phase 5 ──► Phase 6
```

| Phase | Depends On | Can Parallelize With |
|-------|-----------|---------------------|
| 1 (Applet MVP) | Nothing | -- |
| 2 (Applet Polish) | Phase 1 | -- |
| 3 (Settings App) | Phase 1 (for launch button) | Phase 2 |
| 4 (NixOS) | Phase 1 minimum, ideally Phase 3 | Phase 5 |
| 5 (Ubuntu/Make) | Phase 1 minimum | Phase 4 |
| 6 (Docs) | All phases | Can start early, finish last |

---

## Key Technical Decisions

### 1. libcosmic Dependency Pinning

libcosmic is not on crates.io. We depend on the git repo:

```toml
libcosmic = { git = "https://github.com/pop-os/libcosmic", ... }
```

**Strategy**: Pin to a specific commit hash (not `master`) for reproducible builds. Update periodically. The `Cargo.lock` will pin the exact commit regardless, but an explicit `rev = "..."` in `Cargo.toml` makes the intent clear and prevents accidental updates.

### 2. Separate Binaries vs Monorepo Binary

The applet and settings app are **separate binaries** (`jasper-cosmic-applet` and `jasper-cosmic-settings`) in separate crates. This is intentional:
- The COSMIC panel launches the applet binary directly
- The settings app should be launchable independently
- Different `libcosmic` feature sets (applet vs full app)
- Failure isolation (settings crash doesn't kill the panel applet)

### 3. No Shared D-Bus Types Crate (Yet)

The `#[proxy]` macro in `dbus_client.rs` duplicates the interface definition from the daemon. This is acceptable because:
- The interface is small (7 methods, 2 signals)
- A shared crate would couple daemon and applet build graphs
- The D-Bus interface is stable and changes infrequently

If the interface grows significantly, extract a `jasper-dbus-types` crate.

### 4. Config Isolation

The applet does NOT read `config.toml`. It only communicates with the daemon via D-Bus. The settings app reads/writes `config.toml` directly. This maintains clean separation:
- Applet config (cosmic-config): presentation only
- Daemon config (TOML): behavior only
- Settings app: bridge between the user and daemon config

### 5. Emoji Rendering in Panel

COSMIC's panel renders text via iced's text widget. Emoji support depends on the system's font configuration (Noto Color Emoji or similar). If emoji rendering is problematic, fall back to a named icon from the icon theme. The applet should handle both cases.

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| libcosmic API churn | Medium | Medium | Pin to specific commit, review changelogs before updating |
| Emoji rendering issues in panel | Low | Low | Fallback to icon theme icons |
| COSMIC panel not discovering applet | Medium | High | Test `.desktop` file placement, check `X-CosmicApplet=true` |
| Build complexity (libcosmic deps) | Medium | Medium | Document required system packages, test in CI |
| NixOS COSMIC module not yet stable | Medium | Medium | Use `or false` guards in detection, test on actual COSMIC NixOS setup |
| D-Bus interface version mismatch | Low | High | Applet and daemon from same repo, versioned together |

---

## Testing Strategy

### Unit Tests
- Config serialization/deserialization roundtrip
- D-Bus proxy message formatting
- Panel text truncation logic

### Integration Tests
- Applet launches without daemon (should show offline state)
- Applet receives insight via D-Bus polling
- Applet receives insight via D-Bus signal
- Config changes persist and reload
- Settings app reads and writes config.toml correctly

### Manual Testing Checklist
- [ ] Applet appears in COSMIC panel after installation
- [ ] Click opens popup with insight text
- [ ] Refresh button works
- [ ] Emoji updates when new insight arrives
- [ ] Applet shows offline state when daemon is stopped
- [ ] Toggle settings persist across applet restart
- [ ] Settings app launches from popup
- [ ] Settings app changes are picked up by daemon
- [ ] Multiple monitor configurations work
- [ ] Panel position (top/bottom/left/right) works correctly
