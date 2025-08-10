#!/usr/bin/env bash

# Waybar script wrapper for Jasper
# This script uses the D-Bus interface exclusively

TIMEOUT=30

# Check if this is a refresh request (via command line argument or environment variable)
if [[ "$1" == "--refresh" ]] || [[ "$JASPER_REFRESH" == "1" ]]; then
    # Trigger AI analysis refresh first
    gdbus call --session --dest org.personal.CompanionAI --object-path /org/personal/CompanionAI/Companion --method org.personal.CompanionAI.Companion1.RequestRefresh >/dev/null 2>&1 || true
    sleep 2  # Give daemon time to analyze
fi

# Get formatted insights from daemon
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