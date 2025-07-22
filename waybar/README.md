# Jasper Waybar Integration

This directory contains the Waybar configuration for displaying Jasper's AI-powered calendar insights in your status bar.

## Files

- `config.json` - Complete Waybar configuration with Jasper module
- `style.css` - CSS styling with Stylix theming support
- `README.md` - This integration guide

## What Jasper Displays

Jasper shows **intelligent calendar insights** in your status bar:
- **Scheduling conflicts** and overcommitted days
- **Travel preparation** alerts
- **Event preparation** reminders  
- **Pattern analysis** and optimization suggestions
- **Urgency-based prioritization** (1-10 scale)

## Setup

1. **Install Waybar** (if not already installed):
   ```bash
   # On NixOS, add to your configuration.nix or home.nix:
   programs.waybar.enable = true;
   ```

2. **Copy configuration files**:
   ```bash
   mkdir -p ~/.config/waybar
   cp waybar/config.json ~/.config/waybar/config
   cp waybar/style.css ~/.config/waybar/style.css
   ```

3. **Ensure Jasper daemon is built**:
   ```bash
   nix build
   # Or if in development:
   cargo build --release
   ```

4. **Test the Waybar module**:
   ```bash
   # Test JSON output
   jasper-companion-daemon waybar
   
   # Test simple text output
   jasper-companion-daemon waybar --simple
   ```

5. **Start Waybar**:
   ```bash
   waybar
   ```

## Features

### Visual Indicators
- **🚨 Critical (9-10)**: Red background with pulsing animation
- **⚠️ Warning (7-8)**: Yellow background
- **💡 Info (5-6)**: Blue background
- **📝 Low (3-4)**: Gray background
- **📅 Clear/Minimal (0-2)**: Green background

### Interactions
- **Click**: Manual refresh of insights (sends SIGRTMIN+8 to waybar)
- **Hover**: Shows detailed tooltip with up to 3 insights
- **Auto-refresh**: Updates every 15 minutes (900 seconds)
- **Smart caching**: Avoids unnecessary AI analysis calls

### Tooltip Content
The tooltip shows:
- Up to 3 most important insights
- Action items for each insight
- Urgency scores (1-10)
- Total number of insights if more than 3

## Customization

### Update Interval
Edit `config.json` and change the `interval` value (in seconds):
```json
"custom/jasper": {
    "interval": 900,  // 15 minutes (recommended)
    ...
}
```

**Note**: Shorter intervals (< 5 minutes) may hit Claude API rate limits.

### Stylix Integration
The CSS now uses Stylix color variables for consistent theming:
- `@base00` to `@base0F` - Base16 color palette
- `@font-family` - System font family
- `@font-size` - System font size

### Styling
Edit `style.css` to customize colors, fonts, and animations:
```css
#custom-jasper.critical {
    background-color: @base08;  /* Uses Stylix red */
    /* Your custom styles */
}
```

### Module Position
Edit `config.json` to change where Jasper appears:
```json
"modules-left": ["hyprland/workspaces"],
"modules-center": ["custom/jasper"],  // Center position
"modules-right": ["network", "memory", "cpu", "clock"],
```

## Troubleshooting

### No insights showing
1. Check if daemon is running: `jasper-companion-daemon status`
2. Test output manually: `jasper-companion-daemon waybar`
3. Check logs: `journalctl -u jasper-companion --follow`

### Styling issues
1. Ensure `style.css` is in `~/.config/waybar/`
2. Restart Waybar after CSS changes
3. Check browser developer tools if using waybar with GTK debugging

### API rate limiting
If you see "All clear" frequently, check if API rate limiting is active:
```bash
jasper-companion-daemon test-calendar
```

## Integration with Hyprland

For Hyprland users, add to your `hyprland.conf`:
```conf
# Start Waybar on launch
exec-once = waybar

# Reload Waybar
bind = SUPER, R, exec, pkill waybar && waybar
```

## NixOS Integration

Add to your `home.nix`:
```nix
programs.waybar = {
  enable = true;
  settings = {
    mainBar = builtins.fromJSON (builtins.readFile ./waybar/config.json);
  };
  style = builtins.readFile ./waybar/style.css;
};
```