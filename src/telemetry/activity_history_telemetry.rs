// SPDX-FileCopyrightText: 2026 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::AppState;
use crate::persistence::activity_history::{
    enforce_retention_caps, retain_only_torrent_series_for_keys, ActivityHistoryPersistedState,
    ActivityHistorySeries, ActivityHistorySeriesRollupState,
};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct ActivityHistoryTelemetry;

impl ActivityHistoryTelemetry {
    pub fn on_second_tick(app_state: &mut AppState) {
        let now_unix = current_unix_time();
        let active_torrent_keys: HashSet<String> = app_state
            .torrent_list_order
            .iter()
            .map(hex::encode)
            .collect();

        retain_only_torrent_series_for_keys(
            &mut app_state.activity_history_state,
            &mut app_state.activity_history_rollups,
            &active_torrent_keys,
        );

        let cpu_x10 = (app_state.cpu_usage.clamp(0.0, 100.0) * 10.0).round() as u64;
        let ram_x10 = (app_state.ram_usage_percent.clamp(0.0, 100.0) * 10.0).round() as u64;
        let tuning_current = app_state.current_tuning_score;
        let tuning_best = app_state.last_tuning_score;

        let mut changed = false;
        changed |= app_state.activity_history_rollups.cpu.ingest_second_sample(
            &mut app_state.activity_history_state.cpu,
            now_unix,
            cpu_x10,
            0,
        );
        changed |= app_state.activity_history_rollups.ram.ingest_second_sample(
            &mut app_state.activity_history_state.ram,
            now_unix,
            ram_x10,
            0,
        );
        changed |= app_state
            .activity_history_rollups
            .disk
            .ingest_second_sample(
                &mut app_state.activity_history_state.disk,
                now_unix,
                app_state.avg_disk_read_bps,
                app_state.avg_disk_write_bps,
            );
        changed |= app_state
            .activity_history_rollups
            .tuning
            .ingest_second_sample(
                &mut app_state.activity_history_state.tuning,
                now_unix,
                tuning_current,
                tuning_best,
            );

        for info_hash in &app_state.torrent_list_order {
            let key = hex::encode(info_hash);
            let (dl_bps, ul_bps) = app_state
                .torrents
                .get(info_hash)
                .map(|torrent| {
                    (
                        torrent.smoothed_download_speed_bps,
                        torrent.smoothed_upload_speed_bps,
                    )
                })
                .unwrap_or((0, 0));

            let series = app_state
                .activity_history_state
                .torrents
                .entry(key.clone())
                .or_default();
            let rollups = app_state
                .activity_history_rollups
                .torrents
                .entry(key)
                .or_default();
            changed |= rollups.ingest_second_sample(series, now_unix, dl_bps, ul_bps);
        }

        if changed {
            app_state.activity_history_dirty = true;
        }

        enforce_retention_caps(&mut app_state.activity_history_state);
    }

    pub fn apply_loaded_state(app_state: &mut AppState, state: ActivityHistoryPersistedState) {
        let was_dirty = app_state.activity_history_dirty;
        let merged = merge_state_for_late_restore(&app_state.activity_history_state, state);
        app_state.activity_history_state = merged;
        app_state.activity_history_rollups =
            crate::persistence::activity_history::ActivityHistoryRollupState::from_persisted(
                &app_state.activity_history_state,
            );
        app_state.activity_history_dirty = was_dirty;
    }
}

fn current_unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn latest_second_timestamp(series: &ActivityHistorySeries) -> u64 {
    series
        .tiers
        .second_1s
        .last()
        .map(|point| point.ts_unix)
        .unwrap_or(0)
}

fn replay_live_seconds_into_loaded(
    live_series: &ActivityHistorySeries,
    merged_series: &mut ActivityHistorySeries,
) {
    let replay_cutoff_unix = latest_second_timestamp(merged_series);
    let mut rollups = ActivityHistorySeriesRollupState::from_snapshot(&merged_series.rollups);

    for point in live_series
        .tiers
        .second_1s
        .iter()
        .filter(|point| point.ts_unix > replay_cutoff_unix)
    {
        let _ = rollups.ingest_second_sample(
            merged_series,
            point.ts_unix,
            point.primary,
            point.secondary,
        );
    }
}

fn merge_state_for_late_restore(
    live_state: &ActivityHistoryPersistedState,
    loaded_state: ActivityHistoryPersistedState,
) -> ActivityHistoryPersistedState {
    let mut merged = loaded_state;
    merged.schema_version = merged.schema_version.max(live_state.schema_version);
    merged.updated_at_unix = merged.updated_at_unix.max(live_state.updated_at_unix);

    replay_live_seconds_into_loaded(&live_state.cpu, &mut merged.cpu);
    replay_live_seconds_into_loaded(&live_state.ram, &mut merged.ram);
    replay_live_seconds_into_loaded(&live_state.disk, &mut merged.disk);
    replay_live_seconds_into_loaded(&live_state.tuning, &mut merged.tuning);

    let mut all_torrents: HashSet<String> = merged.torrents.keys().cloned().collect();
    all_torrents.extend(live_state.torrents.keys().cloned());

    for info_hash in all_torrents {
        if let Some(live_series) = live_state.torrents.get(&info_hash) {
            let merged_series = merged.torrents.entry(info_hash).or_default();
            replay_live_seconds_into_loaded(live_series, merged_series);
        }
    }

    enforce_retention_caps(&mut merged);
    merged
}
