# Superseedr TUI RSS Implementation Plan (Webapp Selection Parity)

## Goal
Add native RSS automation into Superseedr TUI while preserving the current file-based ingest contract (`.magnet`, `.torrent`, `.path`, `shutdown.cmd`) and matching the RSS webapp's selection workflow:
- Explore aggregated feed items.
- Use an item title to seed regex filtering.
- Manually trigger one-off download for a selected item.
- Visually distinguish potential matches and already-downloaded items.
- Expose sync controls and download history in-screen.

## Parity Contract (MVP)
Webapp-visible behavior is the source of truth for MVP parity. Enhancements are deferred unless explicitly marked.

1. Match-priority sorting is automatic only when search/filter is active.
2. Preview explorer dedupes by normalized item title.
3. Filters are add/delete in MVP (no per-filter UI toggle).
4. RSS screen includes `Sync Now`, sync status, and a history table.

## Current Behavior to Mirror
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
- Matching rows are sorted to the top only when filter/search is active.
- Downloaded rows are badged.
6. History list + periodic sync status (last sync, next sync) + Sync Now control.

## Superseedr Integration Constraints
- Keep all torrent ingestion through existing handlers in `src/integrations/watcher.rs` and add flows equivalent to CLI atomic write style in `src/integrations/cli.rs`.
- Keep status output model compatible with `src/integrations/status.rs` (`status_files/app_state.json`).
- Persist durable config in `settings.toml` via `src/config.rs::save_settings` and use the same backup behavior.
- Reuse existing mode/event/view architecture (`AppMode`, `tui/events.rs`, `tui/view.rs`, reducer/effect style from `tui/screens/normal.rs` and `tui/screens/config.rs`).

## Target TUI UX (Selection Experience)

### Screen model
Add a dedicated `AppMode::Rss` screen, accessible from normal mode keybinding `r`.

### Layout
Four logical sections:
1. Feeds (URL + enabled toggle).
2. Filters (regex list; add/delete only in MVP).
3. Feed Explorer list (title, source, date, badges).
4. History table (added/downloaded items with timestamp + source + mode).

Footer includes context-sensitive keys and sync metadata.

### Explorer row states
- `Downloaded` badge if row is present in RSS history.
- `Match` style if row matches active search regex or any saved filter.
- `Dim` style for non-match rows when filter/search is active.

### Sorting behavior (parity)
- No manual sort toggle in MVP.
- If no search/filter is active: chronological order.
- If search/filter is active: matching rows float to top automatically.

### Key interactions
When focus is on explorer list:
- `f`: Use selected title as escaped regex seed in filter input buffer (not auto-save yet).
- `A`: add filter from current input buffer.
- `Enter`: one-off Send to Client for selected item (manual add bypasses filters).
- `y`: copy selected link to clipboard.
- `/`: start inline regex search buffer.

Global RSS controls:
- `S`: Sync Now.
- `Ctrl+V`: paste into Quick Add Link input.
- `v`: Windows fallback paste trigger for Quick Add Link input.

### Pane navigation
- `Tab`/`Shift+Tab` cycle panes.
- `j/k` and arrows navigate rows.
- `x` toggles enabled state on selected feed.
- `d` deletes selected feed/filter.
- `a` opens add-feed prompt.
- `Esc` exits RSS mode (persisting committed changes).

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
    pub last_sync_at: Option<String>,
    pub feed_errors: std::collections::HashMap<String, FeedSyncError>,
}

pub struct RssFeed {
    pub url: String,
    pub enabled: bool,
}

pub struct RssFilter {
    pub regex: String,
    // MVP parity: always active; no UI toggle.
    pub enabled: bool,
}

pub struct RssHistoryEntry {
    pub dedupe_key: String, // ingest-level canonical key
    pub guid: Option<String>,
    pub link: Option<String>,
    pub title: String,
    pub source: Option<String>,
    pub date_iso: String,
    pub added_via: RssAddedVia, // Auto|Manual
}

pub struct FeedSyncError {
    pub message: String,
    pub occurred_at_iso: String,
}
```

### Dedupe model (decision-locked)
Use two dedupe rules for different goals:
1. Explorer preview dedupe key (parity): normalized `title`.
2. Ingest/history dedupe key (safety): `guid`; fallback `link`; fallback `title+source`.

### Persistent requirements
Persist these values:
1. RSS feature enabled/disabled.
2. Poll interval.
3. Feed list + per-feed enabled state.
4. Filter list (all effectively enabled in MVP UI behavior).
5. Download/add history used for dedupe and Downloaded badge.
6. Last successful sync timestamp.
7. Last sync error per feed (for UI diagnostics).

### Retention
- Cap history at 1000 entries.
- Evict oldest first when inserting past cap.

### Non-persistent (session-only)
- Current pane focus.
- Current selected row indices.
- Search input draft.
- Temporary add-feed/add-filter text buffers.
- Cached preview rows.

### Migration
- `Settings` defaults must initialize `rss` safely.
- Old settings files without RSS fields must load cleanly with defaults (`#[serde(default)]`).
- Preserve existing backup/save behavior unchanged.

## Runtime Components

### RSS worker service
Add dedicated `rss_service` runtime task:
- Spawn once from app startup.
- Poll enabled feeds on interval.
- Parse and aggregate items.
- Build explorer preview using title-based dedupe.
- Apply regex filters.
- For auto matches: write to watch folder using atomic temp->rename pattern.
- Emit app commands/events to refresh RSS mode state.
- Stop via existing app shutdown signal.

### Feed parsing
- Use `feed-rs` and normalize fields (`title`, `link`, `guid`, `pubDate`, `source`).

### Download write path
Implement a shared helper for manual and auto flows:
- `magnet:` -> write `.magnet.tmp` then rename to `.magnet`.
- Torrent URL -> fetch bytes then write `.torrent.tmp` then rename to `.torrent`.
- Reuse `watch_folder` override semantics identical to existing ingest path behavior.

### Fetch safety (moderate defaults)
- Accept `http`/`https` torrent URLs only.
- Enforce connect/read timeout.
- Enforce max download size cap.

## App/Event Architecture Changes

### App state
- Add `rss_ui` to `UiState` for screen-local state.
- Add `rss_runtime` app state for latest fetched rows and sync metadata.

### New app commands/events
Add async/runtime commands:
- `RssSyncNow`
- `RssPreviewUpdated`
- `RssSyncStatusUpdated`
- `RssFeedErrorUpdated`
- `RssDownloadSelected`
- `RssConfigUpdated`

Keep mode transitions in TUI screen handlers, consistent with existing architecture.

### Keymap updates
- Add normal-mode keybinding (`r`) to open RSS mode.
- Add help screen entries in `tui/screens/help.rs`.
- Add footer hint for RSS key in normal footer.

## Render Plan

### New screen module
Create `src/tui/screens/rss.rs` with:
- `draw(...)`
- `handle_event(...)`
- reducer/effect helpers for testability.

### Visual parity targets
- Explorer dominates width.
- Match highlighting + dimming mirrors webapp.
- Downloaded badge displayed inline.
- Match count shown in explorer header (`N potential matches`).
- Sync status shown (`Last sync`, `Next sync`) and `Sync Now` action visible.
- History table visible in RSS screen.

## Failure Handling
- Invalid regex: reject add with non-blocking error message.
- Feed fetch failures: keep last good preview; annotate per-feed error status.
- Write/download failures: surface in `system_error` and append to RSS diagnostics.
- Clipboard failure (`y` or paste): soft error; allow manual input fallback.

## Test Plan

### Unit tests
1. Regex compile/validation behavior.
2. Automatic match-priority sorting only when filter/search active.
3. Match/dim logic for explorer rows.
4. Preview title-dedupe behavior.
5. Ingest dedupe key behavior (`guid -> link -> title+source`).
6. History cap + insertion/eviction order.
7. Reducer coverage for RSS key actions.

### Integration tests
1. Poll feed fixture -> matched item writes atomic `.magnet`/`.torrent`.
2. Manual row download writes once and marks history.
3. Manual add bypasses filters but still dedupes by history key.
4. Restart persistence: feeds/filters/history/sync metadata survive reload.
5. Watch folder override works with RSS writes.

### UI parity checks
- Use as Filter pre-fills escaped title.
- Downloaded badge appears after manual add.
- Matches float to top only when search/filter active.
- No manual sort toggle exists in MVP.
- Sync Now control works and updates sync status.
- History table renders and reflects persisted history.

## Implementation Phases
1. **Parity contract + data semantics**: lock behavior rules, add settings schema, dedupe definitions.
2. **Persistence + runtime wiring**: defaults/migration, dedicated `rss_service` lifecycle, command bus plumbing.
3. **Worker + ingest path**: polling, parsing, filter application, shared atomic writer, fetch policy.
4. **RSS screen MVP**: mode switch, feeds/filters/explorer/history UI, key interactions, Sync Now, quick link paste.
5. **Help/footer/status integration**: key hints, parity docs, sync metadata display.
6. **Hardening + regression**: failure paths, parity tests, retention limits.

## Scope Control
Initial in-TUI RSS ships without external web service. Sidecar plugin may coexist, but core TUI RSS is self-contained, persisted in `settings.toml`, and operates through Superseedr's existing file-based ingest model.
