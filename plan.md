# Claude Code Max Usage Panel Applet for COSMIC (Pop!_OS 24)

## Problem
Show Claude Code Max plan usage (rate limits) as a panel applet on the COSMIC desktop panel in Pop!_OS 24.

## Environment Summary
- **Desktop**: COSMIC (Wayland) — Pop!_OS 24.04
- **Panel system**: `cosmic-panel` with `CosmicAppletStatusArea` (supports StatusNotifierItem/DBus tray protocol)
- **Claude CLI**: v2.1.63 installed, OAuth-authenticated (Max 5x plan)
- **Usage API**: `GET https://api.anthropic.com/api/oauth/usage` — returns utilization percentages for 5-hour and 7-day windows with reset times
- **Auth**: OAuth Bearer token from `~/.claude/.credentials.json`, with refresh via `https://platform.claude.com/v1/oauth/token` (client_id: `9d1c250a-e61b-44d9-88ed-5944d1962f5e`)
- **Available Python libs**: GTK3, dbus-python, libnotify; AppIndicator GIR package needs installing (`gir1.2-ayatanaappindicator3-0.1`)

## Approach
Build a **Python system tray applet** using AyatanaAppIndicator3 (GTK3) that:
1. Reads the OAuth token from `~/.claude/.credentials.json`
2. Auto-refreshes the token when expired using the refresh token flow
3. Polls the usage API every 5 minutes
4. Shows a tray icon with the primary (5-hour) usage percentage as label text
5. On click, shows a dropdown menu with all rate limits, reset times, and a refresh option
6. Uses color-coded icon states (green/yellow/red) based on usage level
7. Sends desktop notifications when usage exceeds 80%
8. Installs as a systemd user service for autostart

## API Response Shape
```json
{
  "five_hour": { "utilization": 34.0, "resets_at": "2026-03-01T18:00:00Z" },
  "seven_day": { "utilization": 11.0, "resets_at": "2026-03-07T07:00:00Z" },
  "seven_day_sonnet": { "utilization": 4.0, "resets_at": "2026-03-07T08:00:00Z" },
  "seven_day_opus": null,
  "extra_usage": { "is_enabled": false, ... }
}
```

## Todos

1. **install-deps** — Install `gir1.2-ayatanaappindicator3-0.1` for Python AppIndicator support
2. **create-icons** — Generate SVG tray icons (green/yellow/red gauge states) in `~/.local/share/claude-usage-applet/icons/`
3. **create-applet-script** — Write the main Python script at `~/.local/bin/claude-usage-applet` that:
   - Reads and refreshes OAuth tokens
   - Polls the usage API on a timer
   - Renders tray icon with usage % label
   - Shows menu with all limits and reset countdowns
   - Sends notifications at high usage
4. **create-systemd-service** — Create `~/.config/systemd/user/claude-usage-applet.service` for autostart
5. **test-and-verify** — Start the service, verify it shows in the panel, confirm API data displays correctly

## Notes
- COSMIC's StatusArea applet acts as the system tray and supports the StatusNotifierItem DBus protocol, which AyatanaAppIndicator uses under the hood.
- Token expires roughly every hour; the refresh token is long-lived. The applet must handle refresh seamlessly.
- The 5-hour window is the most actionable limit for daily use, so it gets top billing in the icon label.
