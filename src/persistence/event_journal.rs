// SPDX-FileCopyrightText: 2026 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::config::get_app_paths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tracing::{event as tracing_event, Level};

const EVENT_JOURNAL_FILE_NAME: &str = "event_journal.toml";
pub const EVENT_JOURNAL_CAP: usize = 5_000;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EventCategory {
    #[default]
    Ingest,
    TorrentLifecycle,
    DataHealth,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    #[default]
    IngestQueued,
    IngestAdded,
    IngestDuplicate,
    IngestInvalid,
    IngestFailed,
    TorrentCompleted,
    DataUnavailable,
    DataRecovered,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IngestOrigin {
    #[default]
    WatchFolder,
    RssAuto,
    RssManual,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IngestKind {
    #[default]
    TorrentFile,
    MagnetFile,
    PathFile,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventDetails {
    #[default]
    None,
    Ingest {
        origin: IngestOrigin,
        ingest_kind: IngestKind,
    },
    DataHealth {
        issue_count: usize,
        issue_files: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(default)]
pub struct EventJournalEntry {
    pub id: u64,
    pub host_id: Option<String>,
    pub ts_iso: String,
    pub category: EventCategory,
    pub event_type: EventType,
    pub torrent_name: Option<String>,
    pub info_hash_hex: Option<String>,
    pub source_watch_folder: Option<PathBuf>,
    pub source_path: Option<PathBuf>,
    pub correlation_id: Option<String>,
    pub message: Option<String>,
    pub details: EventDetails,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(default)]
pub struct EventJournalState {
    pub next_id: u64,
    pub entries: Vec<EventJournalEntry>,
}

pub fn event_journal_state_file_path() -> io::Result<PathBuf> {
    let (_, data_dir) = get_app_paths().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not resolve app data directory for event journal persistence",
        )
    })?;

    Ok(data_dir.join("persistence").join(EVENT_JOURNAL_FILE_NAME))
}

pub fn load_event_journal_state() -> EventJournalState {
    match event_journal_state_file_path() {
        Ok(path) => load_event_journal_state_from_path(&path),
        Err(e) => {
            tracing_event!(
                Level::WARN,
                "Failed to get event journal persistence path. Using empty state: {}",
                e
            );
            EventJournalState::default()
        }
    }
}

pub fn save_event_journal_state(state: &EventJournalState) -> io::Result<()> {
    let path = event_journal_state_file_path()?;
    save_event_journal_state_to_path(state, &path)
}

pub fn enforce_event_journal_retention(state: &mut EventJournalState) {
    if state.entries.len() > EVENT_JOURNAL_CAP {
        let overflow = state.entries.len() - EVENT_JOURNAL_CAP;
        state.entries.drain(0..overflow);
    }
}

fn load_event_journal_state_from_path(path: &Path) -> EventJournalState {
    if !path.exists() {
        return EventJournalState::default();
    }

    match fs::read_to_string(path) {
        Ok(content) => match toml::from_str::<EventJournalState>(&content) {
            Ok(mut state) => {
                enforce_event_journal_retention(&mut state);
                state
            }
            Err(e) => {
                tracing_event!(
                    Level::WARN,
                    "Failed to parse event journal file {:?}. Resetting event journal state: {}",
                    path,
                    e
                );
                EventJournalState::default()
            }
        },
        Err(e) => {
            tracing_event!(
                Level::WARN,
                "Failed to read event journal file {:?}. Using empty state: {}",
                path,
                e
            );
            EventJournalState::default()
        }
    }
}

fn save_event_journal_state_to_path(state: &EventJournalState, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut journal_state = state.clone();
    enforce_event_journal_retention(&mut journal_state);

    let content = toml::to_string_pretty(&journal_state).map_err(io::Error::other)?;
    let tmp_path = path.with_extension("toml.tmp");
    fs::write(&tmp_path, content)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempdir().expect("create tempdir");
        let path = dir.path().join("event_journal.toml");

        let state = load_event_journal_state_from_path(&path);
        assert_eq!(state, EventJournalState::default());
    }

    #[test]
    fn load_invalid_file_returns_default() {
        let dir = tempdir().expect("create tempdir");
        let path = dir.path().join("event_journal.toml");
        fs::write(&path, "not = [valid").expect("write malformed toml");

        let state = load_event_journal_state_from_path(&path);
        assert_eq!(state, EventJournalState::default());
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = tempdir().expect("create tempdir");
        let path = dir.path().join("event_journal.toml");

        let state = EventJournalState {
            next_id: 2,
            entries: vec![EventJournalEntry {
                id: 1,
                host_id: Some("node-a".to_string()),
                ts_iso: "2026-03-15T12:00:00Z".to_string(),
                category: EventCategory::Ingest,
                event_type: EventType::IngestAdded,
                torrent_name: Some("Sample Alpha Episode 1".to_string()),
                info_hash_hex: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()),
                source_watch_folder: Some(PathBuf::from("/watch")),
                source_path: Some(PathBuf::from("/watch/alpha.magnet")),
                correlation_id: Some("corr-1".to_string()),
                message: Some("Added torrent from watched magnet file".to_string()),
                details: EventDetails::Ingest {
                    origin: IngestOrigin::WatchFolder,
                    ingest_kind: IngestKind::MagnetFile,
                },
            }],
        };

        save_event_journal_state_to_path(&state, &path).expect("save event journal state");
        let loaded = load_event_journal_state_from_path(&path);

        assert_eq!(loaded, state);
    }

    #[test]
    fn retention_prunes_oldest_entries() {
        let mut state = EventJournalState {
            next_id: (EVENT_JOURNAL_CAP + 2) as u64,
            entries: (0..EVENT_JOURNAL_CAP + 1)
                .map(|idx| EventJournalEntry {
                    id: idx as u64,
                    ts_iso: format!("2026-03-15T12:00:{idx:02}Z"),
                    ..Default::default()
                })
                .collect(),
        };

        enforce_event_journal_retention(&mut state);

        assert_eq!(state.entries.len(), EVENT_JOURNAL_CAP);
        assert_eq!(state.entries.first().map(|entry| entry.id), Some(1));
    }
}
