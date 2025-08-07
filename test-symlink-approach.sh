#!/usr/bin/env bash

# Test the NixOS symlink approach manually

EXTENSION_UUID="jasper-dev-v1@tom.local"
NIXOS_EXT_DIR="/run/current-system/sw/share/gnome-shell/extensions"
EXTENSION_PATH="/nix/store/d4yg4hzy4nyzdlwjjfq81a4nhwiigkj5-jasper-companion-gnome-extension-dev-0.2.0-dev-dirty/share/gnome-shell/extensions/jasper-dev-v1@tom.local"

echo "Testing NixOS extension symlink approach..."
echo "Extension path: $EXTENSION_PATH"
echo "NixOS directory: $NIXOS_EXT_DIR"

echo "1. Current NixOS extensions:"
ls -la $NIXOS_EXT_DIR/ | grep -v "^total"

echo -e "\n2. Creating symlink (requires sudo)..."
if sudo ln -sfn "$EXTENSION_PATH" "$NIXOS_EXT_DIR/$EXTENSION_UUID"; then
    echo "✓ Symlink created successfully"
else
    echo "✗ Failed to create symlink"
    exit 1
fi

echo -e "\n3. Verifying symlink:"
ls -la "$NIXOS_EXT_DIR/$EXTENSION_UUID"

echo -e "\n4. Checking if GNOME detects the extension:"
sleep 2
gnome-extensions list | grep jasper || echo "Extension not yet detected"

echo -e "\n5. Trying to enable extension:"
if gnome-extensions enable "$EXTENSION_UUID"; then
    echo "✓ Extension enabled successfully"
else
    echo "✗ Failed to enable extension"
fi

echo -e "\n6. Extension status:"
gnome-extensions info "$EXTENSION_UUID" || echo "Extension info not available"

echo -e "\n7. Checking for log file (should appear if extension runs):"
sleep 5
if [ -f ~/.jasper-extension-dev.log ]; then
    echo "✓ Log file found!"
    tail -5 ~/.jasper-extension-dev.log
else
    echo "✗ No log file found - extension may not be running"
fi

echo -e "\nTest complete. Check your GNOME Shell panel for extension."