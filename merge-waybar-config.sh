#!/usr/bin/env bash
# Dynamic Waybar Configuration Merger for Jasper Development Mode
# Merges live waybar configuration with Jasper module definition

set -e

# Check for required tools
if ! command -v jq > /dev/null; then
    echo "Error: jq is required but not installed" >&2
    exit 1
fi

if [ $# -ne 3 ]; then
    echo "Usage: $0 <live_config> <jasper_template> <output>" >&2
    echo ""
    echo "Example:"
    echo "  $0 ~/.config/waybar/config.backup waybar/config.json waybar/merged-config.json" >&2
    exit 1
fi

LIVE_CONFIG="$1"
JASPER_TEMPLATE="$2"
OUTPUT="$3"

# Check input files exist
if [ ! -f "$LIVE_CONFIG" ]; then
    echo "Error: Live config file does not exist: $LIVE_CONFIG" >&2
    exit 1
fi

if [ ! -f "$JASPER_TEMPLATE" ]; then
    echo "Error: Jasper template file does not exist: $JASPER_TEMPLATE" >&2
    exit 1
fi

# Extract Jasper module from template
JASPER_MODULE=$(jq '.[0]["custom/jasper"]' "$JASPER_TEMPLATE")
if [ "$JASPER_MODULE" = "null" ]; then
    echo "Error: No custom/jasper module found in template" >&2
    exit 1
fi

# Merge configuration 
jq --argjson jasper_module "$JASPER_MODULE" '
  .[0] as $live |
  
  # Add/update Jasper module
  $live | .["custom/jasper"] = $jasper_module |
  
  # Ensure Jasper is in modules-center if not already present in any modules list
  if (
    (.["modules-left"] // [] | index("custom/jasper")) or
    (.["modules-center"] // [] | index("custom/jasper")) or  
    (.["modules-right"] // [] | index("custom/jasper"))
  ) then
    .  # Already in modules, do nothing
  else
    # Add to modules-center
    .["modules-center"] = ((.["modules-center"] // []) + ["custom/jasper"])
  end |
  
  # Wrap in array 
  [.]
' "$LIVE_CONFIG" > "$OUTPUT"

if [ $? -eq 0 ]; then
    echo "✅ Successfully merged waybar configuration"
    echo "   Live config: $LIVE_CONFIG"
    echo "   Jasper template: $JASPER_TEMPLATE"
    echo "   Output: $OUTPUT"
else
    echo "Error: Failed to merge configurations" >&2
    exit 1
fi