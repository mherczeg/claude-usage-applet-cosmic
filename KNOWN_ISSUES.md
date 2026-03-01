# Known Issues

## ~~Tray icon not displaying custom SVG~~ (Resolved)

**Status**: Fixed — resolved by rewriting as a native COSMIC applet using libcosmic.

The old Python/GTK applet used AyatanaAppIndicator3, which communicates icon names over the StatusNotifierItem DBus protocol. COSMIC's tray could not resolve these icon names regardless of installation location or theme configuration. The native COSMIC applet integrates directly into the panel via libcosmic, eliminating the icon lookup issue entirely.
