// SPDX-FileCopyrightText: 2026 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::config::get_app_paths;
use crate::persistence::network_history::{
    HOUR_1H_CAP, MINUTE_15M_CAP, MINUTE_1M_CAP, SECOND_1S_CAP,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tracing::{event as tracing_event, Level};

pub const ACTIVITY_HISTORY_SCHEMA_VERSION: u32 = 1;
const ACTIVITY_HISTORY_FILE_NAME: &str = "activity_history.json";
const ACTIVITY_HISTORY_TEMP_EXTENSION: &str = "json.tmp";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ActivityHistoryPoint {
    pub ts_unix: u64,
    pub primary: u64,
    pub secondary: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ActivityHistoryTiers {
    pub second_1s: Vec<ActivityHistoryPoint>,
    pub minute_1m: Vec<ActivityHistoryPoint>,
    pub minute_15m: Vec<ActivityHistoryPoint>,
    pub hour_1h: Vec<ActivityHistoryPoint>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(default)]
pub struct PersistedRollupAccumulator {
    pub count: u32,
    pub primary_sum: u128,
    pub secondary_sum: u128,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ActivityHistoryRollupSnapshot {
    pub second_to_minute: PersistedRollupAccumulator,
    pub minute_to_15m: PersistedRollupAccumulator,
    pub m15_to_hour: PersistedRollupAccumulator,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ActivityHistorySeries {
    pub rollups: ActivityHistoryRollupSnapshot,
    pub tiers: ActivityHistoryTiers,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct ActivityHistoryPersistedState {
    pub schema_version: u32,
    pub updated_at_unix: u64,
    pub cpu: ActivityHistorySeries,
    pub ram: ActivityHistorySeries,
    pub disk: ActivityHistorySeries,
    pub tuning: ActivityHistorySeries,
    pub torrents: HashMap<String, ActivityHistorySeries>,
}

impl Default for ActivityHistoryPersistedState {
    fn default() -> Self {
        Self {
            schema_version: ACTIVITY_HISTORY_SCHEMA_VERSION,
            updated_at_unix: 0,
            cpu: ActivityHistorySeries::default(),
            ram: ActivityHistorySeries::default(),
            disk: ActivityHistorySeries::default(),
            tuning: ActivityHistorySeries::default(),
            torrents: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RollupAccumulator {
    count: u32,
    primary_sum: u128,
    secondary_sum: u128,
}

impl RollupAccumulator {
    fn push(&mut self, point: &ActivityHistoryPoint) {
        self.count += 1;
        self.primary_sum += point.primary as u128;
        self.secondary_sum += point.secondary as u128;
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

impl From<&RollupAccumulator> for PersistedRollupAccumulator {
    fn from(accumulator: &RollupAccumulator) -> Self {
        Self {
            count: accumulator.count,
            primary_sum: accumulator.primary_sum,
            secondary_sum: accumulator.secondary_sum,
        }
    }
}

impl From<&PersistedRollupAccumulator> for RollupAccumulator {
    fn from(accumulator: &PersistedRollupAccumulator) -> Self {
        Self {
            count: accumulator.count,
            primary_sum: accumulator.primary_sum,
            secondary_sum: accumulator.secondary_sum,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivityHistorySeriesRollupState {
    second_to_minute: RollupAccumulator,
    minute_to_15m: RollupAccumulator,
    m15_to_hour: RollupAccumulator,
}

impl ActivityHistorySeriesRollupState {
    pub fn to_snapshot(&self) -> ActivityHistoryRollupSnapshot {
        ActivityHistoryRollupSnapshot {
            second_to_minute: PersistedRollupAccumulator::from(&self.second_to_minute),
            minute_to_15m: PersistedRollupAccumulator::from(&self.minute_to_15m),
            m15_to_hour: PersistedRollupAccumulator::from(&self.m15_to_hour),
        }
    }

    pub fn from_snapshot(snapshot: &ActivityHistoryRollupSnapshot) -> Self {
        Self {
            second_to_minute: RollupAccumulator::from(&snapshot.second_to_minute),
            minute_to_15m: RollupAccumulator::from(&snapshot.minute_to_15m),
            m15_to_hour: RollupAccumulator::from(&snapshot.m15_to_hour),
        }
    }

    pub fn ingest_second_sample(
        &mut self,
        series: &mut ActivityHistorySeries,
        ts_unix: u64,
        primary: u64,
        secondary: u64,
    ) -> bool {
        let second_point = ActivityHistoryPoint {
            ts_unix,
            primary,
            secondary,
        };
        let mut should_persist = !is_zero_point(&second_point);
        series.tiers.second_1s.push(second_point.clone());
        cap_vec(&mut series.tiers.second_1s, SECOND_1S_CAP);

        self.second_to_minute.push(&second_point);
        if self.second_to_minute.count >= 60 {
            let minute_point = make_rollup_point(&self.second_to_minute, ts_unix);
            self.second_to_minute.clear();
            should_persist |= !is_zero_point(&minute_point);

            series.tiers.minute_1m.push(minute_point.clone());
            cap_vec(&mut series.tiers.minute_1m, MINUTE_1M_CAP);

            self.minute_to_15m.push(&minute_point);
            if self.minute_to_15m.count >= 15 {
                let m15_point = make_rollup_point(&self.minute_to_15m, ts_unix);
                self.minute_to_15m.clear();
                should_persist |= !is_zero_point(&m15_point);

                series.tiers.minute_15m.push(m15_point.clone());
                cap_vec(&mut series.tiers.minute_15m, MINUTE_15M_CAP);

                self.m15_to_hour.push(&m15_point);
                if self.m15_to_hour.count >= 4 {
                    let hour_point = make_rollup_point(&self.m15_to_hour, ts_unix);
                    self.m15_to_hour.clear();
                    should_persist |= !is_zero_point(&hour_point);

                    series.tiers.hour_1h.push(hour_point);
                    cap_vec(&mut series.tiers.hour_1h, HOUR_1H_CAP);
                }
            }
        }

        series.rollups = self.to_snapshot();
        should_persist
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivityHistoryRollupState {
    pub cpu: ActivityHistorySeriesRollupState,
    pub ram: ActivityHistorySeriesRollupState,
    pub disk: ActivityHistorySeriesRollupState,
    pub tuning: ActivityHistorySeriesRollupState,
    pub torrents: HashMap<String, ActivityHistorySeriesRollupState>,
}

impl ActivityHistoryRollupState {
    pub fn from_persisted(state: &ActivityHistoryPersistedState) -> Self {
        let torrents = state
            .torrents
            .iter()
            .map(|(info_hash, series)| {
                (
                    info_hash.clone(),
                    ActivityHistorySeriesRollupState::from_snapshot(&series.rollups),
                )
            })
            .collect();
        Self {
            cpu: ActivityHistorySeriesRollupState::from_snapshot(&state.cpu.rollups),
            ram: ActivityHistorySeriesRollupState::from_snapshot(&state.ram.rollups),
            disk: ActivityHistorySeriesRollupState::from_snapshot(&state.disk.rollups),
            tuning: ActivityHistorySeriesRollupState::from_snapshot(&state.tuning.rollups),
            torrents,
        }
    }

    pub fn sync_snapshots_to_state(&self, state: &mut ActivityHistoryPersistedState) {
        state.cpu.rollups = self.cpu.to_snapshot();
        state.ram.rollups = self.ram.to_snapshot();
        state.disk.rollups = self.disk.to_snapshot();
        state.tuning.rollups = self.tuning.to_snapshot();
        for (info_hash, rollups) in &self.torrents {
            if let Some(series) = state.torrents.get_mut(info_hash) {
                series.rollups = rollups.to_snapshot();
            }
        }
    }
}

fn make_rollup_point(acc: &RollupAccumulator, ts_unix: u64) -> ActivityHistoryPoint {
    if acc.count == 0 {
        return ActivityHistoryPoint {
            ts_unix,
            ..Default::default()
        };
    }
    ActivityHistoryPoint {
        ts_unix,
        primary: (acc.primary_sum / acc.count as u128) as u64,
        secondary: (acc.secondary_sum / acc.count as u128) as u64,
    }
}

fn cap_vec<T>(vec: &mut Vec<T>, cap: usize) {
    if vec.len() > cap {
        let overflow = vec.len() - cap;
        vec.drain(0..overflow);
    }
}

pub fn enforce_retention_caps(state: &mut ActivityHistoryPersistedState) {
    cap_series(&mut state.cpu);
    cap_series(&mut state.ram);
    cap_series(&mut state.disk);
    cap_series(&mut state.tuning);
    for series in state.torrents.values_mut() {
        cap_series(series);
    }
}

pub fn retain_only_torrent_series_for_keys(
    state: &mut ActivityHistoryPersistedState,
    rollups: &mut ActivityHistoryRollupState,
    keep_keys: &HashSet<String>,
) {
    state.torrents.retain(|key, _| keep_keys.contains(key));
    rollups.torrents.retain(|key, _| keep_keys.contains(key));
}

fn cap_series(series: &mut ActivityHistorySeries) {
    cap_vec(&mut series.tiers.second_1s, SECOND_1S_CAP);
    cap_vec(&mut series.tiers.minute_1m, MINUTE_1M_CAP);
    cap_vec(&mut series.tiers.minute_15m, MINUTE_15M_CAP);
    cap_vec(&mut series.tiers.hour_1h, HOUR_1H_CAP);
}

pub fn is_zero_point(point: &ActivityHistoryPoint) -> bool {
    point.primary == 0 && point.secondary == 0
}

fn sparse_points_for_persistence(points: &[ActivityHistoryPoint]) -> Vec<ActivityHistoryPoint> {
    points
        .iter()
        .filter(|point| !is_zero_point(point))
        .cloned()
        .collect()
}

fn sparse_series_for_persistence(series: &ActivityHistorySeries) -> ActivityHistorySeries {
    ActivityHistorySeries {
        rollups: series.rollups.clone(),
        tiers: ActivityHistoryTiers {
            second_1s: sparse_points_for_persistence(&series.tiers.second_1s),
            minute_1m: sparse_points_for_persistence(&series.tiers.minute_1m),
            minute_15m: sparse_points_for_persistence(&series.tiers.minute_15m),
            hour_1h: sparse_points_for_persistence(&series.tiers.hour_1h),
        },
    }
}

fn sparse_state_for_persistence(
    state: &ActivityHistoryPersistedState,
) -> ActivityHistoryPersistedState {
    let mut sparse = ActivityHistoryPersistedState {
        schema_version: state.schema_version,
        updated_at_unix: state.updated_at_unix,
        cpu: sparse_series_for_persistence(&state.cpu),
        ram: sparse_series_for_persistence(&state.ram),
        disk: sparse_series_for_persistence(&state.disk),
        tuning: sparse_series_for_persistence(&state.tuning),
        torrents: HashMap::new(),
    };

    for (info_hash, series) in &state.torrents {
        let sparse_series = sparse_series_for_persistence(series);
        if has_any_point(&sparse_series) {
            sparse.torrents.insert(info_hash.clone(), sparse_series);
        }
    }

    sparse
}

fn has_any_point(series: &ActivityHistorySeries) -> bool {
    !series.tiers.second_1s.is_empty()
        || !series.tiers.minute_1m.is_empty()
        || !series.tiers.minute_15m.is_empty()
        || !series.tiers.hour_1h.is_empty()
}

pub fn activity_history_state_file_path() -> io::Result<PathBuf> {
    let (_, data_dir) = get_app_paths().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not resolve app data directory for activity history persistence",
        )
    })?;
    Ok(data_dir
        .join("persistence")
        .join(ACTIVITY_HISTORY_FILE_NAME))
}

pub fn load_activity_history_state() -> ActivityHistoryPersistedState {
    match activity_history_state_file_path() {
        Ok(path) => load_activity_history_state_from_path(&path),
        Err(e) => {
            tracing_event!(
                Level::WARN,
                "Failed to resolve activity history persistence path. Using default state: {}",
                e
            );
            ActivityHistoryPersistedState::default()
        }
    }
}

pub fn save_activity_history_state(state: &ActivityHistoryPersistedState) -> io::Result<()> {
    let path = activity_history_state_file_path()?;
    save_activity_history_state_to_path(state, &path)
}

fn load_activity_history_state_from_path(path: &Path) -> ActivityHistoryPersistedState {
    if !path.exists() {
        return ActivityHistoryPersistedState::default();
    }

    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<ActivityHistoryPersistedState>(&bytes) {
            Ok(mut state) => {
                if state.schema_version != ACTIVITY_HISTORY_SCHEMA_VERSION {
                    tracing_event!(
                        Level::WARN,
                        "Unsupported activity history schema version {} in {:?}. Resetting state.",
                        state.schema_version,
                        path
                    );
                    return ActivityHistoryPersistedState::default();
                }
                enforce_retention_caps(&mut state);
                state
            }
            Err(e) => {
                tracing_event!(
                    Level::WARN,
                    "Failed to decode activity history persistence file {:?}. Resetting state: {}",
                    path,
                    e
                );
                ActivityHistoryPersistedState::default()
            }
        },
        Err(e) => {
            tracing_event!(
                Level::WARN,
                "Failed to read activity history persistence file {:?}. Using empty state: {}",
                path,
                e
            );
            ActivityHistoryPersistedState::default()
        }
    }
}

fn save_activity_history_state_to_path(
    state: &ActivityHistoryPersistedState,
    path: &Path,
) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let sparse_state = sparse_state_for_persistence(state);
    let content = serde_json::to_vec(&sparse_state).map_err(io::Error::other)?;
    let tmp_path = path.with_extension(ACTIVITY_HISTORY_TEMP_EXTENSION);

    fs::write(&tmp_path, content)?;
    fs::rename(&tmp_path, path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rollup_ingest_creates_minute_point_after_sixty_seconds() {
        let mut series = ActivityHistorySeries::default();
        let mut rollups = ActivityHistorySeriesRollupState::default();
        for i in 0..60 {
            let changed = rollups.ingest_second_sample(&mut series, i, 10, 20);
            assert!(changed);
        }

        assert_eq!(series.tiers.second_1s.len(), 60);
        assert_eq!(series.tiers.minute_1m.len(), 1);
        assert_eq!(series.tiers.minute_1m[0].primary, 10);
        assert_eq!(series.tiers.minute_1m[0].secondary, 20);
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = tempdir().expect("create tempdir");
        let path = dir.path().join(ACTIVITY_HISTORY_FILE_NAME);

        let mut state = ActivityHistoryPersistedState {
            updated_at_unix: 1_777_777_777,
            ..Default::default()
        };
        state.cpu.tiers.second_1s.push(ActivityHistoryPoint {
            ts_unix: 1,
            primary: 250,
            secondary: 0,
        });
        state.torrents.insert(
            "abcd".to_string(),
            ActivityHistorySeries {
                tiers: ActivityHistoryTiers {
                    second_1s: vec![ActivityHistoryPoint {
                        ts_unix: 1,
                        primary: 100,
                        secondary: 200,
                    }],
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        save_activity_history_state_to_path(&state, &path).expect("save state");
        let loaded = load_activity_history_state_from_path(&path);

        assert_eq!(loaded.updated_at_unix, state.updated_at_unix);
        assert_eq!(loaded.cpu.tiers.second_1s, state.cpu.tiers.second_1s);
        assert_eq!(loaded.torrents.get("abcd"), state.torrents.get("abcd"));
    }

    #[test]
    fn retain_only_torrent_series_prunes_absent_keys() {
        let mut state = ActivityHistoryPersistedState::default();
        state
            .torrents
            .insert("keep".to_string(), ActivityHistorySeries::default());
        state
            .torrents
            .insert("drop".to_string(), ActivityHistorySeries::default());

        let mut rollups = ActivityHistoryRollupState::default();
        rollups.torrents.insert(
            "keep".to_string(),
            ActivityHistorySeriesRollupState::default(),
        );
        rollups.torrents.insert(
            "drop".to_string(),
            ActivityHistorySeriesRollupState::default(),
        );

        let keep = HashSet::from(["keep".to_string()]);
        retain_only_torrent_series_for_keys(&mut state, &mut rollups, &keep);

        assert!(state.torrents.contains_key("keep"));
        assert!(!state.torrents.contains_key("drop"));
        assert!(rollups.torrents.contains_key("keep"));
        assert!(!rollups.torrents.contains_key("drop"));
    }
}
