# Jasper Daemon File Audit - Legacy vs New Architecture

## ✅ **KEEP - Essential for New Architecture**

### Core Infrastructure
- `errors.rs` - Error types (used by new architecture)
- `database.rs` - Database layer with new insights tables
- `config.rs` - Configuration loading (needed for API keys, etc)
- `http_utils.rs` - HTTP utilities for API calls  
- `api_manager.rs` - API management (needed for AI calls)
- `google_calendar.rs` - Calendar API integration
- `sops_integration.rs` - Secrets management

### New Architecture Core
- `new_main.rs` - New simplified entry point
- `new_daemon_core.rs` - Simplified daemon core
- `new_dbus_service.rs` - Clean D-Bus API
- `significance_engine.rs` - Smart change detection
- `waybar_adapter.rs` - Waybar D-Bus client

### Context Sources (Needed for Data)
- `context_sources/mod.rs` - Context source framework
- `context_sources/calendar.rs` - Calendar context 
- `context_sources/weather.rs` - Weather context
- `context_sources/obsidian.rs` - Notes context
- `context_sources/tasks.rs` - Task context

## ❌ **REMOVE - Legacy/Obsolete Code**

### Old Architecture Core
- `main.rs` - Old complex main with all the commands
- `daemon_core.rs` - Old complex daemon core
- `dbus_service.rs` - Old D-Bus service with formatting
- `correlation_engine.rs` - Complex correlation logic (replaced by significance_engine)

### Service Layer (All UI/Notification Code)
- `services/` - **ENTIRE DIRECTORY** - All service layer abstractions
  - `services/notification.rs` - Notifications (now in frontend)
  - `services/companion.rs` - Main service orchestration 
  - `services/calendar.rs` - Calendar service wrapper
  - `services/insight.rs` - Insight formatting (now in frontend)
  - `services/mod.rs` - Service module definitions

### Formatter Modules (All UI Code)
- `formatters/` - **ENTIRE DIRECTORY** - All output formatting
  - `formatters/waybar.rs` - Waybar JSON formatting
  - `formatters/gnome.rs` - GNOME formatting  
  - `formatters/terminal.rs` - Terminal formatting
  - `formatters/mod.rs` - Formatter module definitions
- `waybar_formatter.rs` - Legacy waybar formatter
- `frontend_framework.rs` - Old frontend abstraction
- `frontend_manager.rs` - Frontend management layer

### Command System (Replaced by Simple CLI)
- `commands/` - **ENTIRE DIRECTORY** - Old command pattern
  - `commands/auth.rs` - Auth commands
  - `commands/calendar.rs` - Calendar commands
  - `commands/daemon_ops.rs` - Daemon operation commands
  - `commands/waybar.rs` - Waybar command
  - `commands/mod.rs` - Command module definitions
- `command_dispatcher.rs` - Command dispatch system

### Support/Helper Code
- `calendar_sync.rs` - Calendar sync wrapper (functionality moved)
- `data_sanitizer.rs` - Data sanitization (may not be needed)
- `desktop_detection.rs` - Desktop environment detection
- `error_recovery.rs` - Error recovery logic
- `test_data.rs` - Test data generation
- `config_v2.rs` - Alternative config system

## 🤔 **EVALUATE - May Need**

### Configuration
- `config_v2.rs` - If we want to migrate to new config system

### Testing/Development  
- `test_data.rs` - If we need test data generation for development

### Data Processing
- `data_sanitizer.rs` - If we need privacy/sanitization features

## 📊 **Summary**

**Current:** ~40 files  
**After cleanup:** ~15 files (62% reduction)

**Files to remove:** ~25 files
- Entire `services/` directory (5 files)
- Entire `formatters/` directory (4 files) 
- Entire `commands/` directory (5 files)
- ~11 other legacy files

**Impact:** 
- ✅ Much cleaner codebase
- ✅ Faster compilation 
- ✅ Easier maintenance
- ✅ Clear separation of concerns
- ✅ No accidental usage of old patterns