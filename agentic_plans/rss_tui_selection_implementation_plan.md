# Superseedr TUI RSS Implementation Plan (Webapp Selection Parity)

## Goal
Add native RSS automation into Superseedr TUI while preserving the current file-based ingest contract (`.magnet`, `.torrent`, `.path`, `shutdown.cmd`) and matching the RSS webapp's selection workflow:
- Explore aggregated feed items.
- Use an item title to seed regex filtering.
- Manually trigger one-off download for a selected item.
- Visually distinguish potential matches and already-downloaded items.

## Current Behavior to Mirror (from `plugins/RSS/superseedr-rss`)
1. Multi-feed management with enable/disable per feed.
2. Regex filter list used for automation.
3. "Live Feed Explorer" list of aggregated items across enabled feeds.
4. Per-item actions:
- Use title as regex seed (escape title into literal regex).
- Copy link.
- Send item to client (manual one-off add).
5. Highlighting semantics:
- Items matching current search regex OR saved filters are highlighted.
- Non-matches are dimmed when any filter/search is active.
- Matching rows are sorted to the top.
- Downloaded rows are badged.
6. History list + periodic sync status (last sync, next sync).

## Superseedr Integration Constraints
- Keep all torrent ingestion through existing handlers in `src/integrations/watcher.rs` and add flows equivalent to CLI atomic write style in `src/integrations/cli.rs`.
- Keep status output model compatible with `src/integrations/status.rs` (`status_files/app_state.json`).
- Persist durable config in `settings.toml` via `src/config.rs::save_settings` and use the same backup behavior.
- Reuse existing mode/event/view architecture (`AppMode`, `tui/events.rs`, `tui/view.rs`, reducer/effect style from `tui/screens/normal.rs` and `tui/screens/config.rs`).

## Target TUI UX (Selection Experience)

### Screen model
Add a dedicated `AppMode::Rss` modal/screen (similar interaction scope as Config/FileBrowser), accessible from normal mode keybinding (proposed: `R`).

### Layout
Three panes:
1. Left: Feeds (URL + enabled toggle).
2. Middle: Filters (regex + enabled toggle).
3. Right (largest): Feed Explorer list (title, source, date, badges).

Footer includes context-sensitive keys.

### Explorer row states
- `Downloaded` badge if row guid/link/title exists in RSS history.
- `Match` style if row matches active search regex or any enabled saved filter.
- `Dim` style for non-match rows when filter/search is active.

### Key interactions to mimic webapp row actions
When focus is on explorer list:
- `f`: "Use as Filter" -> write escaped selected title into filter input buffer (not auto-save yet).
- `A`: add filter from current input buffer.
- `Enter`: one-off "Send to Client" for selected item.
- `y`: copy selected link to clipboard.
- `/`: start inline regex search buffer (same model as Normal screen search).
- `s`: toggle sort mode between chronological and "matches first".

### Pane navigation
- `Tab`/`Shift+Tab` cycle panes.
- `j/k` and arrows navigate rows.
- `x` toggles enabled state on selected feed/filter.
- `d` deletes selected feed/filter.
- `a` opens add-feed prompt.
- `Esc` exits RSS mode (persisting any committed changes).

## Data Model and Persistence

### Add to `Settings` (persistent)
Add `rss: RssSettings` under `Settings` in `src/config.rs`.

```rust
pub struct RssSettings {
    pub enabled: bool,
    pub poll_interval_secs: u64,
    pub max_preview_items: usize,
    pub feeds: Vec<RssFeed>,
    pub filters: Vec<RssFilter>,
    pub history: Vec<RssHistoryEntry>,
}

pub struct RssFeed {
    pub url: String,
    pub enabled: bool,
}

pub struct RssFilter {
    pub regex: String,
    pub enabled: bool,
}

pub struct RssHistoryEntry {
    pub guid: String,
    pub title: String,
    pub source: Option<String>,
    pub date_iso: String,
    pub added_via: RssAddedVia, // Auto|Manual
}
```

### Persistent requirements
Persist these values:
1. RSS feature enabled/disabled.
2. Poll interval.
3. Feed list + per-feed enabled state.
4. Filter list + per-filter enabled state.
5. Download/add history used for dedupe and "Downloaded" badge.
6. Last successful sync timestamp.
7. Optional: last sync errors per feed (for UI diagnostics).

### Non-persistent (session-only)
- Current pane focus.
- Current selected row indices.
- Search input draft.
- Temporary add-feed/add-filter text buffers.
- Cached preview rows (can be rebuilt).

### Migration
- `Settings` defaults must initialize `rss` safely.
- Old settings files without RSS fields must load cleanly with defaults (`#[serde(default)]`).
- Preserve existing backup/save behavior unchanged.

## Runtime Components

### RSS worker service
Add `rss_service` runtime task:
- Polls enabled feeds on interval.
- Parses items and aggregates/deduplicates.
- Applies enabled regex filters.
- For matches: writes to watch folder using atomic temp->rename pattern.
- Emits UI events to refresh RSS mode state.

### Feed parsing
- Add RSS parser crate (`rss`/`feed-rs`) and normalize fields (`title`, `link`, `guid`, `pubDate`, `source`).
- Dedup key preference: `guid`; fallback `link`; fallback `title+source`.

### Download write path
Implement a shared helper for manual and auto flows:
- If link is `magnet:` -> write `.magnet.tmp` then rename to `.magnet`.
- If link appears torrent URL -> download bytes then write `.torrent.tmp` then rename to `.torrent`.
- Reuse `watch_folder` override semantics identical to existing ingest path behavior.

## App/Event Architecture Changes

### App state
Add `rss_ui` to `UiState` for screen state and `rss_runtime` state for latest fetched rows and sync metadata.

### New commands/events
Add app commands:
- `OpenRssScreen`
- `RssSyncNow`
- `RssFeedsUpdated`
- `RssFiltersUpdated`
- `RssPreviewUpdated`
- `RssDownloadSelected`

Follow existing reducer/effect separation pattern used in `tui/screens/normal.rs` and `tui/screens/config.rs`.

### Keymap updates
- Add normal-mode keybinding (`R`) to open RSS mode.
- Add help screen entries in `tui/screens/help.rs`.
- Add footer hint for RSS open key in normal footer.

## Render Plan

### New screen module
Create `src/tui/screens/rss.rs` with:
- `draw(...)`
- `handle_event(...)`
- action/effect reducer pattern for testability.

### Visual parity targets
- Explorer dominates width.
- Match highlighting + dimming logic mirrors webapp.
- Downloaded badge displayed inline.
- Match count shown in explorer header (`N potential matches`).
- Sync status shown (`Last sync`, `Next sync`).

## Failure Handling
- Invalid regex: reject add/enable with non-blocking error message.
- Feed fetch failures: keep last good preview; annotate per-feed error status.
- Write/download failures: surface in `system_error` and append to RSS diagnostics log.
- Clipboard failure (`y`): soft error only.

## Test Plan

### Unit tests
1. Regex compile/validation behavior.
2. Match/dim/sort logic for explorer rows.
3. Dedupe key behavior across feed merges.
4. History cap + insertion order.
5. Reducer coverage for RSS key actions.

### Integration tests
1. Poll feed fixture -> matched item writes atomic `.magnet`/`.torrent`.
2. Manual row download writes once and marks history.
3. Restart persistence: feeds/filters/history survive reload from `settings.toml`.
4. Watch folder override works with RSS writes.

### UI parity checks
- "Use as Filter" from selected row pre-fills escaped title.
- "Downloaded" badge appears after manual add.
- Matches float to top when filter/search active.

## Implementation Phases
1. **Data + persistence scaffolding**: extend `Settings`, migrations/defaults, tests.
2. **Worker + ingest path**: RSS polling, dedupe, atomic write helper.
3. **TUI screen skeleton**: mode switch, draw panes, navigation.
4. **Selection parity interactions**: use-as-filter, manual add, copy link, match sort.
5. **Help/footer/status integration**: docs, key hints, sync metadata.
6. **Hardening**: failure paths, regression tests, history limits.

## Scope Control
Initial in-TUI RSS should ship without external web service. The sidecar plugin can still exist, but core TUI RSS should be self-contained, persisted in `settings.toml`, and operate solely through Superseedr's existing file-based ingest model.
