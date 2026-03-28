# Superseedr Feature Review

**Date:** 2026-03-28

This document compares Superseedr's current feature set against qBittorrent (minus GUI) and identifies features that would make sense for a minimal yet modern TUI BitTorrent client.

---

## Current Feature Set (What Superseedr Has)

### Core BitTorrent
- ✅ BEP 3 (BitTorrent Protocol)
- ✅ BEP 5 (DHT) - optional feature flag for private builds
- ✅ BEP 9 (Magnet metadata extension)
- ✅ BEP 10 (Extension Protocol)
- ✅ BEP 11 (PEX) - optional feature flag
- ✅ BEP 19 (WebSeed/HTTP seeding)
- ✅ BEP 52 (BitTorrent v2 + Merkle verification)
- ✅ TCP peer connections with split read/write tasks
- ✅ Tit-for-tat choking algorithm
- ✅ Rarest-first piece selection
- ✅ File-level priority (Normal/High/Skip)

### Modern TUI Experience
- ✅ 60 FPS animated interface with themes (40 built-in)
- ✅ Bandwidth graphs with multiple time ranges (1m to 1y)
- ✅ Swarm heatmap / peer activity visualization
- ✅ "Matrix-style" block particle stream
- ✅ Peer lifecycle scatterplot
- ✅ Self-tuning resource allocator
- ✅ Per-torrent and system activity charts with persistence

### Operations & Automation
- ✅ RSS feed management with filtering (fuzzy + regex)
- ✅ Watch folder for auto-ingest (.torrent, .magnet, .path files)
- ✅ CLI commands (`add`, `stop-client`)
- ✅ JSON state dump for external monitoring
- ✅ Docker + Gluetun VPN integration with dynamic port reload
- ✅ Integrity prober for background data verification

---

## Missing vs qBittorrent (Core Features)

### 🔴 High Priority / Core Functionality

| Feature | qBittorrent | Superseedr | Notes |
|---------|-------------|------------|-------|
| **uTP transport** | ✅ | ❌ | UDP-based transport for better congestion control and NAT traversal. Critical for some ISPs. |
| **IPv6 support** | ✅ | ❌ | Modern internet standard, expanding peer pools significantly |
| **UPnP/NAT-PMP** | ✅ | ❌ | Automatic port forwarding for home users without VPN |
| **UDP Trackers** | ✅ | ❌ | Only HTTP trackers supported. UDP is faster and more common now |
| **Per-torrent speed limits** | ✅ | ❌ | Only global limits exist |
| **Sequential download** | ✅ | ❌ | Essential for streaming video while downloading |
| **Multi-select / bulk actions** | ✅ | ❌ | Can only act on one torrent at a time |
| **Torrent recheck (force)** | ✅ | Partial | Has background integrity prober but no user-triggered full recheck |
| **Download location change** | ✅ | ❌ | Cannot move downloaded files to different location post-add |

### 🟡 Medium Priority / Quality of Life

| Feature | qBittorrent | Superseedr | Notes |
|---------|-------------|------------|-------|
| **Tracker list editing** | ✅ | ❌ | Cannot add/remove/replace trackers on existing torrent |
| **Peer details/flags expanded** | ✅ | Partial | Has peer list but limited info (no client version display, limited flags) |
| **Download queue/ranking** | ✅ | ❌ | No priority queue for multiple downloads |
| **Super-seeding mode** | ✅ | ❌ | Initial seeding optimization for new torrents |
| **Torrent labels/tags** | ✅ | ❌ | No categorization system |
| **Search plugins** | ✅ | ❌ | No built-in torrent search |
| **Bandwidth scheduler** | ✅ | ❌ | Time-based speed limits |
| **IP blocklist** | ✅ | ❌ | No peer blocking capability |
| **Download-first/last pieces** | ✅ | ❌ | For streaming/partial preview |

### 🟢 Lower Priority / Advanced

| Feature | qBittorrent | Superseedr | Notes |
|---------|-------------|------------|-------|
| **Create torrent dialog** | ✅ | ❌ | No torrent creation UI (CLI only?) |
| **Tracker tier management** | ✅ | ❌ | No tier/backup tracker logic |
| **Private torrent flag handling** | ✅ | Partial | Private builds disable DHT/PEX, but no per-torrent private flag enforcement |
| **Logging UI** | ✅ | ❌ | Logs go to file, no in-app log viewer |
| **Session restore granularity** | ✅ | ❌ | Crashes lose partial piece state |

---

## Features That Would Make Sense for a Minimal Modern TUI

### Essential Additions (Small Scope, High Impact)

#### 1. Multi-select with bulk actions
- Visual indication (marked rows with different styling)
- `v` to toggle selection on current row
- `V` to select all / clear selection
- Actions apply to selection: pause, resume, delete
- Minimal implementation: track `HashSet<usize>` of selected indices

#### 2. Force recheck command
- `R` (shift+r) on selected torrent triggers full hash recheck
- Progress indication in activity message
- Already has `IntegrityScheduler`, just needs user-triggered path

#### 3. Sequential/priority download mode
- Single key toggle (`o` for "ordered")
- Visually indicated in torrent list
- Downloads pieces from start to end instead of rarest-first
- Critical for "watch while downloading" use case

#### 4. Per-torrent speed limits
- Add to config screen or inline edit
- Override global limits per-torrent
- Important for managing bandwidth across multiple downloads

#### 5. UDP tracker support
- BEP 15 - already widespread
- Same announce semantics, just over UDP
- Faster handshake, less server load

### Quality of Life (Fits Minimal Ethos)

#### 6. Torrent tags/categories
- Simple string labels, not complex hierarchies
- Filter torrent list by tag
- Persisted with torrent settings
- Minimal: just a `Option<String>` tag field

#### 7. Tracker management on existing torrent
- Add/Remove trackers from a running torrent
- Show tracker status/tier
- Tracker error messages visible in UI

#### 8. Download location change
- Move files to new location without re-download
- Update path in torrent state
- Trigger recheck at new location

#### 9. Expanded peer info
- Show client name (parse from peer_id)
- Show connection type (incoming/outgoing)
- Show encryption status
- Show progress percentage per-peer

#### 10. In-app log viewer
- New screen (`L` key)
- Scrollable, tail mode
- Log level filtering
- Already has structured logging infrastructure

### Network Modernization

#### 11. uTP transport
- Major undertaking but increasingly essential
- Better for congested networks
- NAT hole punching benefits
- Could be feature-flagged as optional

#### 12. IPv6 support
- Expands peer pool
- Modern standard
- Requires binding to `[::]` and address family handling

#### 13. UPnP port mapping
- Critical for non-VPN home users
- Many routers support it
- Automatic external port discovery

### Power User Features

#### 14. IP/Peer blocklist
- Simple blocklist file (e.g., `blocklist.txt`)
- Auto-disconnect blocked peers
- Could leverage existing peer info tracking

#### 15. Bandwidth scheduler
- Simple time-of-day rules
- "Slow mode" during work hours
- Minimal: just time-based limit overrides

#### 16. Super-seeding / initial seeding mode
- Optimize for seeding new torrents
- Sends pieces strategically to maximize spread
- Toggle per-torrent

---

## Architecture Observations

### Strengths
- Clean Actor-based architecture with Action/Effect pattern
- Good separation of concerns (network, storage, state)
- Self-tuning resource allocator is innovative for a TUI app
- Excellent test coverage with fuzzing/integration harness

### Technical Debt/Considerations
- No `uTP` implementation means the networking stack would need significant expansion
- Adding features like per-torrent limits requires touching the `TorrentManager` actor
- Multi-select would need careful state management in `AppState`

---

## Recommended Priority Order

| Priority | Feature | Effort | Impact | Rationale |
|----------|---------|--------|--------|-----------|
| 1 | Multi-select + bulk actions | Medium | High | Transforms daily usability |
| 2 | Force recheck | Low | High | Trivial to add, essential utility |
| 3 | Sequential download | Medium | High | Enables streaming use case |
| 4 | UDP trackers | Medium | Medium | Expands tracker compatibility |
| 5 | Per-torrent speed limits | Medium | High | Core missing feature |
| 6 | Tags/categories | Low | Medium | Organization at scale |
| 7 | uTP | High | High | Major undertaking but high value |
| 8 | In-app log viewer | Low | Medium | Debugging without leaving TUI |
| 9 | UPnP | Medium | Medium | Home user convenience |
| 10 | IPv6 | Medium | Medium | Future-proofing |

---

## Detailed Feature Specifications

### Multi-Select Implementation Notes

**State Changes:**
```rust
// Add to UiState
pub selected_torrent_indices: HashSet<usize>,
pub selection_mode: bool,
```

**Key Bindings:**
- `v` - Toggle selection on current row
- `V` - Select all visible torrents
- `Escape` - Clear selection
- Actions (`p`, `d`, etc.) apply to selection if non-empty, else single selection

**Visual:**
- Selected rows get different background color
- Selection count shown in status bar

**Affected Files:**
- `src/app.rs` - State management
- `src/tui/screens/normal.rs` - Rendering
- `src/tui/events.rs` - Input handling

### Force Recheck Implementation Notes

**Command:**
```rust
pub enum ManagerCommand {
    // ... existing
    ForceRecheck,  // New command
}
```

**Flow:**
1. User presses `R` on torrent
2. Send `ManagerCommand::ForceRecheck` to `TorrentManager`
3. Manager resets piece verification state
4. Integrity prober picks up work
5. Progress shown in activity message

**Affected Files:**
- `src/torrent_manager/mod.rs` - Add command variant
- `src/torrent_manager/manager.rs` - Handle command
- `src/tui/events.rs` - Key binding

### Sequential Download Implementation Notes

**State:**
```rust
pub struct TorrentSettings {
    // ... existing
    pub sequential_download: bool,
}
```

**Piece Selection Change:**
- In `piece_manager.rs`, add mode switch
- When `sequential_download == true`, select lowest-index missing piece
- When `false`, use existing rarest-first

**Key Binding:**
- `o` - Toggle sequential mode on selected torrent

**Affected Files:**
- `src/config.rs` - Add setting
- `src/torrent_manager/piece_manager.rs` - Selection logic
- `src/app.rs` - State display

### UDP Tracker Implementation Notes

**Protocol (BEP 15):**
- UDP socket connection to tracker
- Connect request/response (action=0)
- Announce request/response (action=1)
- Scrape request/response (action=2)

**New Module:**
```rust
// src/tracker/udp_client.rs
pub async fn udp_announce(...) -> Result<TrackerResponse, TrackerError>
```

**Integration:**
- Detect tracker URL scheme (`udp://` vs `http://`)
- Route to appropriate client

**Affected Files:**
- `src/tracker/mod.rs` - Module exports
- `src/tracker/client.rs` - HTTP client (existing)
- `src/tracker/udp_client.rs` - New UDP client
- `src/torrent_manager/manager.rs` - Tracker selection logic

---

## References

- [BEP 15: UDP Tracker Protocol](http://www.bittorrent.org/beps/bep_0015.html)
- [BEP 29: uTP](http://www.bittorrent.org/beps/bep_0029.html)
- [BEP 55: Holepunch extension](http://www.bittorrent.org/beps/bep_0055.html)
- [qBittorrent Feature List](https://www.qbittorrent.org/features)
