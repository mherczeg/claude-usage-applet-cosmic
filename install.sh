#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="/usr/bin"
DESKTOP_DIR="/usr/share/applications"

echo "── Claude Usage Applet Installer ──"

# Ensure cargo is in PATH
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

# Build release binary
echo "Building release binary…"
cd "$SCRIPT_DIR"
cargo build --release

# Install binary and desktop entry
echo "Installing (requires sudo)…"
sudo cp target/release/claude-usage-applet "$BIN_DIR/"
sudo cp data/com.github.mherczeg.claude-usage-applet.desktop "$DESKTOP_DIR/"

echo "✓ Applet installed."
echo "  Add it to your panel via COSMIC Settings → Desktop → Panel."
