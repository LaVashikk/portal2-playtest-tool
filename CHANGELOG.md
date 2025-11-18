# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0-rc.2] - 2025-11-18

### Added
- **Survey Widgets:** Introduced new layout and input widgets: `Header`, `TextBlock`, `Separator`, and `Checkboxes`. (Closes #2)
- **UI Notifications:** Implemented in-game toast notifications for log messages (Info, Warn, Error).
- **Flexible Config Loading:** Survey configurations can now be loaded using flexible paths (e.g., `default` or `default.json`).

### Changed
- **Preserve Answer Order:** Replaced `BTreeMap` with `IndexMap` in submissions to maintain the original question order.
- **Error Handling:** Improved survey loading by replacing panics with proper `Result`-based error handling to prevent crashes.
- **Default Survey:** The default survey content has been overhauled to showcase all available widgets.

### Fixed
- **Survey Loading:** The `open_survey 1` command now correctly reloads the default survey after a custom one was used. (Closes #3)
- **UI Behavior:** Corrected the `ScrollArea` speed and its reset behavior within surveys.