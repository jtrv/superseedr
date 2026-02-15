# TUI Architecture (Current Baseline)

This document describes the current `src/tui` architecture before the screen-based refactor.

## Module Layout
- `src/tui/view.rs`: rendering entrypoint and most screen rendering logic.
- `src/tui/events.rs`: input/event handling and most mode transitions.
- `src/tui/layout.rs`: responsive layout planning helpers.
- `src/tui/tree.rs`: tree data math/navigation utilities for file browser and preview.
- `src/tui/formatters.rs`: text/style formatting helpers used by the renderer.

## Runtime Flow
1. App loop receives async events in `App::run` (`src/app.rs`).
2. Input events are routed to `events::handle_event(event, &mut app)`.
3. Draw loop calls `tui::view::draw(f, &mut app_state, &settings)`.
4. `ui_needs_redraw` is used as a dirty flag (always drawn outside power-saving mode).

## Coupling Notes (Current)
- TUI directly depends on `crate::app::*` domain and mode types.
- `AppState` currently mixes domain and UI concerns.
- File browser UI state currently lives inside `AppMode::FileBrowser`/`FileBrowserMode`.

## Current Transition Summary
- `Welcome`: `Esc` -> `Normal`.
- `Normal`:
  - `/` enters search.
  - `z` -> `PowerSaving`.
  - `c` -> `Config`.
  - `a` opens file browser for adding torrents.
  - `d`/`D` -> `DeleteConfirm`.
  - `Q` sets quit flag.
  - `Esc` clears `system_error` (does not leave `Normal`).
- `PowerSaving`: `z` -> `Normal`.
- `Config`:
  - `Esc`/`Q` applies edited settings and returns to `Normal`.
  - `Enter` either edits field or opens file browser for path selection.
- `FileBrowser`:
  - `Y` confirms current action (context-specific) and usually returns.
  - `Esc` returns to `Normal` or `Config` depending on browser mode.
  - `/` enters browser search.
- `DeleteConfirm`: `Enter` confirms and returns to `Normal`; `Esc` cancels to `Normal`.

## Help Overlay Behavior
- `show_help` is currently a global overlay flag, not a separate `AppMode`.
- Windows:
  - `m` press toggles help.
- Non-Windows:
  - `m` press opens help.
  - `m` release closes help.
  - `Esc` closes help.
