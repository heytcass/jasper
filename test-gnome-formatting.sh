#!/bin/bash

# Script to test the GNOME formatter via D-Bus
echo "Testing GNOME formatter via D-Bus API..."

# Make sure daemon is running 
./dev-mode.sh status > /dev/null 2>&1
if [ $? -ne 0 ]; then
    echo "❌ Development mode not active. Run './dev-mode.sh start' first."
    exit 1
fi

echo "📡 Testing D-Bus connection..."

# Test the list_frontends method
echo "🔍 Available frontends:"
busctl --user call org.personal.CompanionAI /org/personal/CompanionAI/Companion org.personal.CompanionAI.Companion1 ListFrontends 2>/dev/null | \
    python3 -c "
import sys
import json
try:
    line = sys.stdin.read().strip()
    # Extract the JSON part from D-Bus response
    json_start = line.find('[')
    if json_start != -1:
        json_data = line[json_start:]
        frontends = json.loads(json_data)
        for frontend_id, frontend_name in frontends:
            print(f'  • {frontend_id}: {frontend_name}')
    else:
        print('  Could not parse frontend list')
except Exception as e:
    print(f'  Error parsing response: {e}')
"

echo ""
echo "🧪 Testing GNOME formatter..."

# Test getting formatted insights for GNOME
busctl --user call org.personal.CompanionAI /org/personal/CompanionAI/Companion org.personal.CompanionAI.Companion1 GetFormattedInsights s "gnome" 2>/dev/null | \
    python3 -c "
import sys
import json
try:
    line = sys.stdin.read().strip()
    # Extract the JSON part from D-Bus response (skip the 's' type prefix)
    json_start = line.find('{')
    if json_start != -1:
        json_data = line[json_start:-1]  # Remove trailing quote
        gnome_data = json.loads(json_data)
        
        print('✅ GNOME formatter working!')
        print(f'   Panel text: {gnome_data.get(\"text\", \"N/A\")}')
        print(f'   Tooltip: {gnome_data.get(\"tooltip\", \"N/A\")}')
        print(f'   Style class: {gnome_data.get(\"style_class\", \"N/A\")}')
        print(f'   Visible: {gnome_data.get(\"visible\", \"N/A\")}')
        print(f'   Insights count: {len(gnome_data.get(\"insights\", []))}')
        
        # Pretty print the full JSON
        print('')
        print('📋 Full GNOME Shell JSON output:')
        print(json.dumps(gnome_data, indent=2))
    else:
        print('❌ Could not parse GNOME formatter response')
        print(f'Raw response: {line}')
except Exception as e:
    print(f'❌ Error testing GNOME formatter: {e}')
    print(f'Raw response: {line}')
"

echo ""
echo "🔄 Comparing with waybar format..."

# Also test waybar for comparison
busctl --user call org.personal.CompanionAI /org/personal/CompanionAI/Companion org.personal.CompanionAI.Companion1 GetFormattedInsights s "waybar" 2>/dev/null | \
    python3 -c "
import sys
import json
try:
    line = sys.stdin.read().strip() 
    json_start = line.find('{')
    if json_start != -1:
        json_data = line[json_start:-1]
        waybar_data = json.loads(json_data)
        
        print('📊 Waybar format (for comparison):')
        print(f'   Text: {waybar_data.get(\"text\", \"N/A\")}')
        print(f'   Tooltip: {waybar_data.get(\"tooltip\", \"N/A\")}')
        print(f'   Class: {waybar_data.get(\"class\", \"N/A\")}')
    else:
        print('Could not parse waybar response')
except Exception as e:
    print(f'Error comparing with waybar: {e}')
"

echo ""
echo "✅ GNOME formatter test complete!"