#!/usr/bin/env bash
set -euo pipefail

echo "── Claude Usage Applet Uninstaller ──"

sudo rm -f /usr/bin/claude-usage-applet
sudo rm -f /usr/share/applications/com.github.mherczeg.claude-usage-applet.desktop

echo "✓ Applet removed. You may need to restart the COSMIC panel."
