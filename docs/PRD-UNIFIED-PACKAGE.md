# PRD: Unified Jasper Package with Auto-Desktop Detection

**Version**: 1.0  
**Date**: August 2025  
**Status**: Planning  
**Author**: Claude Code  

## Executive Summary

This PRD outlines the architectural evolution of Jasper from multiple separate packages to a single, intelligent package that automatically detects desktop environments and enables appropriate frontend components. This addresses current user experience friction while leveraging Jasper's sophisticated existing FrontendFramework infrastructure.

## Current State Analysis

### Existing Architecture Strengths ✅

**Sophisticated Frontend Framework**: Jasper already has excellent frontend abstraction:
```rust
// Well-designed trait system in daemon/src/frontend_framework.rs
pub trait FrontendFormatter<T> {
    fn format(&self, insights: &[InsightData], timezone: Tz) -> Result<T>;
    fn frontend_id(&self) -> &'static str;
    fn frontend_name(&self) -> &'static str;
}

pub struct FrontendRegistry {
    formatters: std::collections::HashMap<String, Box<dyn JsonFrontendFormatter>>,
}
```

**Multiple Frontend Support**: Already supports:
- **Waybar**: JSON output for status bars (`daemon/src/formatters/waybar.rs`)
- **GNOME**: Shell extension integration (`daemon/src/formatters/gnome.rs`) 
- **Terminal**: Debug/testing output (`daemon/src/formatters/terminal.rs`)

**NixOS Module**: Existing module with frontend awareness (`nix/module.nix:27-31`)

### Current Problems ❌

**Package Fragmentation**: Users must install multiple packages:
```nix
# Current required configuration
environment.systemPackages = [
  pkgs.jasper-companion            # Daemon
  pkgs.jasper-gnome-extension-dev  # GNOME extension
];
```

**Manual Frontend Management**: Users must:
- Know which frontends they need
- Manually configure multiple packages
- Understand implementation details

**Maintenance Overhead**: 
- Three separate packages to version and maintain
- Duplicate configuration in NixOS overlay
- Complex integration testing matrix

## Vision: Unified Intelligent Package

### Core Principle
**"Install once, works everywhere"** - Jasper should automatically detect the desktop environment and activate appropriate frontends without user intervention.

### Target User Experience
```bash
# Simple installation
services.jasperCompanion.enable = true;

# That's it! Jasper automatically:
# ✅ Detects GNOME → Installs extension + enables D-Bus service
# ✅ Detects Waybar → Provides JSON output 
# ✅ Detects Sway → Future: Enables sway-specific features
# ✅ Detects multiple DEs → Enables all applicable frontends
```

## Technical Requirements

### 1. Desktop Environment Detection

**Runtime Detection Variables**:
- `$XDG_CURRENT_DESKTOP` (gnome, KDE, sway, etc.)
- `$DESKTOP_SESSION` (gnome, plasma, sway)
- `$GNOME_SHELL_VERSION` (GNOME-specific)
- `$KDE_SESSION_VERSION` (KDE-specific)

**Process Detection**:
- `gnome-shell` process → GNOME active
- `waybar` process → Likely tiling WM setup
- `plasmashell` process → KDE active

**File System Detection**:
- `~/.config/waybar/` → Waybar configured
- GNOME Shell extensions directory → Extension support

### 2. Smart Frontend Registry

**Conditional Registration**:
```rust
// Enhanced FrontendRegistry with detection
impl FrontendRegistry {
    pub fn new_with_detection() -> Self {
        let mut registry = Self::new();
        
        // Only register available frontends
        if is_gnome_available() {
            registry.register(GnomeFrontendFormatter::new());
        }
        if is_waybar_available() {
            registry.register(WaybarFrontendFormatter::new());
        }
        // Future: KDE, Sway, etc.
        
        registry
    }
}
```

### 3. Unified Package Structure

**Single Package Contents**:
- **Daemon binary**: Core AI analysis engine
- **GNOME extension**: Conditionally installed if GNOME detected
- **D-Bus service files**: For desktop integration
- **Configuration templates**: For different desktop environments
- **Frontend components**: All formatters bundled

**Installation Logic**:
```nix
# Intelligent component selection
jasper-unified = pkgs.stdenv.mkDerivation {
  # Install daemon always
  # Install GNOME extension only if GNOME detected
  # Install Waybar configs only if Waybar detected
  # Future: KDE Plasma widgets, etc.
};
```

### 4. Enhanced NixOS Integration

**Simplified Service Definition**:
```nix
{
  options.services.jasperCompanion = {
    enable = mkEnableOption "Jasper Companion (unified)";
    
    # Auto-detection options
    autoDetectDesktop = mkOption {
      type = types.bool;
      default = true;
      description = "Automatically detect and enable appropriate frontends";
    };
    
    # Manual overrides (advanced users)
    forceEnableFrontends = mkOption {
      type = types.listOf types.str;
      default = [];
      description = "Manually specify frontends (gnome, waybar, kde)";
    };
  };
}
```

## Implementation Phases

### Phase 1: Detection Infrastructure
**Goal**: Build robust desktop environment and frontend detection

**Tasks**:
1. **Create detection utilities** (`daemon/src/desktop_detection.rs`)
   - Environment variable parsing
   - Process detection functions  
   - File system checking utilities
   - Desktop capability assessment

2. **Enhanced frontend registry**
   - Conditional formatter registration
   - Runtime availability checking
   - Graceful fallback handling

3. **Testing framework**
   - Mock different desktop environments
   - Validate detection accuracy
   - Performance benchmarks

### Phase 2: Package Unification
**Goal**: Merge daemon + extension into single intelligent package

**Tasks**:
1. **Update flake.nix**
   - Single `default` package with conditional components
   - Remove separate `gnome-extension` packages
   - Smart build-time component inclusion

2. **NixOS module enhancement**
   - Simplified service configuration
   - Auto-detection integration
   - Backward compatibility preservation

3. **Installation logic**
   - Conditional GNOME extension installation
   - Dynamic D-Bus service registration
   - Cross-desktop configuration management

### Phase 3: Smart Service Management
**Goal**: Runtime frontend detection and automatic activation

**Tasks**:
1. **Service startup detection**
   - Desktop environment assessment at daemon startup
   - Dynamic frontend activation
   - Configuration adaptation

2. **GNOME extension integration**
   - Automatic extension installation/activation
   - Version management
   - Runtime status monitoring

3. **Multi-frontend coordination**
   - Simultaneous frontend support (e.g., Waybar + GNOME)
   - Resource sharing and conflict resolution
   - Performance optimization

### Phase 4: User Experience Polish
**Goal**: Seamless cross-desktop experience

**Tasks**:
1. **Configuration simplification**
   - Unified configuration schema
   - Desktop-specific defaults
   - Intelligent preference detection

2. **Documentation update**
   - Single installation guide
   - Cross-desktop examples
   - Troubleshooting for mixed environments

3. **Quality assurance**
   - Cross-desktop testing matrix
   - User workflow validation
   - Performance regression prevention

## Success Criteria

### User Experience Metrics
- **Installation complexity**: From 3+ packages → 1 package
- **Configuration steps**: From manual frontend selection → automatic detection
- **Time to first notification**: < 2 minutes after installation
- **Cross-desktop compatibility**: Works on GNOME, KDE, Sway, Waybar setups

### Technical Metrics
- **Detection accuracy**: >95% correct desktop environment identification
- **Resource efficiency**: No performance regression vs. current implementation
- **Reliability**: Graceful handling of unsupported/mixed desktop environments
- **Maintainability**: Single version/release process for all components

## Risk Mitigation

### Desktop Detection Reliability
- **Risk**: False positive/negative desktop detection
- **Mitigation**: Multiple detection methods, user override options, extensive testing

### GNOME Extension Compatibility
- **Risk**: Automatic extension installation conflicts
- **Mitigation**: Version compatibility matrix, careful extension lifecycle management

### Package Complexity
- **Risk**: Single package becomes unwieldy
- **Mitigation**: Modular internal architecture, optional component system

## Developer Instructions

**CRITICAL**: When implementing this architecture, you **MUST** follow these development practices:

> Do deep research on the best way to tackle this in 2025. Think hard on this challenge, and act like a senior developer when solving this challenge. Document **along the way** when needed, and if you run into issues _during development_ that stumps you, pause and **perform comprehensive web searches** on how to **actually solve the issue** before proceeding. **DO NOT** just make guesses on how to solve hurdles, but look for real solutions first.

### Specific Research Areas Required

1. **Desktop Environment Detection Best Practices**
   - Research current standards for DE detection in 2025
   - Study how major Linux applications handle multi-desktop support
   - Investigate XDG specifications and emerging standards

2. **NixOS Conditional Package Installation**
   - Research advanced NixOS packaging patterns for conditional components
   - Study how other packages handle desktop-specific features
   - Investigate Nix's capability for runtime environment detection

3. **GNOME Extension Lifecycle Management**
   - Research automatic GNOME extension installation from system packages
   - Study GNOME Shell extension API changes and compatibility
   - Investigate version management for system-installed extensions

4. **Cross-Desktop D-Bus Integration**  
   - Research D-Bus service compatibility across different desktop environments
   - Study notification system differences between GNOME/KDE/Sway
   - Investigate freedesktop.org specifications for cross-desktop compatibility

## Related Context

This PRD builds upon the recently completed **Native D-Bus Notifications** implementation:

### Recently Completed (August 2025)
✅ **Native D-Bus notifications** via `notify-rust` crate  
✅ **Frontend-agnostic design** supporting GNOME 48, mako, dunst  
✅ **Enhanced notification features** (categories, urgency, desktop integration)  
✅ **Robust async implementation** with proper error handling  
✅ **Comprehensive testing and diagnostics**

### Current Package Status
- **Daemon**: Fully functional with D-Bus notifications
- **GNOME Extension**: Separate development package
- **Configuration**: Updated with new D-Bus options

This foundation provides the perfect launching point for package unification, as the core notification and frontend infrastructure is already sophisticated and well-architected.

---

*This PRD represents a strategic evolution of Jasper's architecture to provide a seamless, intelligent user experience while maintaining the sophisticated technical foundation already established.*