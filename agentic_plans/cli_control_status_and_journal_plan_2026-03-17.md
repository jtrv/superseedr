# CLI Control Commands, Status Streaming, And Command Journal

## Summary
Add CLI support for `status`, `pause`, `resume`, `delete`, and `priority` while keeping the existing watch-folder architecture as the primary online control path. Fold in the current refactors needed for watch-path lifecycle and runtime status cadence, and extend the local event journal so CLI control requests are recorded alongside ingest/completion/health events.

Chosen defaults:
- Torrent selector: info hash only
- File-priority targeting: support both file index and relative path
- Status behavior: one-shot by default, with optional `--follow`
- Delete scope: remove from client/settings only; no file deletion in this phase
- Offline behavior: hybrid
  - when the app is running, use watch-folder control files
  - when the app is not running, directly edit settings for `pause`, `resume`, `priority`, and `delete`
  - `status` requires a running app

## Findings To Fix As Part Of The Work
- `output_status_interval` is captured once at startup, so runtime status enable/disable cannot work without refactoring the timer in `src/app.rs`.
- Watcher reconfiguration only updates `settings.watch_folder`, not the full `configured_watch_paths()` set, so stale watch paths can remain active after config changes.
- Only the legacy local watch/processed directories are created automatically; explicit `watch_folder` and `SUPERSEEDR_WATCH_PATH_*` paths are not bootstrapped.
- These fixes are part of the feature work because the new CLI/status flow depends on them.

## Implementation Changes
- Extend the CLI in `src/integrations/cli.rs`.
  - Add subcommands:
    - `status`
    - `pause <info-hash>`
    - `resume <info-hash>`
    - `delete <info-hash>`
    - `priority <info-hash> (--file-index <n> | --file-path <relative-path>) <normal|high|skip>`
  - `status`
    - default: request a fresh status dump and print raw JSON to stdout
    - `--follow`: enable temporary 5-second status dumps for the current runtime and stream updates
    - `--stop`: disable runtime status streaming
- Add a structured online control-file format.
  - Use a dedicated extension such as `.control`
  - Typed JSON or TOML payload with an explicit `action`
  - Supported actions:
    - `status_now`
    - `status_follow_start`
    - `status_follow_stop`
    - `pause`
    - `resume`
    - `delete`
    - `set_file_priority`
- Extend watcher parsing in `src/integrations/watcher.rs`.
  - Map `.control` files to a new `AppCommand` carrying a parsed control request
  - Keep processed-file cleanup consistent with other watched command files
- Handle control requests inside the app using existing runtime semantics.
  - `pause` / `resume`
    - mutate persisted `torrent_control_state`
    - update in-memory display state
    - send `ManagerCommand::Pause` / `Resume` when active
    - save state immediately
  - `delete`
    - remove the torrent from persisted settings/config only
    - when running, converge through the existing manager shutdown/removal path without deleting files
    - no `--with-files` behavior in this phase
  - `priority`
    - resolve torrent by info hash
    - resolve file target by index or manifest-relative path
    - update persisted `file_priorities`
    - update in-memory torrent state
    - send `ManagerCommand::SetUserTorrentConfig` when active
    - save state immediately
- Implement offline CLI behavior in `src/main.rs` / `src/integrations/cli.rs`.
  - Detect whether the app is running using the existing single-instance/lock-file model
  - If not running:
    - `pause`, `resume`, `priority`, and `delete` load settings, mutate the target torrent, and save settings directly
    - `status` returns a clear `requires running app` error
  - If running:
    - write a `.control` request to the command inbox and let the running app apply it
- Refactor status dumping in `src/app.rs`.
  - Replace the startup-captured `output_status_interval` with runtime-reschedulable state
  - `status_now` triggers an immediate dump even when periodic status is disabled
  - `status_follow_start` / `status_follow_stop` change cadence for the current runtime only
  - Do not persist runtime status cadence changes in v1
- Refactor watch-path lifecycle.
  - Introduce one helper that diffs full `configured_watch_paths()` before and after config changes and applies watcher `watch/unwatch` updates
  - Ensure all configured watch directories exist before watcher startup and before CLI writes

## Event Journal Changes
- Extend the local event journal to record CLI control activity.
- Add a new low-frequency journal category for control operations.
  - `EventCategory::Control`
- Add new event types:
  - `ControlQueued`
  - `ControlApplied`
  - `ControlFailed`
- Add a small typed detail shape for control actions.
  - action name: `status_now`, `status_follow_start`, `status_follow_stop`, `pause`, `resume`, `delete`, `set_file_priority`
  - target info hash
  - optional priority target info:
    - file index
    - relative path
    - requested priority
  - source origin:
    - `CliOnline`
    - `CliOffline`
- Journal behavior:
  - online CLI requests:
    - append `ControlQueued` when the `.control` file is discovered
    - append `ControlApplied` or `ControlFailed` when handled
    - use a stable correlation id derived from the control-file path
  - offline CLI requests:
    - append a local journal entry directly when the CLI mutates settings
    - record `ControlApplied` or `ControlFailed`
    - no queued phase for offline direct mutations
  - `status` operations should also journal:
    - `status_now`
    - `status_follow_start`
    - `status_follow_stop`
- Keep the journal local-only even in shared-config mode.

## Public Interfaces
- New CLI surface:
  - `superseedr status`
  - `superseedr status --follow`
  - `superseedr status --stop`
  - `superseedr pause <info-hash>`
  - `superseedr resume <info-hash>`
  - `superseedr delete <info-hash>`
  - `superseedr priority <info-hash> (--file-index <n> | --file-path <relative-path>) <normal|high|skip>`
- New internal watch-folder file type:
  - `*.control`
- New internal app command for parsed control requests
- Event journal additions:
  - `EventCategory::Control`
  - `ControlQueued`
  - `ControlApplied`
  - `ControlFailed`

## Test Plan
- CLI parsing tests
  - all new subcommands parse correctly
  - `priority` requires exactly one of `--file-index` or `--file-path`
- Offline behavior tests
  - offline `pause` / `resume` mutate and save settings correctly
  - offline `priority` updates the correct file priority
  - offline `delete` removes the torrent from settings
  - offline `status` fails cleanly
- Watcher/control-file tests
  - `.control` files parse into the correct app command
  - malformed control files are rejected safely
- Online control tests
  - `pause` / `resume` update persisted state and send manager commands
  - `delete` removes the torrent without deleting payload files
  - `priority` updates persisted state and sends `SetUserTorrentConfig`
- Status tests
  - `status_now` triggers an immediate JSON dump
  - `status --follow` enables temporary 5-second runtime dumps
  - `status --stop` disables them without restart
- Event journal tests
  - online control commands record `ControlQueued` then `ControlApplied`
  - failed online control commands record `ControlFailed`
  - offline direct settings mutations record control journal entries without a queued phase
  - status actions are journaled
- Refactor regression tests
  - runtime watch-path updates diff the full `configured_watch_paths()` set
  - configured watch directories are created before watcher use
  - `resolve_command_watch_path(settings)` remains included in watched paths

## Assumptions
- The existing watch-folder command architecture remains the primary online control mechanism; no socket or HTTP control plane is added.
- Info hash is the only supported torrent selector in v1.
- Relative-path priority targeting uses manifest-relative file paths, not absolute filesystem paths.
- `status` prints raw JSON in v1; no parsed/pretty CLI presentation is added.
- Delete in this phase means removal from Superseedr only; payload files are never deleted by the CLI.
