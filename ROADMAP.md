# superseedr Roadmap

This document outlines the development roadmap for **superseedr**, a modern, privacy-first, terminal-based BitTorrent client written in Rust.

## Guiding Principles

* Performance-first and resource efficient
* Privacy-by-default
* Rich terminal user experience
* Real-time observability
* Modular, extensible architecture

---

## Phase: 0 - Pre-v1.0

**Goal:** Ship a stable, reliable `v1.0.0` release.

### Core Stability
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Ongoing testing across various platforms and terminals | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Expand unit test coverage | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Add full cross-platform testing | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Set up CI/CD pipelines | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Improve shutdown safety and crash edge cases | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Harden crash recovery for persistent state | [Issue #____]

### Configuration
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Atomic config writes - configs automatically saved to disk after any change | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Live config reload support | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Improve OS-level magnet link handling | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Improve .torrent file association handling | [Issue #____]

### Packaging & Distribution
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Stabilize Windows MSI builds | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Stabilize macOS PKG builds | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Stabilize Debian/Ubuntu DEB builds | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Improve cargo install flow | [Issue #____]

### Docker & VPN
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Improve non-VPN Docker image stability | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Improve Gluetun VPN Docker stability | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Add better Docker health checks | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Add Docker troubleshooting documentation | [Issue #____]

### Terminal UI (TUI)
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Improve in-app help screens | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Add keybindings guide UI | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Optimize high-FPS TUI performance | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Improve startup experience / onboarding | [Issue #____]
- **phase: 0 - pre-v1.0 (UNCONFIRMED)** | Better error and warning visuals | [Issue #____]

---

## Phase: 1 - v1.x

**Goal:** Improve flexibility, usability, and long-lived workloads.

### Docker & Deployment
- **phase: 1 - v1.x (UNCONFIRMED)** | Add detached/headless mode - allow for detached and attach sessions, TUI off mode | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Add TUI attach/detach support | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Improve Docker Compose examples | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Improve VPN port auto-reload | [Issue #____]

### Torrent Management
- **phase: 1 - v1.x (UNCONFIRMED)** | Selective file downloading - allow users to choose which specific files inside a multi-file torrent they want to download | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Sequential downloading - download pieces in order, primarily useful for streaming media files while they're downloading | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Torrent prioritization / queueing - allow users to set priorities for torrents and configure limits on the number of active downloading or seeding torrents | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Torrent queue limits | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Per-torrent upload limits | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Per-torrent download limits | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Per-torrent ratio goals | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Per-torrent connection limits | [Issue #____]

### TUI Enhancements
- **phase: 1 - v1.x (UNCONFIRMED)** | Layout edit mode - allow the user to resize or drag and drop the layout of the panels | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Draggable and resizable panels | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | File tree view for torrents | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | TUI files view hierarchy - add a popup that shows in an interactive hierarchy view and live progress for the files of the torrent | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Live per-file progress display | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Advanced sorting options | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Advanced filtering options | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | In-app configuration editor | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Advanced TUI controls - add more interactive features to the TUI, like in-app configuration editing, more detailed peer/file views, advanced sorting/filtering | [Issue #____]

### Observability & Analytics
- **phase: 1 - v1.x (UNCONFIRMED)** | Persist network history to disk | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Improved peer visualizations | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Improved swarm health metrics | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Tracker scraping - implement the ability to query trackers for seeder/leecher counts without doing a full announce | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Disk health monitoring | [Issue #____]

### Torrent History
- **phase: 1 - v1.x (UNCONFIRMED)** | Torrent logbook - historic log book of all torrents added and deleted | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Searchable torrent history | [Issue #____]
- **phase: 1 - v1.x (UNCONFIRMED)** | Redownload-from-history feature | [Issue #____]

---

## Phase: 2 - v2.0+

**Goal:** Advanced networking, extensibility, and automation.

### Networking
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Full IPv6 peer support - allow connecting to IPv6 peers | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | IPv6 tracker announce support - announcing to IPv6 trackers | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Parse compact peers6 responses | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | UPnP port forwarding - automatically configure port forwarding on compatible routers to improve connectability | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | NAT-PMP port forwarding | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Encrypted peer connections (MSE/PE) | [Issue #____]

### Architecture Refactors
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Reduce external dependencies - codebase refactor to reduce dependencies by implementing some of these features in the codebase | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Fully async torrent validation - refactor for handling torrent validation and revalidations async | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Non-blocking rehashing pipeline | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Engine/UI separation | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Config migration system | [Issue #____]

### Headless & Remote Control
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Daemon/service mode | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | REST/RPC control API | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Web UI prototype | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Remote monitoring dashboard | [Issue #____]

### Automation
- **phase: 2 - v2.0+ (UNCONFIRMED)** | RSS feed monitoring - automatically monitor RSS feeds and download new torrents matching user-defined filters | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Filter-based auto downloads | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Post-download scripts/hooks | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Webhook notifications | [Issue #____]

### Power User TUI
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Multiple saved layouts | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Plugin system | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Deep peer inspection UI | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Advanced peer filtering | [Issue #____]
- **phase: 2 - v2.0+ (UNCONFIRMED)** | Search across torrents/peers/files | [Issue #____]

---

## Phase: Future

**Goal:** Long-term exploratory features and research.

- **phase: future (UNCONFIRMED)** | Crash dump ring buffer - fully replayable crash dump of torrent state actions | [Issue #____]

---

## Roadmap Tracking

All roadmap items will be connected to:
* GitHub Issues
* GitHub Milestones
* GitHub Project Boards

When an issue is created, replace `[Issue #____]` with the actual issue number, e.g., `[Issue #123]`.

Project Lead to confirm priorities, remove `(UNCONFIRMED)`, once the action/ deliverable Phase has been confirmed.

---

## Notes

This file is a **living document** and will evolve over time based on:
* User feedback
* Real-world usage
* Contributor activity
* Project lead prioritization

---

**Last Updated:** January 2026
