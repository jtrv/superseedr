// SPDX-FileCopyrightText: 2026 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::FilePriority;
use crate::config::{load_torrent_metadata, Settings, TorrentMetadataEntry, TorrentSettings};
use crate::integrations::control::{
    ControlFilePriorityOverride, ControlPriorityTarget, ControlRequest,
};
use crate::persistence::event_journal::{ControlOrigin, EventDetails};
use crate::torrent_file::parser::from_bytes;
use crate::torrent_identity::{decode_info_hash, info_hash_from_torrent_source};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub fn find_torrent_settings_index_by_info_hash(
    settings: &Settings,
    info_hash: &[u8],
) -> Option<usize> {
    settings.torrents.iter().position(|torrent| {
        info_hash_from_torrent_source(&torrent.torrent_or_magnet).as_deref() == Some(info_hash)
    })
}

pub fn describe_priority_target(target: &ControlPriorityTarget) -> String {
    match target {
        ControlPriorityTarget::FileIndex(index) => format!("index {}", index),
        ControlPriorityTarget::FilePath(path) => format!("path {}", path),
    }
}

pub fn online_control_success_message(request: &ControlRequest) -> String {
    match request {
        ControlRequest::Pause { info_hash_hex } => {
            format!("Queued pause request for torrent '{}'", info_hash_hex)
        }
        ControlRequest::Resume { info_hash_hex } => {
            format!("Queued resume request for torrent '{}'", info_hash_hex)
        }
        ControlRequest::Delete {
            info_hash_hex,
            delete_files,
        } => {
            if *delete_files {
                format!("Queued purge request for torrent '{}'", info_hash_hex)
            } else {
                format!("Queued remove request for torrent '{}'", info_hash_hex)
            }
        }
        ControlRequest::SetFilePriority {
            info_hash_hex,
            target,
            priority,
        } => format!(
            "Queued file priority request for torrent '{}' ({}) -> {:?}",
            info_hash_hex,
            describe_priority_target(target),
            priority
        ),
        ControlRequest::AddTorrentFile { source_path, .. } => format!(
            "Queued add request for torrent file '{}'",
            source_path.display()
        ),
        ControlRequest::AddMagnet { magnet_link, .. } => {
            let label = magnet_link
                .split('&')
                .next()
                .unwrap_or(magnet_link.as_str());
            format!("Queued add request for magnet '{}'", label)
        }
        ControlRequest::StatusNow
        | ControlRequest::StatusFollowStart { .. }
        | ControlRequest::StatusFollowStop => "Queued control request.".to_string(),
    }
}

pub fn control_event_details(request: &ControlRequest, origin: ControlOrigin) -> EventDetails {
    let (file_index, file_path) = match request.priority_target() {
        Some(ControlPriorityTarget::FileIndex(index)) => (Some(*index), None),
        Some(ControlPriorityTarget::FilePath(path)) => (None, Some(path.clone())),
        None => (None, None),
    };

    EventDetails::Control {
        origin,
        action: request.action_name().to_string(),
        target_info_hash_hex: request.target_info_hash_hex().map(str::to_string),
        file_index,
        file_path,
        priority: request
            .priority_value()
            .map(|priority| format!("{:?}", priority)),
    }
}

pub fn load_torrent_file_list_for_settings(
    torrent_settings: &TorrentSettings,
) -> Result<Vec<(Vec<String>, u64)>, String> {
    if let Some(metadata_files) = load_torrent_file_list_from_metadata(torrent_settings)? {
        return Ok(metadata_files);
    }

    if torrent_settings.torrent_or_magnet.starts_with("magnet:") {
        return Err(
            "This torrent does not have a persisted .torrent source for file path lookup"
                .to_string(),
        );
    }

    let bytes = fs::read(&torrent_settings.torrent_or_magnet).map_err(|error| {
        format!(
            "Failed to read torrent metadata from '{}': {}",
            torrent_settings.torrent_or_magnet, error
        )
    })?;
    let torrent = from_bytes(&bytes).map_err(|error| {
        format!(
            "Failed to parse torrent metadata from '{}': {:?}",
            torrent_settings.torrent_or_magnet, error
        )
    })?;
    Ok(torrent.file_list())
}

fn load_torrent_file_list_from_metadata(
    torrent_settings: &TorrentSettings,
) -> Result<Option<Vec<(Vec<String>, u64)>>, String> {
    let Some(info_hash) = info_hash_from_torrent_source(&torrent_settings.torrent_or_magnet) else {
        return Ok(None);
    };
    let info_hash_hex = hex::encode(info_hash);
    let metadata = match load_torrent_metadata() {
        Ok(metadata) => metadata,
        Err(_) => return Ok(None),
    };
    let Some(entry) = metadata
        .torrents
        .iter()
        .find(|entry| entry.info_hash_hex == info_hash_hex)
    else {
        return Ok(None);
    };
    if entry.files.is_empty() {
        return Ok(None);
    }
    Ok(Some(file_list_from_metadata_entry(entry)))
}

fn file_list_from_metadata_entry(entry: &TorrentMetadataEntry) -> Vec<(Vec<String>, u64)> {
    entry
        .files
        .iter()
        .map(|file| {
            (
                file.relative_path
                    .split('/')
                    .filter(|segment| !segment.is_empty())
                    .map(|segment| segment.to_string())
                    .collect(),
                file.length,
            )
        })
        .collect()
}

pub fn file_priorities_to_map(
    values: &[ControlFilePriorityOverride],
) -> HashMap<usize, FilePriority> {
    values
        .iter()
        .filter(|value| !matches!(value.priority, FilePriority::Normal))
        .map(|value| (value.file_index, value.priority))
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum ControlExecutionPlan {
    StatusNow,
    StatusFollowStart {
        interval_secs: u64,
    },
    StatusFollowStop,
    ApplySettings {
        next_settings: Settings,
        success_message: String,
    },
    AddTorrentFile {
        source_path: PathBuf,
        download_path: Option<PathBuf>,
        container_name: Option<String>,
        file_priorities: HashMap<usize, FilePriority>,
    },
    AddMagnet {
        magnet_link: String,
        download_path: Option<PathBuf>,
        container_name: Option<String>,
        file_priorities: HashMap<usize, FilePriority>,
    },
}

pub fn plan_control_request(
    settings: &Settings,
    request: &ControlRequest,
) -> Result<ControlExecutionPlan, String> {
    match request {
        ControlRequest::StatusNow => Ok(ControlExecutionPlan::StatusNow),
        ControlRequest::StatusFollowStart { interval_secs } => {
            Ok(ControlExecutionPlan::StatusFollowStart {
                interval_secs: (*interval_secs).max(1),
            })
        }
        ControlRequest::StatusFollowStop => Ok(ControlExecutionPlan::StatusFollowStop),
        ControlRequest::Pause { info_hash_hex } => {
            let info_hash = decode_info_hash(info_hash_hex)?;
            let Some(index) = find_torrent_settings_index_by_info_hash(settings, &info_hash) else {
                return Err(format!("Torrent '{}' was not found", info_hash_hex));
            };
            let mut next_settings = settings.clone();
            next_settings.torrents[index].torrent_control_state =
                crate::app::TorrentControlState::Paused;
            Ok(ControlExecutionPlan::ApplySettings {
                next_settings,
                success_message: format!("Paused torrent '{}'", info_hash_hex),
            })
        }
        ControlRequest::Resume { info_hash_hex } => {
            let info_hash = decode_info_hash(info_hash_hex)?;
            let Some(index) = find_torrent_settings_index_by_info_hash(settings, &info_hash) else {
                return Err(format!("Torrent '{}' was not found", info_hash_hex));
            };
            let mut next_settings = settings.clone();
            next_settings.torrents[index].torrent_control_state =
                crate::app::TorrentControlState::Running;
            Ok(ControlExecutionPlan::ApplySettings {
                next_settings,
                success_message: format!("Resumed torrent '{}'", info_hash_hex),
            })
        }
        ControlRequest::Delete {
            info_hash_hex,
            delete_files,
        } => {
            let info_hash = decode_info_hash(info_hash_hex)?;
            let Some(index) = find_torrent_settings_index_by_info_hash(settings, &info_hash) else {
                return Err(format!("Torrent '{}' was not found", info_hash_hex));
            };
            let mut next_settings = settings.clone();
            if *delete_files {
                next_settings.torrents[index].torrent_control_state =
                    crate::app::TorrentControlState::Deleting;
                next_settings.torrents[index].delete_files = true;
            } else {
                next_settings.torrents.retain(|torrent| {
                    info_hash_from_torrent_source(&torrent.torrent_or_magnet).as_deref()
                        != Some(info_hash.as_slice())
                });
            }
            Ok(ControlExecutionPlan::ApplySettings {
                next_settings,
                success_message: if *delete_files {
                    format!("Queued purge for torrent '{}'", info_hash_hex)
                } else {
                    format!("Removed torrent '{}'", info_hash_hex)
                },
            })
        }
        ControlRequest::SetFilePriority {
            info_hash_hex,
            target,
            priority,
        } => {
            let info_hash = decode_info_hash(info_hash_hex)?;
            let Some(index) = find_torrent_settings_index_by_info_hash(settings, &info_hash) else {
                return Err(format!("Torrent '{}' was not found", info_hash_hex));
            };
            let mut next_settings = settings.clone();
            let torrent_settings = next_settings
                .torrents
                .get(index)
                .cloned()
                .ok_or_else(|| format!("Torrent '{}' was not found", info_hash_hex))?;
            let file_index = resolve_priority_file_index(&torrent_settings, target)?;
            if matches!(priority, FilePriority::Normal) {
                next_settings.torrents[index]
                    .file_priorities
                    .remove(&file_index);
            } else {
                next_settings.torrents[index]
                    .file_priorities
                    .insert(file_index, *priority);
            }
            Ok(ControlExecutionPlan::ApplySettings {
                next_settings,
                success_message: format!(
                    "Set file priority for torrent '{}' at index {} to {:?}",
                    info_hash_hex, file_index, priority
                ),
            })
        }
        ControlRequest::AddTorrentFile {
            source_path,
            download_path,
            container_name,
            file_priorities,
        } => Ok(ControlExecutionPlan::AddTorrentFile {
            source_path: source_path.clone(),
            download_path: download_path.clone(),
            container_name: container_name.clone(),
            file_priorities: file_priorities_to_map(file_priorities),
        }),
        ControlRequest::AddMagnet {
            magnet_link,
            download_path,
            container_name,
            file_priorities,
        } => Ok(ControlExecutionPlan::AddMagnet {
            magnet_link: magnet_link.clone(),
            download_path: download_path.clone(),
            container_name: container_name.clone(),
            file_priorities: file_priorities_to_map(file_priorities),
        }),
    }
}

pub fn resolve_priority_file_index(
    torrent_settings: &TorrentSettings,
    target: &ControlPriorityTarget,
) -> Result<usize, String> {
    let file_list = load_torrent_file_list_for_settings(torrent_settings)?;
    match target {
        ControlPriorityTarget::FileIndex(index) => {
            if *index < file_list.len() {
                Ok(*index)
            } else {
                Err(format!(
                    "File index {} is out of range for torrent '{}' ({} files)",
                    index,
                    torrent_settings.name,
                    file_list.len()
                ))
            }
        }
        ControlPriorityTarget::FilePath(path) => {
            let normalized_target = path.replace('\\', "/");
            file_list
                .into_iter()
                .enumerate()
                .find_map(|(index, (parts, _))| {
                    (parts.join("/") == normalized_target).then_some(index)
                })
                .ok_or_else(|| {
                    format!(
                        "No file matching '{}' was found in torrent '{}'",
                        path, torrent_settings.name
                    )
                })
        }
    }
}

pub fn apply_offline_control_request(
    settings: &mut Settings,
    request: &ControlRequest,
) -> Result<String, String> {
    match plan_control_request(settings, request)? {
        ControlExecutionPlan::StatusNow
        | ControlExecutionPlan::StatusFollowStart { .. }
        | ControlExecutionPlan::StatusFollowStop => {
            Err("Status commands require a running superseedr instance".to_string())
        }
        ControlExecutionPlan::ApplySettings {
            next_settings,
            success_message,
        } => {
            *settings = next_settings;
            Ok(success_message)
        }
        ControlExecutionPlan::AddTorrentFile {
            source_path,
            download_path,
            container_name,
            file_priorities,
        } => {
            let name = source_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Queued Torrent")
                .to_string();
            settings.torrents.push(TorrentSettings {
                torrent_or_magnet: source_path.to_string_lossy().to_string(),
                name,
                download_path,
                container_name,
                file_priorities,
                ..TorrentSettings::default()
            });
            Ok(format!(
                "Queued torrent file '{}' for the next runtime",
                source_path.display()
            ))
        }
        ControlExecutionPlan::AddMagnet {
            magnet_link,
            download_path,
            container_name,
            file_priorities,
        } => {
            settings.torrents.push(TorrentSettings {
                torrent_or_magnet: magnet_link,
                name: "Queued Magnet".to_string(),
                download_path,
                container_name,
                file_priorities,
                ..TorrentSettings::default()
            });
            Ok("Queued magnet for the next runtime".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_offline_control_request, find_torrent_settings_index_by_info_hash,
        plan_control_request, ControlExecutionPlan,
    };
    use crate::config::{Settings, TorrentSettings};
    use crate::integrations::control::{ControlPriorityTarget, ControlRequest};

    #[test]
    fn offline_hybrid_magnet_lookup_prefers_btih_identity() {
        let magnet = concat!(
            "magnet:?xt=urn:btih:1111111111111111111111111111111111111111",
            "&xt=urn:btmh:1220aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        let settings = Settings {
            torrents: vec![TorrentSettings {
                torrent_or_magnet: magnet.to_string(),
                name: "Sample Hybrid".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        assert_eq!(
            find_torrent_settings_index_by_info_hash(&settings, &[0x11; 20]),
            Some(0)
        );
    }

    #[test]
    fn offline_delete_targets_hybrid_magnet_by_btih() {
        let magnet = concat!(
            "magnet:?xt=urn:btih:1111111111111111111111111111111111111111",
            "&xt=urn:btmh:1220aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        let mut settings = Settings {
            torrents: vec![TorrentSettings {
                torrent_or_magnet: magnet.to_string(),
                name: "Sample Hybrid".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let result = apply_offline_control_request(
            &mut settings,
            &ControlRequest::Delete {
                info_hash_hex: "1111111111111111111111111111111111111111".to_string(),
                delete_files: false,
            },
        );

        assert!(result.is_ok());
        assert!(settings.torrents.is_empty());
    }

    #[test]
    fn priority_file_path_resolution_still_requires_torrent_metadata() {
        let mut settings = Settings {
            torrents: vec![TorrentSettings {
                torrent_or_magnet: "magnet:?xt=urn:btih:1111111111111111111111111111111111111111"
                    .to_string(),
                name: "Magnet".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let result = apply_offline_control_request(
            &mut settings,
            &ControlRequest::SetFilePriority {
                info_hash_hex: "1111111111111111111111111111111111111111".to_string(),
                target: ControlPriorityTarget::FilePath("folder/item.bin".to_string()),
                priority: crate::app::FilePriority::High,
            },
        );

        assert!(result.is_err());
    }

    #[test]
    fn control_plan_and_offline_apply_share_pause_and_purge_mutations() {
        let mut settings = Settings {
            torrents: vec![TorrentSettings {
                torrent_or_magnet: "magnet:?xt=urn:btih:1111111111111111111111111111111111111111"
                    .to_string(),
                name: "Sample Node".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let pause = ControlRequest::Pause {
            info_hash_hex: "1111111111111111111111111111111111111111".to_string(),
        };
        match plan_control_request(&settings, &pause).expect("plan pause") {
            ControlExecutionPlan::ApplySettings { next_settings, .. } => {
                assert_eq!(
                    next_settings.torrents[0].torrent_control_state,
                    crate::app::TorrentControlState::Paused
                );
            }
            other => panic!("unexpected plan: {:?}", other),
        }

        apply_offline_control_request(&mut settings, &pause).expect("apply pause");
        assert_eq!(
            settings.torrents[0].torrent_control_state,
            crate::app::TorrentControlState::Paused
        );

        let purge = ControlRequest::Delete {
            info_hash_hex: "1111111111111111111111111111111111111111".to_string(),
            delete_files: true,
        };
        match plan_control_request(&settings, &purge).expect("plan purge") {
            ControlExecutionPlan::ApplySettings { next_settings, .. } => {
                assert_eq!(
                    next_settings.torrents[0].torrent_control_state,
                    crate::app::TorrentControlState::Deleting
                );
                assert!(next_settings.torrents[0].delete_files);
            }
            other => panic!("unexpected plan: {:?}", other),
        }
    }
}
