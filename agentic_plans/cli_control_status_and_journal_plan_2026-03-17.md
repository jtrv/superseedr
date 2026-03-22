# CLI Control Commands, Status Streaming, And Command Journal

## Summary
Add and validate the current CLI surface while keeping the watch-folder architecture as the primary online control path. The CLI now includes:

- `status`
- `pause`
- `resume`
- `remove`
- `purge`
- `priority`
- `files`
- `info`
- `torrents`
- `journal`

The plan also covers the local event journal for control activity and the optional `--json` output envelope shared by all CLI commands.

## Current Defaults
- Torrent selector:
  - info hash by default
  - unique payload-file-path reverse resolution for `purge` and `info`
- File-priority targeting:
  - file index
  - manifest-relative path
- Status behavior:
  - one-shot by default
  - `--follow` for streaming
  - `--stop` to disable streaming
- Output behavior:
  - human-readable by default
  - optional `--json` envelope on every CLI command
- Remove behavior:
  - removes the torrent from Superseedr
  - keeps payload files
- Purge behavior:
  - removes the torrent from Superseedr
  - deletes payload files only when the local file layout can be resolved safely

## Implementation Notes
- Online mutating commands still go through `.control` files and the watched command sink.
- Offline mutating commands edit settings directly when safe.
- Offline `purge` uses persisted metadata or a local `.torrent` source to build a delete plan.
- `files`, `info`, and `torrents` are read-only local queries over settings plus persisted metadata.
- `status` remains JSON-native; `--json` wraps it in the shared CLI envelope instead of changing the underlying status schema.

## Output Contract
All commands support optional `--json`.

Success shape:

```json
{
  "ok": true,
  "command": "info",
  "data": {}
}
```

Failure shape:

```json
{
  "ok": false,
  "command": "info",
  "error": "..."
}
```

Read commands should keep stable field types. In particular:

- `files` is always an array
- `info.torrent.files` is always an array
- `torrents[].files` is always an array
- missing manifest/path data should be reported through sibling error fields, not by changing the type of `files`

## Public CLI Surface
- `superseedr status`
- `superseedr status --follow`
- `superseedr status --stop`
- `superseedr torrents`
- `superseedr info <info-hash-or-path>`
- `superseedr files <info-hash>`
- `superseedr pause <info-hash>`
- `superseedr resume <info-hash>`
- `superseedr remove <info-hash>`
- `superseedr purge <info-hash-or-path>`
- `superseedr priority <info-hash> (--file-index <n> | --file-path <relative-path>) <normal|high|skip>`
- `superseedr journal`
- optional `--json` on all of the above

## Internal Control Actions
The watch-folder `.control` path continues to carry:

- `status_now`
- `status_follow_start`
- `status_follow_stop`
- `pause`
- `resume`
- `delete`
- `set_file_priority`

Notes:

- The user-facing split is `remove` vs `purge`.
- The internal control action remains `delete` with `delete_files = false|true`.

## Event Journal Expectations
- `EventCategory::Control`
- `ControlQueued`
- `ControlApplied`
- `ControlFailed`

Control journal entries should record:

- action name
- target info hash
- optional file-index or file-path targeting
- CLI origin:
  - `CliOnline`
  - `CliOffline`

## Test Plan

### CLI Parsing
- All current subcommands parse correctly.
- `priority` requires exactly one of `--file-index` or `--file-path`.
- `purge` requires at least one target.
- `info`, `files`, and `torrents` remain read-only and do not map to control requests.
- Global `--json` parses before or after subcommands.

### Offline Mutations
- offline `pause` / `resume` mutate and save settings correctly
- offline `priority` updates the correct file priority
- offline `remove` removes the torrent from settings
- offline `purge` deletes payload files and removes the torrent when paths are known
- offline `purge` fails cleanly when manifest/path data is unavailable
- offline failures still honor `--json`

### Offline Read Commands
- offline `status` returns an offline JSON snapshot
- offline `files` returns manifest-relative paths and resolved full paths when available
- offline `info` resolves by info hash
- offline `info` resolves by unique payload file path
- offline `torrents` lists every configured torrent with nested file manifests

### Online Control Path
- `.control` files parse into the correct app command
- malformed control files are rejected safely
- online `pause` / `resume` update persisted state and send manager commands
- online `remove` removes the torrent without deleting payload files
- online `purge` queues delete-with-files
- online `priority` updates persisted state and sends `SetUserTorrentConfig`

### Status
- `status_now` triggers an immediate JSON dump
- `status --follow` enables temporary runtime dumps
- `status --stop` disables them without restart
- `status --json` wraps the status payload in the shared CLI envelope

### Structured Output
- every command supports `--json`
- `--json` successes use the common `{ ok, command, data }` shape
- `--json` failures use the common `{ ok: false, command, error }` shape
- failures before command dispatch, including settings-load failures, still honor `--json`
- `files` remains an array field in `files`, `info`, and `torrents`

### Event Journal
- online control commands record `ControlQueued` then `ControlApplied`
- failed online control commands record `ControlFailed`
- offline direct settings mutations record control journal entries without a queued phase
- status actions are journaled

## Assumptions
- The watch-folder control architecture remains the primary online control mechanism.
- No socket or HTTP control plane is added in this phase.
- Info hash remains the primary selector.
- Payload-file-path reverse resolution is a convenience only for `purge` and `info`.
- Relative-path priority targeting uses manifest-relative paths, not absolute filesystem paths.
