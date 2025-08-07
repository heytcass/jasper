#!/usr/bin/env bash

# Manual test script to understand extension installation issues

set -e

echo "=== Manual Extension Installation Test ==="

EXTENSION_UUID="jasper-dev-v1@tom.local"

echo "1. Building extension..."
nix build .#gnome-extension-dev -o result-gnome-dev

echo "2. Checking what was built..."
find result-gnome-dev -name "*.json" -exec cat {} \;

echo "3. Current extensions in user directory:"
ls -la ~/.local/share/gnome-shell/extensions/ 2>/dev/null || echo "No user extensions directory"

echo "4. Current extensions known to GNOME:"
gnome-extensions list || echo "Failed to list extensions"

echo "5. Installing to user directory manually..."
mkdir -p ~/.local/share/gnome-shell/extensions
rm -rf ~/.local/share/gnome-shell/extensions/$EXTENSION_UUID
cp -r result-gnome-dev/share/gnome-shell/extensions/$EXTENSION_UUID ~/.local/share/gnome-shell/extensions/
chmod -R u+w ~/.local/share/gnome-shell/extensions/$EXTENSION_UUID

echo "6. Checking file permissions:"
ls -la ~/.local/share/gnome-shell/extensions/$EXTENSION_UUID

echo "7. Trying to refresh GNOME Shell extension list:"
killall -SIGUSR1 gnome-shell 2>/dev/null || echo "Could not signal GNOME Shell"
sleep 2

echo "8. Checking if GNOME sees the extension now:"
gnome-extensions list | grep jasper || echo "Extension not detected by GNOME Shell"

echo "9. Trying to get extension info:"
gnome-extensions info $EXTENSION_UUID || echo "Extension info not available"

echo "10. Checking system extensions directory:"
ls -la /usr/share/gnome-shell/extensions/ | grep jasper || echo "No jasper extensions in system directory"

echo "11. Current GNOME Shell version:"
gnome-shell --version

echo "=== Manual test complete ==="