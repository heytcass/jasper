#!/usr/bin/env bash

# Waybar script wrapper for Jasper
# This script uses the D-Bus interface exclusively

TIMEOUT=30

# Try D-Bus interface (only method)
if output=$(timeout $TIMEOUT dbus-send --session --dest=org.personal.CompanionAI --print-reply --type=method_call /org/personal/CompanionAI/Companion org.personal.CompanionAI.Companion1.GetFormattedInsights string:"waybar" 2>/dev/null | grep -E '^\s*string\s*"' | sed 's/^\s*string\s*"\(.*\)"$/\1/' | sed 's/\\"/"/g'); then
    # Only output if we got valid JSON (starts with {)
    if [[ "$output" =~ ^\{.*\}$ ]]; then
        echo "$output"
        exit 0
    fi
fi

# D-Bus failed - daemon not running or no insights available
echo '{"text":"📅","tooltip":"Jasper daemon not available","alt":"loading","class":"minimal","percentage":null}'
exit 0