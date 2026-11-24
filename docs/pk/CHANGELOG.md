# SheprdPK changelog

Local fork versions; upstream release notes are generated separately.

## [2026.11.06] - 2026-11-24

### Changed

- SheprdPK lives in one umbrella repository now: this fork is its `vendor/` submodule and the local profiles sit beside it as `patch/`.

## [2026.11.05] - 2026-11-24

### Added

- The Details button moves the column on each click: right of the tabs, hidden, then left between the workspaces and the tabs. The choice is kept.

### Fixed

- Give the native status metrics the bar's full height, so quotas, resources and the keep-awake cup show.

## [2026.11.04] - 2026-11-24

### Fixed

- Restore the Moonframe status bar after native metrics become available at startup.

## [2026.11.03] - 2026-11-24

### Fixed

- Show the read-only check for official releases in the App Updates panel instead of About.

## [2026.11.02] - 2026-11-24

### Added

- Browse the changes, branch, and history of each Git checkout in a local parent folder.
- Open About or check for updates from the status bar version menu.

### Fixed

- Share the classic status bar's provider usage, system resources and steaming keep-awake cup with Moonframe.
- Pass Tab and Shift-Tab to the focused terminal rather than navigating the window.
