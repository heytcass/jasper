#!/usr/bin/env bash
# Simple script to add Jasper module to live waybar configuration

set -e

if [ $# -ne 2 ]; then
    echo "Usage: $0 <live_config> <output>" >&2
    echo "Adds Jasper module to live waybar configuration" >&2
    exit 1
fi

LIVE_CONFIG="$1"
OUTPUT="$2"

if [ ! -f "$LIVE_CONFIG" ]; then
    echo "Error: Live config file does not exist: $LIVE_CONFIG" >&2
    exit 1
fi

# Extract the Jasper module config from our template
JASPER_EXEC="/home/tom/git/jasper/waybar-jasper.sh"
JASPER_INTERVAL=900

echo "Adding Jasper module to live waybar configuration..."

# Use sed to add the custom/jasper module
# This is a simple approach - add before the closing brace of the main config object
sed '/"tray": {/i\
    "custom/jasper": {\
      "format": "{}",\
      "tooltip": true,\
      "interval": 900,\
      "exec": "/home/tom/git/jasper/waybar-jasper.sh",\
      "return-type": "json",\
      "signal": 8,\
      "on-click": "notify-send '\''Jasper'\'' '\''Refreshing insights...'\'' && pkill -RTMIN+8 waybar"\
    },' "$LIVE_CONFIG" > "$OUTPUT.tmp"

# Add custom/jasper to modules-center if it's not already there
if ! grep -q '"custom/jasper"' "$OUTPUT.tmp"; then
    # Find modules-center and add jasper to it
    sed '/\"modules-center\":/,/\]/{
        s/\]/,\n      "custom\/jasper"\n    ]/
    }' "$OUTPUT.tmp" > "$OUTPUT.tmp2" && mv "$OUTPUT.tmp2" "$OUTPUT.tmp"
fi

mv "$OUTPUT.tmp" "$OUTPUT"

echo "✅ Successfully added Jasper module to waybar configuration"
echo "   Source: $LIVE_CONFIG" 
echo "   Output: $OUTPUT"