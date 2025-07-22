#!/usr/bin/env bash

# Waybar script wrapper for Jasper
# This script provides a timeout and fallback for the waybar integration

TIMEOUT=30
JASPER_LOCAL="/home/tom/git/jasper/target/debug/jasper-companion-daemon"
JASPER_SYSTEM="jasper-companion-daemon"

# Try local build first (has waybar command), then fall back to system binary
if [ -x "$JASPER_LOCAL" ]; then
    JASPER_CMD="$JASPER_LOCAL waybar"
else
    JASPER_CMD="$JASPER_SYSTEM waybar"
fi

# Try to run jasper with timeout, suppress ALL output except the final JSON line
if output=$(timeout $TIMEOUT $JASPER_CMD 2>/dev/null | tail -n 1); then
    # Only output if we got valid JSON (starts with {)
    if [[ "$output" =~ ^\{.*\}$ ]]; then
        echo "$output"
        exit 0
    fi
fi

# Fallback case - provide fallback JSON
echo '{"text":"📅","tooltip":"Jasper is starting up or analyzing your calendar","alt":"loading","class":"minimal","percentage":null}'
exit 0