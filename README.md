# Claude Usage Applet for COSMIC

A native [COSMIC](https://system76.com/cosmic/) panel applet that displays your [Claude Code](https://docs.anthropic.com/en/docs/claude-code) Max plan usage directly in the desktop panel.

The applet shows your current 5-hour utilization as a color-coded percentage badge in the panel, and expands into a popup with detailed usage breakdowns and progress bars for all tracked limits.

## Features

- **Panel badge** — shows 5-hour usage as a percentage with a green / amber / red background
- **Popup details** — 5-hour window, 7-day total, 7-day Sonnet, and 7-day Opus usage with progress bars and reset countdowns
- **Auto-refresh** — polls the Anthropic usage API every 5 minutes
- **Manual refresh** — click the ↻ Refresh button in the popup
- **Token management** — automatically refreshes expired OAuth tokens using `~/.claude/.credentials.json`

## Prerequisites

- [COSMIC desktop environment](https://system76.com/cosmic/)
- [Rust toolchain](https://rustup.rs/) (stable)
- An active **Claude Code Max** subscription with credentials stored at `~/.claude/.credentials.json` (created automatically when you log in via [Claude Code](https://docs.anthropic.com/en/docs/claude-code))

## Installation

```bash
git clone https://github.com/mherczeg/claude-usage-indicator-cosmic.git
cd claude-usage-indicator-cosmic
./install.sh
```

The install script builds a release binary and copies it along with the desktop entry to the system directories (requires `sudo`).

After installation, add the applet to your panel via **COSMIC Settings → Desktop → Panel**.

## Uninstallation

```bash
./uninstall.sh
```

## CLI Usage Checker

A standalone Python script is included for quick terminal-based usage checks:

```bash
./claude-usage-fetch
```

This prints a formatted table of all usage limits with progress bars and reset times. No additional Python dependencies are required — it uses only the standard library.

## Project Structure

```
src/
  main.rs     — applet entry point
  api.rs      — OAuth token refresh and usage API client
  window.rs   — COSMIC applet UI (panel badge + popup)
data/         — desktop entry file
icons/        — SVG icons (green, yellow, red)
claude-usage-fetch  — standalone Python CLI script
install.sh    — build & install script
uninstall.sh  — removal script
```

## License

This project is provided as-is without a formal license. Feel free to use and modify it for personal use.
