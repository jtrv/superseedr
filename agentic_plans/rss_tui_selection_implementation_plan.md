# Superseedr TUI RSS Implementation Plan (Webapp Selection Parity)

## Goal
Add native RSS automation into Superseedr TUI while preserving the current file-based ingest contract (`.magnet`, `.torrent`, `.path`, `shutdown.cmd`) and matching the RSS webapp's selection workflow:
- Explore aggregated feed items.
- Use an item title to seed regex filtering.
- Manually trigger one-off download for a selected item.
- Visually distinguish potential matches and already-downloaded items.
- Expose sync controls and history in-screen.

## Parity Contract (MVP)
Webapp-visible behavior is the source of truth for MVP parity. Enhancements are deferred unless explicitly marked.

1. Match-priority sorting is automatic only when search/filter is active.
2. Preview explorer dedupes by normalized item title.
3. Filters are add/delete in MVP (no per-filter UI toggle).
4. RSS mode includes `Sync Now`, sync status, and a dedicated history sub-screen.

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
- Persist durable RSS config in `settings.toml` via `src/config.rs::save_settings`.
- Persist RSS runtime state in `rss.toml` under a dedicated `persistence/` path.
- Reuse existing mode/event/view architecture (`AppMode`, `tui/events.rs`, `tui/view.rs`, reducer/effect style from `tui/screens/normal.rs` and `tui/screens/config.rs`).

## Target TUI UX (Selection Experience)

### Screen model
Keep one mode: `AppMode::Rss` (opened from normal mode key `r`), with four internal RSS sub-screens:
1. Feeds
2. Filters
3. Explorer
4. History

Default sub-screen on open: **Feeds**.

### Global RSS navigation
- `f`: switch to Feeds sub-screen.
- `l`: switch to Filters sub-screen.
- `e`: switch to Explorer sub-screen.
- `h`: switch to History sub-screen.
- `S`: Sync Now.
- `Esc` / `q`: exit RSS mode.

Rules:
- `Tab` / `Shift+Tab` are disabled in RSS mode.
- While typing in add/search/edit buffers, sub-screen switch keys are treated as text only if the input capture mode is active.

### Shared header/footer
All RSS sub-screens render a shared header/footer shell containing:
- Current sub-screen label.
- Last sync / next sync.
- Key hints for global RSS actions.

### Feeds sub-screen
- Feed list with enabled state.
- `j/k` + arrows to move.
- `x` toggles selected feed enabled state.
- `a` opens add-feed prompt.
- `d` deletes selected feed.

### Filters sub-screen
- Filter list + add/delete controls.
- `j/k` + arrows to move.
- `a` opens add-filter prompt.
- `d` deletes selected filter.
- Live draft behavior while typing filter text:
- Draft regex is compiled and applied as-you-type.
- Match count updates as-you-type.
- Inline live preview list updates as-you-type so user sees affected explorer items immediately.
- Invalid draft regex shows non-blocking inline error without losing input buffer.

### Explorer sub-screen
- Aggregated feed items list (`title`, `source`, `date`, badges).
- Row states:
- `Downloaded` badge if present in RSS history.
- `Match` style if item matches active search or saved filters.
- `Dim` style for non-matches when filter/search active.
- Sorting behavior:
- No manual sort toggle.
- Chronological when no search/filter active.
- Matches-first automatically when search/filter active.
- Actions:
- `Enter`: one-off Send to Client for selected item (manual add bypasses filters).
- `f`: use selected title as escaped regex seed (writes to filter draft buffer).
- `y`: copy selected link.
- `/`: start inline explorer search.

### History sub-screen
- Table of RSS history entries (manual + auto adds) with timestamp/source/mode.
- Navigation with `j/k` + arrows.
- Read-only in MVP (no destructive actions).

## Data Model and Persistence

### `settings.toml` (durable config)
Keep RSS user-configurable/static values under `Settings.rss` in `src/config.rs`.

```rust
pub struct RssSettings {
    pub enabled: bool,
    pub poll_interval_secs: u64,
    pub max_preview_items: usize,
    pub feeds: Vec<RssFeed>,
    pub filters: Vec<RssFilter>,
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
```

### `persistence/rss.toml` (runtime state)
Store mutable RSS runtime state in a dedicated persistence file.

```rust
pub struct RssPersistedState {
    pub history: Vec<RssHistoryEntry>,
    pub last_sync_at: Option<String>,
    pub feed_errors: std::collections::HashMap<String, FeedSyncError>,
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
1. In `settings.toml`: RSS enabled/disabled, poll interval, feed list + per-feed enabled state, filter list.
2. In `persistence/rss.toml`: download/add history used for dedupe + Downloaded badge.
3. In `persistence/rss.toml`: last successful sync timestamp.
4. In `persistence/rss.toml`: last sync error per feed (for UI diagnostics).

### Retention
- Cap history at 1000 entries.
- Evict oldest first when inserting past cap.

### Non-persistent (session-only)
- Current RSS sub-screen.
- Current selected row indices per RSS sub-screen.
- Search/edit input drafts.
- Cached preview rows.

### Migration
- `Settings` defaults must initialize `rss` safely.
- Old settings files without RSS fields must load cleanly with defaults (`#[serde(default)]`).
- Add `src/persistence/` module and initialize `persistence/rss.toml` lazily on first RSS write.
- If `persistence/rss.toml` is missing or corrupt, recover to empty RSS state without blocking app startup.

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
- Persist runtime changes via `src/persistence/rss.rs` into `persistence/rss.toml`.

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
- Add `rss_ui` to `UiState` for RSS sub-screen-local state.
- Add `rss_runtime` app state for fetched rows and sync metadata.
- Add `RssScreen` enum for active RSS sub-screen (`Feeds|Filters|Explorer|History`).

### New app commands/events
Add async/runtime commands:
- `RssSyncNow`
- `RssPreviewUpdated`
- `RssSyncStatusUpdated`
- `RssFeedErrorUpdated`
- `RssDownloadSelected`
- `RssConfigUpdated`

Keep top-level mode transitions in TUI screen handlers.

### Keymap updates
- Add normal-mode keybinding (`r`) to open RSS mode.
- Add RSS sub-screen keymap (`f/l/e/h`) to help screen.
- Add footer hint for RSS key in normal footer.

## Render Plan

### RSS screen module
`src/tui/screens/rss.rs` contains:
- `draw(...)` (shared shell + sub-screen dispatch)
- `handle_event(...)` (global keys + sub-screen dispatch)
- reducer/effect helpers for testability

### Visual parity targets
- Explorer remains the primary item browsing surface.
- Match highlighting + dimming mirrors webapp.
- Downloaded badge displayed inline.
- Match count shown in explorer header (`N potential matches`).
- Sync status shown (`Last sync`, `Next sync`) and `Sync Now` action visible.
- History is visible as its own RSS sub-screen.
- Filters sub-screen live preview updates while typing filter drafts.

## Failure Handling
- Invalid regex draft: non-blocking inline error, keep draft text.
- Feed fetch failures: keep last good preview; annotate per-feed error status.
- Write/download failures: surface in `system_error` and append to RSS diagnostics.
- Clipboard failure (`y` or paste): soft error; allow manual input fallback.

## Test Plan

### Unit tests
1. Regex compile/validation behavior.
2. RSS sub-screen switching (`f/l/e/h`) behavior.
3. Key capture precedence while editing/searching.
4. Automatic match-priority sorting only when filter/search active.
5. Match/dim logic for explorer rows.
6. Preview title-dedupe behavior.
7. Ingest dedupe key behavior (`guid -> link -> title+source`).
8. History cap + insertion/eviction order.
9. Reducer coverage for RSS key actions per sub-screen.
10. Filters live draft preview updates as-you-type.

### Integration tests
1. Poll feed fixture -> matched item writes atomic `.magnet`/`.torrent`.
2. Manual row download writes once and marks history.
3. Manual add bypasses filters but still dedupes by history key.
4. Restart persistence split:
- `settings.toml`: feeds/filters/settings survive reload.
- `persistence/rss.toml`: history/sync metadata/errors survive reload.
5. Watch folder override works with RSS writes.

### UI parity checks
- Use as Filter pre-fills escaped title.
- Downloaded badge appears after manual add.
- Matches float to top only when search/filter active.
- No manual sort toggle exists in MVP.
- Sync Now control works and updates sync status.
- History sub-screen renders and reflects persisted history.
- Filters live preview and match counts update during draft typing.

## Implementation Phases
1. **RSS IA refactor**: introduce sub-screen model in `AppMode::Rss` (`Feeds|Filters|Explorer|History`), global key routing (`f/l/e/h`), shared shell.
2. **Feeds/Filters screens**: CRUD interactions, selection state, validation, and filters live draft preview.
3. **Explorer screen**: list rendering, match/dim/sort behavior, manual add/copy/use-as-filter actions.
4. **History screen**: table rendering + navigation, persisted state binding.
5. **Worker + ingest hardening**: polling/fetch/atomic writes/failure handling polish.
6. **Help/footer/status integration + regression tests**: key hints, sync metadata, parity verification.

## Scope Control
Initial in-TUI RSS ships without external web service. Sidecar plugin may coexist, but core TUI RSS is self-contained, persisted via `settings.toml` (config) + `persistence/rss.toml` (runtime state), and operates through Superseedr's existing file-based ingest model.
