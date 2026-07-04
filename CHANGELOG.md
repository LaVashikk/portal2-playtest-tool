# Changelog

All notable changes to this project will be documented in this file.

## [0.1.2] - 2026-07-04

### Fixed
- **Level Loading Freeze:** Fixed an issue where the game would completely freeze during level loading while waiting for the file uploader to release mutex locks.
- **Server File Cleanup:** Fixed an issue where the server would incorrectly remove priority files during cleanup.

## [0.1.0] - 2026-04-27

### Added
- **Video Recording & Capturing:** Implemented game video recording with GPU acceleration, MSAA surface resolving in D3D9 hook, and surface caching. Added `--no-hw-encoder` CLI flag for software encoding fallback.
- **Dynamic Resolution & Time-Based Capture:** Added dynamic aspect-ratio-correct resolution alignment (16px aligned for hardware encoders) and time-based frame capturing intervals for consistent video speed regardless of game framerate.
- **Seamless Recording Restarts:** Implemented automatic FFmpeg restarts if the game resolution changes mid-session.
- **Survey Media Attachments:** Implemented automatic demo recording, log saving, and video capture attachments for bug reports, with multi-threaded uploading (sending archives and video before survey JSON).
- **Enriched Bug Reports:** Bug reports now include raw timestamps, player positions, view angles, custom Discord embed colors, and post-hook commands.
- **Game Events Support:** Added support for listening to and handling in-game events.
- **Server File Manager & Bot Commands:** Added a central file manager for the HTTP server, and introduced `export` and `stat` commands for the Discord bot with unique channel keys and updated embed styling.
- **Overlay Focus Notification:** Added an in-game overlay notification message when input focus is captured. (Closes #5)
- **ConVar Reset:** Implemented a `reset()` method for `ConVar`.

### Changed
- **SDK Renaming & Refactoring:** Renamed `source-sdk` crate to `portal2-sdk` and prepared it for crates.io publication. Added structured types, `Entities` wrapper, `IVEngineServer`, and `IServerTools` interfaces.
- **Global Engine State:** Refactored `UiManager` to use a static `Engine` reference via `OnceLock`.
- **Documentation & Compatibility:** Added documentation for new widgets and clarified non-support for Portal 2: Community Edition (P2:CE).

### Fixed
- **Logger Directory:** Log files are now correctly created in the folder where the DLL resides instead of the current working directory.
- **UI Zoom Scaling:** Fixed broken zoom scaling behavior in `egui_backend`.
- **Server & Logging Cleanup:** Improved general error handling, logging output, and server cleanup routines.

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
