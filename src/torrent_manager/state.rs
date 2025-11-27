// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use tracing::{event, Level};

use crate::command::TorrentCommand;
use crate::networking::BlockInfo;
use crate::torrent_manager::ManagerEvent;

use std::time::Duration;
use std::time::Instant;

use tokio::sync::mpsc::Sender;
use tokio::sync::Semaphore;

use std::mem::Discriminant;
use std::sync::Arc;

use crate::torrent_file::Torrent;
use crate::torrent_manager::block_manager::{BlockManager, BlockAddress, BlockResult, BlockDecision, BLOCK_SIZE};
use std::collections::{HashMap, HashSet};

const MAX_TIMEOUT_COUNT: u32 = 10;
const SMOOTHING_PERIOD_MS: f64 = 5000.0;
const PEER_UPLOAD_IN_FLIGHT_LIMIT: usize = 4;
const MAX_BLOCK_SIZE: u32 = 131_072;
const UPLOAD_SLOTS_DEFAULT: usize = 4;
const DEFAULT_ANNOUNCE_INTERVAL_SECS: u64 = 60;
const TARGET_BYTES_IN_FLIGHT: u64 = 320 * 1024; 

pub type PeerAddr = (String, u16);

#[derive(Debug, Clone)]
pub enum Action {
    TorrentManagerInit {
        is_paused: bool,
        announce_immediately: bool,
    },
    Tick {
        dt_ms: u64,
    },
    RecalculateChokes {
        random_seed: u64,
    },
    CheckCompletion,
    AssignWork {
        peer_id: String,
    },
    ConnectToWebSeeds,
    RegisterPeer {
        peer_id: String,
        tx: Sender<TorrentCommand>,
    },
    PeerSuccessfullyConnected {
        peer_id: String,
    },
    PeerDisconnected {
        peer_id: String,
    },
    UpdatePeerId {
        peer_addr: String,
        new_id: Vec<u8>,
    },
    PeerBitfieldReceived {
        peer_id: String,
        bitfield: Vec<u8>,
    },
    PeerChoked {
        peer_id: String,
    },
    PeerUnchoked {
        peer_id: String,
    },
    PeerInterested {
        peer_id: String,
    },
    PeerHavePiece {
        peer_id: String,
        piece_index: u32,
    },
    IncomingBlock {
        peer_id: String,
        piece_index: u32,
        block_offset: u32,
        data: Vec<u8>,
    },
    PieceVerified {
        peer_id: String,
        piece_index: u32,
        valid: bool,
        data: Vec<u8>,
    },
    BlockVerified {
        peer_id: String,
        block_addr: BlockAddress,
        result: Result<Vec<u8>, ()>,
    },
    PieceWrittenToDisk {
        peer_id: String,
        piece_index: u32,
    },
    PieceWriteFailed {
        piece_index: u32,
    },
    RequestUpload {
        peer_id: String,
        piece_index: u32,
        block_offset: u32,
        length: u32,
    },
    TrackerResponse {
        url: String,
        peers: Vec<PeerAddr>,
        interval: u64,
        min_interval: Option<u64>,
    },
    TrackerError {
        url: String,
    },
    PeerConnectionFailed {
        peer_addr: String,
    },
    MetadataReceived {
        torrent: Box<Torrent>,
        metadata_length: i64,
    },
    ValidationComplete {
        completed_pieces: Vec<u32>,
    },

    BlockSentToPeer {
        peer_id: String,
        byte_count: u64,
    },

    CancelUpload {
        peer_id: String,
        piece_index: u32,
        block_offset: u32,
        length: u32,
    },

    Cleanup,
    Pause,
    Resume,
    Delete,
    UpdateListenPort,
    ValidationProgress {
        count: u32,
    },
    Shutdown,
    FatalError,
}

#[derive(Debug)]
#[must_use]
pub enum Effect {
    DoNothing,
    EmitMetrics {
        bytes_dl: u64,
        bytes_ul: u64,
    },
    EmitManagerEvent(ManagerEvent),
    SendToPeer {
        peer_id: String,
        cmd: Box<TorrentCommand>,
    },
    DisconnectPeer {
        peer_id: String,
    },
    AnnounceCompleted {
        url: String,
    },
    VerifyPiece {
        peer_id: String,
        piece_index: u32,
        data: Vec<u8>,
    },
    VerifyBlock {
        peer_id: String,
        block_addr: BlockAddress,
        data: Vec<u8>,
        root_hash: [u8; 32],
        proof: Vec<[u8; 32]>,
    },
    WriteToDisk {
        peer_id: String,
        piece_index: u32,
        block_offset: u32,
        data: Vec<u8>,
    },
    ReadFromDisk {
        peer_id: String,
        block_info: BlockInfo,
    },
    BroadcastHave {
        piece_index: u32,
    },
    ConnectToPeer {
        ip: String,
        port: u16,
    },
    StartWebSeed {
        url: String,
    },
    InitializeStorage,
    StartValidation,
    AnnounceToTracker {
        url: String,
    },
    ConnectToPeersFromTrackers,
    AbortUpload {
        peer_id: String,
        block_info: BlockInfo,
    },
    ClearAllUploads,
    DeleteFiles,
    TriggerDhtSearch,
    PrepareShutdown {
        tracker_urls: Vec<String>,
        left: usize,
        uploaded: usize,
        downloaded: usize,
    },
}

#[derive(Debug, Clone)]
pub struct TrackerState {
    pub next_announce_time: Instant,
    pub leeching_interval: Option<Duration>,
    pub seeding_interval: Option<Duration>,
}

#[derive(Clone, Debug, Default)]
pub enum TorrentActivity {
    #[default]
    Initializing,
    Paused,
    ConnectingToPeers,
    DownloadingPiece(u32),
    SendingPiece(u32),
    VerifyingPiece(u32),
    AnnouncingToTracker,

    #[cfg(feature = "dht")]
    SearchingDht,
}

#[derive(PartialEq, Debug, Default, Clone)]
pub enum TorrentStatus {
    #[default]
    AwaitingMetadata,
    Validating,
    Standard,
    Endgame,
    Done,
}

#[derive(PartialEq, Debug, Clone)]
pub enum ChokeStatus {
    Choke,
    Unchoke,
}

#[derive(Debug, Clone)]
pub struct TorrentState {
    pub info_hash: Vec<u8>,
    pub torrent: Option<Torrent>,
    pub torrent_metadata_length: Option<i64>,
    pub is_paused: bool,
    pub torrent_status: TorrentStatus,
    pub torrent_validation_status: bool,
    pub last_activity: TorrentActivity,
    pub has_made_first_connection: bool,
    pub session_total_uploaded: u64,
    pub session_total_downloaded: u64,
    pub bytes_downloaded_in_interval: u64,
    pub bytes_uploaded_in_interval: u64,
    pub total_dl_prev_avg_ema: f64,
    pub total_ul_prev_avg_ema: f64,
    pub number_of_successfully_connected_peers: usize,
    pub peers: HashMap<String, PeerState>,
    // UPDATED: Replaced PieceManager with BlockManager
    pub block_manager: BlockManager,
    pub trackers: HashMap<String, TrackerState>,
    pub timed_out_peers: HashMap<String, (u32, Instant)>,
    pub last_known_peers: HashSet<String>,
    pub optimistic_unchoke_timer: Option<Instant>,
    pub validation_pieces_found: u32,
    pub now: Instant,
    pub has_started_announce_sent: bool,
}
impl Default for TorrentState {
    fn default() -> Self {
        Self {
            info_hash: Vec::new(),
            torrent: None,
            torrent_metadata_length: None,
            is_paused: false,
            torrent_status: TorrentStatus::default(),
            torrent_validation_status: false,
            last_activity: TorrentActivity::default(),
            has_made_first_connection: false,
            session_total_uploaded: 0,
            session_total_downloaded: 0,
            bytes_downloaded_in_interval: 0,
            bytes_uploaded_in_interval: 0,
            total_dl_prev_avg_ema: 0.0,
            total_ul_prev_avg_ema: 0.0,
            number_of_successfully_connected_peers: 0,
            peers: HashMap::new(),
            block_manager: BlockManager::new(), // UPDATED
            trackers: HashMap::new(),
            timed_out_peers: HashMap::new(),
            last_known_peers: HashSet::new(),
            optimistic_unchoke_timer: None,
            validation_pieces_found: 0,
            now: Instant::now(),
            has_started_announce_sent: false,
        }
    }
}

impl TorrentState {
    pub fn new(
        info_hash: Vec<u8>,
        torrent: Option<Torrent>,
        torrent_metadata_length: Option<i64>,
        _piece_manager_compat: (), // Deprecated argument placeholder
        trackers: HashMap<String, TrackerState>,
        torrent_validation_status: bool,
    ) -> Self {
        let torrent_status = if torrent.is_some() {
            TorrentStatus::Validating
        } else {
            TorrentStatus::AwaitingMetadata
        };

        // NEW: Initialize BlockManager if we have torrent info
        let mut block_manager = BlockManager::new();
        if let Some(ref t) = torrent {
             let piece_len = t.info.piece_length as u32;
             let total_len: u64 = if !t.info.files.is_empty() {
                t.info.files.iter().map(|f| f.length as u64).sum()
             } else {
                t.info.length as u64
             };
             block_manager.set_geometry(
                 piece_len, 
                 total_len, 
                    torrent.clone().unwrap().info.pieces.chunks(20)
                    .map(|chunk| {
                        let mut h = [0; 20];
                        h.copy_from_slice(chunk);
                        h
                    })
                    .collect(),
                 HashMap::new(), 
                 torrent_validation_status
             );
        }

        Self {
            info_hash,
            torrent,
            torrent_metadata_length,
            torrent_status,
            block_manager,
            trackers,
            torrent_validation_status,
            optimistic_unchoke_timer: Some(
                Instant::now()
                    .checked_sub(Duration::from_secs(31))
                    .unwrap_or(Instant::now()),
            ),
            now: Instant::now(),
            ..Default::default()
        }
    }

    // Helper to determine piece size based on torrent metadata
    fn get_piece_size(&self, piece_index: u32) -> usize {
        if let Some(torrent) = &self.torrent {
            let piece_len = torrent.info.piece_length as u64;
            let total_len: u64 = if !torrent.info.files.is_empty() {
                torrent.info.files.iter().map(|f| f.length as u64).sum()
            } else {
                torrent.info.length as u64
            };

            let offset = piece_index as u64 * piece_len;
            let remaining = total_len.saturating_sub(offset);
            std::cmp::min(piece_len, remaining) as usize
        } else {
            0
        }
    }

    pub fn update(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::TorrentManagerInit {
                is_paused,
                announce_immediately,
            } => {
                let mut effects = Vec::new();

                self.is_paused = is_paused;
                if self.is_paused {
                    return effects;
                }

                effects.extend(self.update(Action::ConnectToWebSeeds));

                let should_announce =
                    announce_immediately || self.torrent_status == TorrentStatus::AwaitingMetadata;
                if should_announce {
                    for url in self.trackers.keys() {
                        effects.push(Effect::AnnounceToTracker { url: url.clone() });
                    }
                    self.has_started_announce_sent = true;
                }

                effects
            }
            Action::Tick { dt_ms } => {
                self.now += Duration::from_millis(dt_ms);
                let scaling_factor = if dt_ms > 0 {
                    1000.0 / dt_ms as f64
                } else {
                    1.0
                };
                let dt = dt_ms as f64;
                let alpha = 1.0 - (-dt / SMOOTHING_PERIOD_MS).exp();

                let inst_total_dl_speed =
                    (self.bytes_downloaded_in_interval as f64 * 8.0) * scaling_factor;
                let inst_total_ul_speed =
                    (self.bytes_uploaded_in_interval as f64 * 8.0) * scaling_factor;

                let dl_tick = self.bytes_downloaded_in_interval;
                let ul_tick = self.bytes_uploaded_in_interval;

                self.bytes_downloaded_in_interval = 0;
                self.bytes_uploaded_in_interval = 0;

                self.total_dl_prev_avg_ema =
                    (inst_total_dl_speed * alpha) + (self.total_dl_prev_avg_ema * (1.0 - alpha));
                self.total_ul_prev_avg_ema =
                    (inst_total_ul_speed * alpha) + (self.total_ul_prev_avg_ema * (1.0 - alpha));

                for peer in self.peers.values_mut() {
                    let inst_dl_speed =
                        (peer.bytes_downloaded_in_tick as f64 * 8.0) * scaling_factor;
                    let inst_ul_speed = (peer.bytes_uploaded_in_tick as f64 * 8.0) * scaling_factor;

                    peer.prev_avg_dl_ema =
                        (inst_dl_speed * alpha) + (peer.prev_avg_dl_ema * (1.0 - alpha));
                    peer.download_speed_bps = peer.prev_avg_dl_ema as u64;

                    peer.prev_avg_ul_ema =
                        (inst_ul_speed * alpha) + (peer.prev_avg_ul_ema * (1.0 - alpha));
                    peer.upload_speed_bps = peer.prev_avg_ul_ema as u64;

                    peer.bytes_downloaded_in_tick = 0;
                    peer.bytes_uploaded_in_tick = 0;
                }

                let mut effects = vec![Effect::EmitMetrics {
                    bytes_dl: dl_tick,
                    bytes_ul: ul_tick,
                }];

                if self.torrent_status == TorrentStatus::Validating || self.is_paused {
                    return effects;
                }

                for (url, tracker) in self.trackers.iter_mut() {
                    if self.now >= tracker.next_announce_time {
                        self.last_activity = TorrentActivity::AnnouncingToTracker;
                        let interval = if self.torrent_status == TorrentStatus::Done {
                            tracker
                                .seeding_interval
                                .unwrap_or(Duration::from_secs(DEFAULT_ANNOUNCE_INTERVAL_SECS))
                        } else {
                            tracker
                                .leeching_interval
                                .unwrap_or(Duration::from_secs(DEFAULT_ANNOUNCE_INTERVAL_SECS))
                        };
                        tracker.next_announce_time = self.now + interval;
                        effects.push(Effect::AnnounceToTracker { url: url.clone() });
                    }
                }

                effects
            }

            Action::RecalculateChokes { random_seed } => {
                let mut effects = Vec::new();

                let mut interested_peers: Vec<&mut PeerState> = self
                    .peers
                    .values_mut()
                    .filter(|p| p.peer_is_interested_in_us)
                    .collect();

                if self.torrent_status == TorrentStatus::Done {
                    interested_peers
                        .sort_by(|a, b| b.bytes_uploaded_to_peer.cmp(&a.bytes_uploaded_to_peer));
                } else {
                    interested_peers.sort_by(|a, b| {
                        b.bytes_downloaded_from_peer
                            .cmp(&a.bytes_downloaded_from_peer)
                    });
                }

                let mut unchoke_candidates: HashSet<String> = interested_peers
                    .iter()
                    .take(UPLOAD_SLOTS_DEFAULT)
                    .map(|p| p.ip_port.clone())
                    .collect();

                if self.optimistic_unchoke_timer.is_some_and(|t| {
                    self.now.saturating_duration_since(t) > Duration::from_secs(30)
                }) {
                    let optimistic_candidates: Vec<&mut PeerState> = interested_peers
                        .into_iter()
                        .filter(|p| !unchoke_candidates.contains(&p.ip_port))
                        .collect();

                    if !optimistic_candidates.is_empty() {
                        let idx = (random_seed as usize) % optimistic_candidates.len();
                        let chosen_id = optimistic_candidates[idx].ip_port.clone();
                        unchoke_candidates.insert(chosen_id);
                    }

                    self.optimistic_unchoke_timer = Some(self.now);
                }

                for peer in self.peers.values_mut() {
                    if unchoke_candidates.contains(&peer.ip_port) {
                        if peer.am_choking == ChokeStatus::Choke {
                            peer.am_choking = ChokeStatus::Unchoke;
                            effects.push(Effect::SendToPeer {
                                peer_id: peer.ip_port.clone(),
                                cmd: Box::new(TorrentCommand::PeerUnchoke),
                            });
                        }
                    } else if peer.am_choking == ChokeStatus::Unchoke {
                        peer.am_choking = ChokeStatus::Choke;
                        effects.push(Effect::SendToPeer {
                            peer_id: peer.ip_port.clone(),
                            cmd: Box::new(TorrentCommand::PeerChoke),
                        });
                    }

                    peer.bytes_downloaded_from_peer = 0;
                    peer.bytes_uploaded_to_peer = 0;
                }

                effects
            }

            Action::CheckCompletion => {
                if self.torrent_status == TorrentStatus::AwaitingMetadata
                    || self.torrent_status == TorrentStatus::Validating
                    || self.torrent_status == TorrentStatus::Done
                {
                    return vec![Effect::DoNothing];
                }

                let mut all_done = false;

                // 1. ROBUST PIECE-LEVEL CHECK (Primary for V1 torrents)
                if let Some(torrent) = &self.torrent {
                    let total_pieces = torrent.info.pieces.len() / 20;
                    
                    let mut completed_piece_count = 0;
                    // Check completion by piece index, relying on BlockManager::is_piece_complete
                    for i in 0..total_pieces {
                        if self.block_manager.is_piece_complete(i as u32) {
                            completed_piece_count += 1;
                        }
                    }

                    if completed_piece_count == total_pieces {
                         all_done = true;
                    }
                }
                
                // 2. BLOCK-LEVEL FALLBACK (Original logic for V2/Hybrid torrents)
                if !all_done {
                    let bitfield = &self.block_manager.block_bitfield;
                    all_done = !bitfield.is_empty() && bitfield.iter().all(|&b| b);
                }


                if all_done {
                    let mut effects = Vec::new();
                    self.torrent_status = TorrentStatus::Done; // <-- Transition must happen here

                    // Announce and disconnect peers... (rest of the logic)
                    for (url, tracker) in self.trackers.iter_mut() {
                        tracker.next_announce_time = self.now;
                        effects.push(Effect::AnnounceCompleted { url: url.clone() });
                    }
                    // ... (rest of effects)
                    for peer in self.peers.values_mut() {
                        if peer.am_interested {
                            peer.am_interested = false;
                            effects.push(Effect::SendToPeer {
                                peer_id: peer.ip_port.clone(),
                                cmd: Box::new(TorrentCommand::NotInterested),
                            });
                        }
                    }
                    return effects;
                }

                vec![Effect::DoNothing]
            }

            Action::AssignWork { peer_id } => {
                if self.torrent_status == TorrentStatus::Validating
                    || self.torrent_status == TorrentStatus::AwaitingMetadata
                    || self.is_paused
                {
                    return vec![Effect::DoNothing];
                }

                if self.block_manager.block_bitfield.iter().all(|&b| b) {
                    return vec![Effect::DoNothing];
                }

                let _torrent = match &self.torrent {
                    Some(t) => t,
                    None => return vec![Effect::DoNothing],
                };

                let mut effects = Vec::new();

                if let Some(peer) = self.peers.get_mut(&peer_id) {
                    // UPDATED: Check block manager for completion
                    let am_interested = self.block_manager.piece_hashes_v1.iter().enumerate().any(|(i, _)| {
                        let peer_has = peer.bitfield.get(i).copied().unwrap_or(false);
                        let we_have = self.block_manager.is_piece_complete(i as u32);
                        peer_has && !we_have
                    });

                    if am_interested && !peer.am_interested {
                        peer.am_interested = true;
                        effects.push(Effect::SendToPeer {
                            peer_id: peer_id.clone(),
                            cmd: Box::new(TorrentCommand::ClientInterested),
                        });
                    } else if !am_interested && peer.am_interested {
                        peer.am_interested = false;
                        effects.push(Effect::SendToPeer {
                            peer_id: peer_id.clone(),
                            cmd: Box::new(TorrentCommand::NotInterested),
                        });
                    }

                    if !peer.am_interested || peer.peer_choking == ChokeStatus::Choke {
                        return effects;
                    }

                    // UPDATED: Adaptive Pipelining

                    let current_bytes_in_flight: u64 = peer.pending_requests.iter()
                        .map(|block| block.length as u64)
                        .sum();

                    if current_bytes_in_flight < TARGET_BYTES_IN_FLIGHT {
                        let bytes_needed = TARGET_BYTES_IN_FLIGHT - current_bytes_in_flight;
                        let blocks_to_request = (bytes_needed + (BLOCK_SIZE as u64) - 1) 
                            / (BLOCK_SIZE as u64);

                        // UPDATED: Use BlockManager picker
                        let needed_pieces = self.block_manager.get_rarest_pieces();
                        let is_endgame = self.torrent_status == TorrentStatus::Endgame;
                        let new_blocks = self.block_manager.pick_blocks_for_peer(
                            &peer.bitfield,
                            blocks_to_request as usize,
                            &needed_pieces,
                            is_endgame,
                        );

                        for block in new_blocks {
                            peer.pending_requests.insert(block);
                            let global_idx = self.block_manager.flatten_address(block);
                            self.block_manager.mark_pending(global_idx);


                           effects.push(Effect::SendToPeer {
                                peer_id: peer_id.clone(),
                                cmd: Box::new(TorrentCommand::RequestDownload(
                                    block.piece_index,
                                    block.byte_offset as i64,
                                    block.length as i64,
                                )),
                            });
                        }
                    }
                }
                effects
            }

            Action::ConnectToWebSeeds => {
                let mut effects = Vec::new();
                if let Some(torrent) = &self.torrent {
                    if let Some(urls) = &torrent.url_list {
                        for url in urls {
                            effects.push(Effect::StartWebSeed { url: url.clone() });
                        }
                    }
                }
                effects
            }

            Action::RegisterPeer { peer_id, tx } => {
                if !self.peers.contains_key(&peer_id) {
                    let mut peer_state = PeerState::new(peer_id.clone(), tx, self.now);
                    peer_state.peer_id = peer_id.as_bytes().to_vec();
                    self.peers.insert(peer_id, peer_state);
                }
                vec![Effect::DoNothing]
            }

            Action::PeerSuccessfullyConnected { peer_id } => {
                self.timed_out_peers.remove(&peer_id);
                if !self.has_made_first_connection {
                    self.has_made_first_connection = true;
                }
                self.number_of_successfully_connected_peers = self.peers.len();
                vec![Effect::EmitManagerEvent(ManagerEvent::PeerConnected {
                    info_hash: self.info_hash.clone(),
                })]
            }

            Action::PeerDisconnected { peer_id } => {
                let mut effects = Vec::new();
                if let Some(removed_peer) = self.peers.remove(&peer_id) {
                    // UPDATED: Release blocks
                    self.block_manager.release_pending_blocks_for_peer(&removed_peer.pending_requests);
                    self.block_manager
                        .update_rarity(self.peers.values().map(|p| &p.bitfield));

                    self.number_of_successfully_connected_peers = self.peers.len();

                    effects.push(Effect::DisconnectPeer {
                        peer_id: peer_id.clone(),
                    });
                    effects.push(Effect::EmitManagerEvent(ManagerEvent::PeerDisconnected {
                        info_hash: self.info_hash.clone(),
                    }));
                }
                effects
            }

            Action::UpdatePeerId { peer_addr, new_id } => {
                if let Some(peer) = self.peers.get_mut(&peer_addr) {
                    peer.peer_id = new_id;
                }
                vec![Effect::DoNothing]
            }

            Action::PeerBitfieldReceived { peer_id, bitfield } => {
                let mut effects = Vec::new();

                if let Some(peer) = self.peers.get_mut(&peer_id) {
                    if !peer.bitfield.is_empty() && peer.bitfield.iter().any(|&b| b) {
                        effects.push(Effect::DisconnectPeer {
                            peer_id: peer_id.clone(),
                        });
                        return effects;
                    }

                    peer.bitfield = bitfield
                        .iter()
                        .flat_map(|&byte| (0..8).map(move |i| (byte >> (7 - i)) & 1 == 1))
                        .collect();

                    if let Some(torrent) = &self.torrent {
                        let total_pieces = torrent.info.pieces.len() / 20;
                        peer.bitfield.resize(total_pieces, false);
                    }
                }

                // UPDATED: Use BlockManager
                self.block_manager
                    .update_rarity(self.peers.values().map(|p| &p.bitfield));
                self.update(Action::AssignWork { peer_id })
            }

            Action::PeerChoked { peer_id } => {
                if let Some(peer) = self.peers.get_mut(&peer_id) {
                    peer.peer_choking = ChokeStatus::Choke;
                    // UPDATED: Release blocks
                    self.block_manager.release_pending_blocks_for_peer(&peer.pending_requests);
                    peer.pending_requests.clear();
                }
                vec![Effect::DoNothing]
            }

            Action::PeerUnchoked { peer_id } => {
                if let Some(peer) = self.peers.get_mut(&peer_id) {
                    peer.peer_choking = ChokeStatus::Unchoke;
                }
                self.update(Action::AssignWork { peer_id })
            }

            Action::PeerInterested { peer_id } => {
                if let Some(peer) = self.peers.get_mut(&peer_id) {
                    peer.peer_is_interested_in_us = true;
                }
                vec![Effect::DoNothing]
            }

            Action::PeerHavePiece {
                peer_id,
                piece_index,
            } => {
                if let Some(peer) = self.peers.get_mut(&peer_id) {
                    if (piece_index as usize) < peer.bitfield.len() {
                        peer.bitfield[piece_index as usize] = true;
                    }
                }
                // UPDATED: Use BlockManager
                self.block_manager
                    .update_rarity(self.peers.values().map(|p| &p.bitfield));
                self.update(Action::AssignWork { peer_id })
            }

            Action::IncomingBlock {
                peer_id,
                piece_index,
                block_offset,
                data,
            } => {
                if data.len() > MAX_BLOCK_SIZE as usize {
                    return vec![Effect::DisconnectPeer { peer_id }];
                }
                if piece_index as usize >= self.block_manager.total_pieces() {
                    return vec![Effect::DoNothing];
                }
                if self.torrent_status == TorrentStatus::Validating 
                    || self.torrent_status == TorrentStatus::AwaitingMetadata 
                {
                    return vec![Effect::DoNothing];
                }

                let block_addr = match self.block_manager.inflate_address_from_overlay(
                    piece_index, 
                    block_offset, 
                    data.len() as u32
                ) {
                    Some(addr) => addr,
                    None => {
                        return vec![Effect::DisconnectPeer { peer_id }];
                    }
                };

                if let Some(peer) = self.peers.get_mut(&peer_id) {
                    let peer_has_piece = peer.bitfield.get(piece_index as usize).copied().unwrap_or(false);
                    if !peer_has_piece {
                        return vec![Effect::DisconnectPeer { peer_id }];
                    }
                    if !peer.pending_requests.remove(&block_addr) {
                        return vec![Effect::DoNothing];
                    }
                    let len = data.len() as u64;
                    peer.bytes_downloaded_from_peer += len;
                    peer.bytes_downloaded_in_tick += len;
                    peer.total_bytes_downloaded += len;
                } else {
                    return vec![Effect::DoNothing];
                }

                let len = data.len() as u64;
                self.bytes_downloaded_in_interval = self.bytes_downloaded_in_interval.saturating_add(len);
                self.session_total_downloaded = self.session_total_downloaded.saturating_add(len);
                self.last_activity = TorrentActivity::DownloadingPiece(piece_index);

                // UPDATED: Block Decision Router
                let decision = self.block_manager.handle_incoming_block_decision(block_addr);

                match decision {
                    BlockDecision::VerifyV2 { root_hash, proof } => {
                        vec![Effect::VerifyBlock {
                            peer_id,
                            block_addr,
                            data,
                            root_hash,
                            proof,
                        }]
                    },
                    BlockDecision::BufferV1 => {
                        if let Some(full_piece_data) = self.block_manager.handle_v1_block_buffering(block_addr, &data) {
                            self.last_activity = TorrentActivity::VerifyingPiece(piece_index);
                            vec![Effect::VerifyPiece {
                                peer_id,
                                piece_index,
                                data: full_piece_data,
                            }]
                        } else {
                            self.update(Action::AssignWork { peer_id })
                        }
                    },
                    BlockDecision::Duplicate | BlockDecision::Error => {
                        vec![Effect::DoNothing]
                    }
                }
            }

            Action::BlockVerified { peer_id, block_addr, result } => {
                match result {
                    Ok(data) => {
                        let res = self.block_manager.commit_verified_block(block_addr);
                        if res == BlockResult::Accepted {
                            let mut effects = vec![
                                Effect::WriteToDisk {
                                    peer_id: peer_id.clone(),
                                    piece_index: block_addr.piece_index,
                                    block_offset: block_addr.byte_offset,
                                    data,
                                },
                                Effect::EmitManagerEvent(ManagerEvent::BlockReceived { 
                                    info_hash: self.info_hash.clone() 
                                })
                            ];
                            if self.block_manager.is_piece_complete(block_addr.piece_index) {
                                 effects.push(Effect::BroadcastHave { piece_index: block_addr.piece_index });
                                 effects.extend(self.update(Action::CheckCompletion));
                            }
                            effects.extend(self.update(Action::AssignWork { peer_id }));
                            effects
                        } else {
                            vec![Effect::DoNothing]
                        }
                    },
                    Err(_) => vec![Effect::DisconnectPeer { peer_id }]
                }
            }

            Action::PieceVerified {
                peer_id,
                piece_index,
                valid,
                data,
            } => {
                if piece_index as usize >= self.block_manager.total_pieces() {
                    return vec![Effect::DoNothing];
                }
                if self.block_manager.is_piece_complete(piece_index) {
                    return vec![Effect::DoNothing];
                }

                if valid {
                    // UPDATED: Commit V1 piece
                    self.block_manager.commit_v1_piece(piece_index);
                    if let Some(peer) = self.peers.get_mut(&peer_id) {
                         peer.pending_requests.retain(|blk| blk.piece_index != piece_index);
                    }
                    vec![Effect::WriteToDisk {
                        peer_id,
                        piece_index,
                        block_offset: 0, 
                        data,
                    }]
                } else {
                    self.block_manager.reset_v1_buffer(piece_index);
                    vec![Effect::DisconnectPeer { peer_id }]
                }
            }

            Action::PieceWrittenToDisk {
                peer_id,
                piece_index,
            } => {
                if piece_index as usize >= self.block_manager.total_pieces() {
                    return vec![Effect::DoNothing];
                }

                if self.torrent_status == TorrentStatus::Validating
                    || self.torrent_status == TorrentStatus::AwaitingMetadata
                {
                    return vec![Effect::DoNothing];
                }

                self.block_manager.commit_v1_piece(piece_index);

                let mut effects = Vec::new();
                
                if let Some(peer) = self.peers.get_mut(&peer_id) {
                    peer.pending_requests.retain(|blk| blk.piece_index != piece_index);
                }
                
                let all_peers: Vec<String> = self.peers.keys().cloned().collect();
                
                for other_pid in all_peers {
                    if other_pid == peer_id { continue; }
                    
                    if let Some(p) = self.peers.get_mut(&other_pid) {
                        let blocks_to_cancel: Vec<BlockAddress> = p.pending_requests.iter()
                            .filter(|b| b.piece_index == piece_index)
                            .cloned()
                            .collect();
                        
                        if !blocks_to_cancel.is_empty() {
                            p.pending_requests.retain(|b| b.piece_index != piece_index);
                            for block in blocks_to_cancel {
                                effects.push(Effect::SendToPeer {
                                    peer_id: other_pid.clone(),
                                    cmd: Box::new(TorrentCommand::Cancel(
                                        block.piece_index,
                                        block.byte_offset,
                                        block.length
                                    )),
                                });
                            }
                            
                            effects.extend(self.update(Action::AssignWork { peer_id: other_pid }));
                        }
                    }
                }

                effects.push(Effect::EmitManagerEvent(ManagerEvent::DiskWriteFinished));
                effects.push(Effect::BroadcastHave { piece_index });
                effects.extend(self.update(Action::CheckCompletion));
                effects.extend(self.update(Action::AssignWork { peer_id }));

                effects
            }

            Action::PieceWriteFailed { piece_index } => {
                if piece_index as usize >= self.block_manager.total_pieces() {
                    return vec![Effect::DoNothing];
                }
                // UPDATED: Revert status in BlockManager
                self.block_manager.revert_v1_piece_completion(piece_index);
                vec![Effect::EmitManagerEvent(ManagerEvent::DiskWriteFinished)]
            }

            Action::RequestUpload {
                peer_id,
                piece_index,
                block_offset,
                length,
            } => {
                if self.torrent.is_none() {
                    return vec![Effect::DoNothing];
                }
                if length > MAX_BLOCK_SIZE {
                    return vec![Effect::DoNothing];
                }

                self.last_activity = TorrentActivity::SendingPiece(piece_index);

                let mut allowed = false;
                if let Some(peer) = self.peers.get_mut(&peer_id) {
                    // UPDATED: Check BlockManager
                    if peer.am_choking == ChokeStatus::Unchoke
                        && self.block_manager.is_piece_complete(piece_index)
                    {
                        allowed = true;
                    }
                }

                if allowed {
                    vec![Effect::ReadFromDisk {
                        peer_id,
                        block_info: BlockInfo {
                            piece_index,
                            offset: block_offset,
                            length,
                        },
                    }]
                } else {
                    vec![Effect::DoNothing]
                }
            }

            Action::TrackerResponse {
                url,
                peers,
                interval,
                min_interval,
            } => {
                let mut effects = Vec::new();

                if let Some(tracker) = self.trackers.get_mut(&url) {
                    let seeding_secs = if interval > 0 { interval + 1 } else { 1800 };
                    tracker.seeding_interval = Some(Duration::from_secs(seeding_secs));

                    let leeching_secs = min_interval.map(|m| m + 1).unwrap_or(60);
                    tracker.leeching_interval = Some(Duration::from_secs(leeching_secs));

                    let next_interval = if self.torrent_status != TorrentStatus::Done {
                        tracker.leeching_interval.unwrap()
                    } else {
                        tracker.seeding_interval.unwrap()
                    };
                    tracker.next_announce_time = self.now + next_interval;
                }

                for (ip, port) in peers {
                    let peer_addr = format!("{}:{}", ip, port);
                    if let Some((_, next_attempt)) = self.timed_out_peers.get(&peer_addr) {
                        if self.now < *next_attempt {
                            continue;
                        }
                    }
                    effects.push(Effect::ConnectToPeer { ip, port });
                }

                effects
            }

            Action::TrackerError { url } => {
                if let Some(tracker) = self.trackers.get_mut(&url) {
                    let current_interval = if self.torrent_status != TorrentStatus::Done {
                        tracker.leeching_interval.unwrap_or(Duration::from_secs(60))
                    } else {
                        tracker
                            .seeding_interval
                            .unwrap_or(Duration::from_secs(1800))
                    };

                    let backoff = current_interval.mul_f32(2.0).min(Duration::from_secs(3600));
                    tracker.next_announce_time = self.now + backoff;
                }
                vec![Effect::DoNothing]
            }

            Action::PeerConnectionFailed { peer_addr } => {
                let (count, _) = self
                    .timed_out_peers
                    .get(&peer_addr)
                    .cloned()
                    .unwrap_or((0, self.now));
                let new_count = (count + 1).min(10);
                let backoff_secs = (15 * 2u64.pow(new_count - 1)).min(1800);
                let next_attempt = self.now + Duration::from_secs(backoff_secs);

                self.timed_out_peers
                    .insert(peer_addr, (new_count, next_attempt));
                vec![Effect::DoNothing]
            }


            Action::MetadataReceived {
                torrent,
                metadata_length,
            } => {
                // 1. GUARD: Check if we already have metadata (prevents accidental overwrite)
                if self.torrent.is_some() {
                    return vec![Effect::DoNothing];
                }

                self.torrent = Some(*torrent.clone());
                self.torrent_metadata_length = Some(metadata_length);

                // 2. CRITICAL FIX: Set BlockManager Geometry
                // We calculate the correct size and set it on the *existing* BlockManager instance.
                let piece_len = torrent.info.piece_length as u32;
                let total_len = if torrent.info.files.is_empty() {
                    torrent.info.length as u64
                } else {
                    torrent.info.files.iter().map(|f| f.length as u64).sum()
                };
                
                // Set geometry based on the new metadata. This sizes the block_bitfield.
                self.block_manager.set_geometry(
                    piece_len,
                    total_len,
                    torrent.info.pieces.chunks(20)
                    .map(|chunk| {
                        let mut h = [0; 20];
                        h.copy_from_slice(chunk);
                        h
                    })
                    .collect(),
                    HashMap::new(), // V2 roots will be populated later
                    self.torrent_validation_status
                );

                // 3. Sync Peer Bitfield Lengths
                let num_pieces = self.block_manager.total_pieces();
                for peer in self.peers.values_mut() {
                    if peer.bitfield.len() > num_pieces {
                        peer.bitfield.truncate(num_pieces);
                    } else if peer.bitfield.len() < num_pieces {
                        peer.bitfield.resize(num_pieces, false);
                    }
                }

                // 4. Tracker Setup (if not done previously)
                if let Some(announce) = &torrent.announce {
                    self.trackers.insert(
                        announce.clone(),
                        TrackerState {
                            next_announce_time: self.now,
                            leeching_interval: None,
                            seeding_interval: None,
                        },
                    );
                }

                // 5. Transition State
                self.validation_pieces_found = 0;
                self.torrent_status = TorrentStatus::Validating;
                
                // 6. Emit Effects to start disk operation
                vec![Effect::InitializeStorage, Effect::StartValidation]
            }

            Action::ValidationComplete { completed_pieces } => {
                let mut effects = Vec::new();

                if self.torrent_status != TorrentStatus::Validating {
                    return vec![Effect::DoNothing];
                }

                if let Some(torrent) = &self.torrent {
                    let total_pieces = torrent.info.pieces.len() / 20;
                    for i in 0..total_pieces {
                        self.block_manager.revert_v1_piece_completion(i as u32);
                    }
                }

                for piece_index in &completed_pieces {
                     // Commit V1 pieces found on disk
                     self.block_manager.commit_v1_piece(*piece_index);
                }

                // Set status to Standard, then manually check if total completion occurred.
                self.torrent_status = TorrentStatus::Standard;

                let total_pieces = self.block_manager.total_pieces();
                let completed_count = self.block_manager.piece_hashes_v1.iter().enumerate()
                    .filter(|(i, _)| self.block_manager.is_piece_complete(*i as u32))
                    .count();

                if completed_count == total_pieces {
                    // Only transition to Done if we are truly complete based on the Model logic (5/5)
                    self.torrent_status = TorrentStatus::Done;

                    // Generate Done effects manually (copied from Action::CheckCompletion)
                    for (url, tracker) in self.trackers.iter_mut() {
                        tracker.next_announce_time = self.now;
                        effects.push(Effect::AnnounceCompleted { url: url.clone() });
                    }
                    for peer in self.peers.values_mut() {
                        if peer.am_interested {
                            peer.am_interested = false;
                            effects.push(Effect::SendToPeer {
                                peer_id: peer.ip_port.clone(),
                                cmd: Box::new(TorrentCommand::NotInterested),
                            });
                        }
                    }
                }


                // UPDATED: Reset manager queues
                self.block_manager.pending_blocks.clear(); 
                self.block_manager.legacy_buffers.clear();

                // UPDATED: Update Rarity
                self.block_manager
                    .update_rarity(self.peers.values().map(|p| &p.bitfield));

                if !self.is_paused {
                    if !self.has_started_announce_sent {
                        self.has_started_announce_sent = true;
                        effects.push(Effect::ConnectToPeersFromTrackers);
                    } else if self.torrent_status != TorrentStatus::Done { // Only announce if we aren't Done yet (Done effects are handled above)
                        for url in self.trackers.keys() {
                            effects.push(Effect::AnnounceToTracker { url: url.clone() });
                        }
                    }
                }

                for piece_index in &completed_pieces {
                    effects.push(Effect::BroadcastHave {
                        piece_index: *piece_index,
                    });
                }

                // If not Done, we still need to run CheckCompletion to handle any lingering states, 
                // but the critical status setting is handled above.
                if self.torrent_status != TorrentStatus::Done {
                    effects.extend(self.update(Action::CheckCompletion)); 
                }
                
                effects.extend(self.update(Action::RecalculateChokes {
                    random_seed: self.now.elapsed().as_nanos() as u64,
                }));

                for peer_id in self.peers.keys().cloned().collect::<Vec<_>>() {
                    effects.extend(self.update(Action::AssignWork { peer_id }));
                }

                effects
            }

            Action::CancelUpload {
                peer_id,
                piece_index,
                block_offset,
                length,
            } => {
                vec![Effect::AbortUpload {
                    peer_id,
                    block_info: BlockInfo {
                        piece_index,
                        offset: block_offset,
                        length,
                    },
                }]
            }

            Action::BlockSentToPeer {
                peer_id,
                byte_count,
            } => {
                self.session_total_uploaded =
                    self.session_total_uploaded.saturating_add(byte_count);
                self.bytes_uploaded_in_interval =
                    self.bytes_uploaded_in_interval.saturating_add(byte_count);

                if let Some(peer) = self.peers.get_mut(&peer_id) {
                    peer.bytes_uploaded_to_peer =
                        peer.bytes_uploaded_to_peer.saturating_add(byte_count);
                    peer.total_bytes_uploaded =
                        peer.total_bytes_uploaded.saturating_add(byte_count);
                    peer.bytes_uploaded_in_tick =
                        peer.bytes_uploaded_in_tick.saturating_add(byte_count);
                }

                vec![Effect::EmitManagerEvent(ManagerEvent::BlockSent {
                    info_hash: self.info_hash.clone(),
                })]
            }

            Action::Cleanup => {
                let mut effects = Vec::new();

                self.timed_out_peers
                    .retain(|_, (retry_count, _)| *retry_count < MAX_TIMEOUT_COUNT);

                let mut stuck_peers = Vec::new();
                for (id, peer) in &self.peers {
                    if peer.peer_id.is_empty()
                        && self.now.saturating_duration_since(peer.created_at)
                            > Duration::from_secs(5)
                    {
                        stuck_peers.push(id.clone());
                    }
                }

                for peer_id in stuck_peers {
                    if let Some(removed_peer) = self.peers.remove(&peer_id) {
                        // UPDATED: Release blocks
                        self.block_manager.release_pending_blocks_for_peer(&removed_peer.pending_requests);

                        effects.push(Effect::DisconnectPeer {
                            peer_id: peer_id.clone(),
                        });
                        effects.push(Effect::EmitManagerEvent(ManagerEvent::PeerDisconnected {
                            info_hash: self.info_hash.clone(),
                        }));
                    }
                }

                self.number_of_successfully_connected_peers = self.peers.len();

                // UPDATED: Check completion using BlockManager bitfield
                let am_seeding = !self.block_manager.block_bitfield.is_empty()
                    && self.block_manager.block_bitfield.iter().all(|&b| b);

                if am_seeding && self.torrent_status != TorrentStatus::Done {
                    self.torrent_status = TorrentStatus::Done;
                    effects.extend(self.update(Action::CheckCompletion));
                }

                if am_seeding {
                    let mut peers_to_disconnect = Vec::new();
                    for (peer_id, peer) in &self.peers {
                        let peer_is_seed = !peer.bitfield.is_empty()
                            && peer.bitfield.iter().all(|&has_piece| has_piece);
                        if peer_is_seed {
                            peers_to_disconnect.push(peer_id.clone());
                        }
                    }
                    for peer_id in peers_to_disconnect {
                        effects.push(Effect::DisconnectPeer { peer_id });
                    }
                }
                effects
            }

            Action::Pause => {
                self.last_activity = TorrentActivity::Paused;
                self.is_paused = true;

                self.last_known_peers = self.peers.keys().cloned().collect();

                // UPDATED: Reset pending blocks in BlockManager
                self.block_manager.pending_blocks.clear();

                self.peers.clear();

                self.number_of_successfully_connected_peers = 0;

                self.bytes_downloaded_in_interval = 0;
                self.bytes_uploaded_in_interval = 0;
                self.total_dl_prev_avg_ema = 0.0;
                self.total_ul_prev_avg_ema = 0.0;

                vec![
                    Effect::EmitMetrics {
                        bytes_dl: self.bytes_downloaded_in_interval,
                        bytes_ul: self.bytes_uploaded_in_interval,
                    },
                    Effect::ClearAllUploads,
                    Effect::EmitManagerEvent(ManagerEvent::PeerDisconnected {
                        info_hash: self.info_hash.clone(),
                    }),
                ]
            }

            Action::Resume => {
                self.last_activity = TorrentActivity::ConnectingToPeers;
                self.is_paused = false;

                if self.torrent_status == TorrentStatus::Validating {
                    return vec![Effect::DoNothing];
                }

                let mut effects = vec![Effect::TriggerDhtSearch];

                effects.extend(self.update(Action::ConnectToWebSeeds));

                for (url, tracker) in self.trackers.iter_mut() {
                    tracker.next_announce_time = self.now + Duration::from_secs(60);
                    effects.push(Effect::AnnounceToTracker { url: url.clone() });
                }

                let peers_to_connect: Vec<String> = std::mem::take(&mut self.last_known_peers)
                    .into_iter()
                    .collect();
                for peer_addr in peers_to_connect {
                    if let Ok(std::net::SocketAddr::V4(v4)) =
                        peer_addr.parse::<std::net::SocketAddr>()
                    {
                        effects.push(Effect::ConnectToPeer {
                            ip: v4.ip().to_string(),
                            port: v4.port(),
                        });
                    }
                }

                effects
            }

            Action::Delete => {
                self.peers.clear();
                self.last_known_peers.clear();
                self.timed_out_peers.clear();

                self.block_manager = BlockManager::new();
                let is_torrent_present = self.torrent.is_some();

                if let Some(ref t) = self.torrent {
                     let piece_len = t.info.piece_length as u32;
                     let total_len: u64 = if !t.info.files.is_empty() {
                        t.info.files.iter().map(|f| f.length as u64).sum()
                     } else {
                        t.info.length as u64
                     };
                     self.block_manager.set_geometry(
                         piece_len, 
                         total_len, 
                         t.info.pieces.chunks(20)
                            .map(|chunk| {
                                let mut h = [0; 20];
                                h.copy_from_slice(chunk);
                                h
                            })
                            .collect(),
                         HashMap::new(), 
                         self.torrent_validation_status
                     );
                }

                self.number_of_successfully_connected_peers = 0;

                self.session_total_downloaded = 0;
                self.session_total_uploaded = 0;

                self.bytes_downloaded_in_interval = 0;
                self.bytes_uploaded_in_interval = 0;

                self.is_paused = true;
                self.torrent_status = if self.torrent.is_some() {
                    TorrentStatus::Validating
                } else {
                    TorrentStatus::AwaitingMetadata
                };
                self.last_activity = TorrentActivity::Initializing;

                vec![Effect::DeleteFiles]
            }

            Action::UpdateListenPort => {
                let mut effects = Vec::new();

                for (url, tracker) in self.trackers.iter_mut() {
                    tracker.next_announce_time = self.now + Duration::from_secs(60);
                    effects.push(Effect::AnnounceToTracker { url: url.clone() });
                }

                effects
            }

            Action::ValidationProgress { count } => {
                self.validation_pieces_found = count;
                vec![Effect::DoNothing]
            }

            Action::Shutdown => {
                self.is_paused = true;
                let left = if let Some(_t) = &self.torrent {
                    // UPDATED: Count missing blocks * 16KB
                    let blocks_needed = self.block_manager.block_bitfield.iter().filter(|&&b| !b).count();
                    blocks_needed * BLOCK_SIZE as usize
                } else {
                    0
                };

                let tracker_urls: Vec<String> = self.trackers.keys().cloned().collect();
                let uploaded = self.session_total_uploaded as usize;
                let downloaded = self.session_total_downloaded as usize;

                self.peers.clear();

                vec![Effect::PrepareShutdown {
                    tracker_urls,
                    left,
                    uploaded,
                    downloaded,
                }]
            }

            Action::FatalError => self.update(Action::Pause),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PeerState {
    pub ip_port: String,
    pub peer_id: Vec<u8>,
    pub bitfield: Vec<bool>,
    pub am_choking: ChokeStatus,
    pub peer_choking: ChokeStatus,
    pub peer_tx: Sender<TorrentCommand>,
    pub am_interested: bool,
    // UPDATED: pending_requests is now BlockAddress
    pub pending_requests: HashSet<BlockAddress>,
    pub peer_is_interested_in_us: bool,
    pub bytes_downloaded_from_peer: u64,
    pub bytes_uploaded_to_peer: u64,
    pub bytes_downloaded_in_tick: u64,
    pub bytes_uploaded_in_tick: u64,
    pub prev_avg_dl_ema: f64,
    pub prev_avg_ul_ema: f64,
    pub total_bytes_downloaded: u64,
    pub total_bytes_uploaded: u64,
    pub download_speed_bps: u64,
    pub upload_speed_bps: u64,
    pub upload_slots_semaphore: Arc<Semaphore>,
    pub last_action: TorrentCommand,
    pub action_counts: HashMap<Discriminant<TorrentCommand>, u64>,
    pub created_at: Instant,
}

impl PeerState {
    pub fn new(ip_port: String, peer_tx: Sender<TorrentCommand>, created_at: Instant) -> Self {
        Self {
            ip_port,
            peer_id: Vec::new(),
            bitfield: Vec::new(),
            am_choking: ChokeStatus::Choke,
            peer_choking: ChokeStatus::Choke,
            peer_tx,
            am_interested: false,
            pending_requests: HashSet::new(),
            peer_is_interested_in_us: false,
            bytes_downloaded_from_peer: 0,
            bytes_uploaded_to_peer: 0,
            bytes_downloaded_in_tick: 0,
            bytes_uploaded_in_tick: 0,
            total_bytes_downloaded: 0,
            total_bytes_uploaded: 0,
            prev_avg_dl_ema: 0.0,
            prev_avg_ul_ema: 0.0,
            download_speed_bps: 0,
            upload_speed_bps: 0,
            upload_slots_semaphore: Arc::new(Semaphore::new(PEER_UPLOAD_IN_FLIGHT_LIMIT)),
            last_action: TorrentCommand::SuccessfullyConnected(String::new()),
            action_counts: HashMap::new(),
            created_at,
        }
    }
}

// -----------------------------------------------------------------------------
// INVARIANTS
// -----------------------------------------------------------------------------

#[cfg(test)]
fn check_invariants(state: &TorrentState) {
    // 1. Global Stats vs. Peer Stats
    let sum_peer_dl: u64 = state.peers.values().map(|p| p.total_bytes_downloaded).sum();
    let sum_peer_ul: u64 = state.peers.values().map(|p| p.total_bytes_uploaded).sum();

    assert!(state.session_total_downloaded >= sum_peer_dl, "Global DL < Sum Peer DL");
    assert!(state.session_total_uploaded >= sum_peer_ul, "Global UL < Sum Peer UL");

    // 2. Bitfield Integrity
    if let Some(torrent) = &state.torrent {
        let expected_pieces = torrent.info.pieces.len() / 20;
        for (id, peer) in &state.peers {
            if !peer.bitfield.is_empty() {
                assert_eq!(peer.bitfield.len(), expected_pieces, "Peer {} bitfield len mismatch", id);
            }
        }
    }

    // 3. Orphaned Pending Check
    for &global_idx in &state.block_manager.pending_blocks {
        let exists = state.peers.values().any(|p| 
            p.pending_requests.iter().any(|addr| 
                state.block_manager.flatten_address(*addr) == global_idx
            )
        );
        if !exists { /* Warn only */ }
    }

    // 4. Capability Check
    for (id, peer) in &state.peers {
        for &req in &peer.pending_requests {
            let has_piece = peer.bitfield.get(req.piece_index as usize).copied().unwrap_or(false);
            assert!(has_piece, "Requested block {:?} from peer {} who doesn't have it", req, id);
        }
    }
}

// -----------------------------------------------------------------------------
// UNIT TESTS (Ported from V1)
// -----------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::TorrentCommand;
    use tokio::sync::mpsc;

    // --- Helpers ---

    pub(crate) fn create_empty_state() -> TorrentState {
        let mut block_manager = BlockManager::new();
        // Default to 1 piece of 16KB to prevent panic on empty geometry access
        block_manager.set_geometry(16384, 16384, vec![[0;20]], HashMap::new(), false);

        TorrentState {
            info_hash: vec![0; 20],
            peers: HashMap::new(),
            block_manager,
            trackers: HashMap::new(),
            ..Default::default()
        }
    }

    pub(crate) fn create_dummy_torrent(piece_count: usize) -> Torrent {
        use crate::torrent_file::Info;
        Torrent {
            announce: Some("http://tracker.test".to_string()),
            announce_list: None,
            url_list: None,
            info: Info {
                name: "test_torrent".to_string(),
                piece_length: 16384,                 
                pieces: vec![0u8; 20 * piece_count], 
                length: (16384 * piece_count) as i64,
                files: vec![],
                private: None,
                md5sum: None,
            },
            info_dict_bencode: vec![],
            created_by: None,
            creation_date: None,
            encoding: None,
            comment: None,
        }
    }

    fn add_peer(state: &mut TorrentState, id: &str) {
        let (tx, _) = mpsc::channel(1);
        let mut peer = PeerState::new(id.to_string(), tx, state.now);
        peer.peer_id = id.as_bytes().to_vec();
        
        let total_pieces = state.block_manager.total_pieces();
        // Ensure bitfield matches geometry
        let size = if total_pieces == 0 { 1 } else { total_pieces };
        peer.bitfield = vec![false; size];
        
        state.peers.insert(id.to_string(), peer);
    }

    // --- TESTS ---

    #[test]
    fn test_metadata_received_triggers_initialization_flow() {
        let mut state = create_empty_state();
        let torrent = create_dummy_torrent(5); 

        let action = Action::MetadataReceived {
            torrent: Box::new(torrent),
            metadata_length: 123,
        };
        let effects = state.update(action);

        assert_eq!(state.torrent_status, TorrentStatus::Validating);
        assert!(state.torrent.is_some());
        assert!(matches!(effects[0], Effect::InitializeStorage));
        assert!(matches!(effects[1], Effect::StartValidation));
    }

    #[test]
    fn test_assign_work_requests_piece_peer_has() {
        let mut state = create_empty_state();
        let torrent = create_dummy_torrent(10);
        // Fix: Provide 10 dummy hashes so logic knows there are 10 pieces
        state.block_manager.set_geometry(16384, 163840, vec![[0;20]; 10], HashMap::new(), false);
        state.torrent = Some(torrent);
        state.torrent_status = TorrentStatus::Standard;

        add_peer(&mut state, "peer_A");
        let peer = state.peers.get_mut("peer_A").unwrap();
        peer.peer_choking = ChokeStatus::Unchoke;
        // Peer has Piece 0
        peer.bitfield[0] = true; 

        state.block_manager.update_rarity(state.peers.values().map(|p| &p.bitfield));

        // WHEN: We assign work
        let effects = state.update(Action::AssignWork {
            peer_id: "peer_A".to_string(),
        });

        // THEN: Expect Request for Piece 0, Offset 0, Len 16384
        let request = effects.iter().find(|e| {
            matches!(e, Effect::SendToPeer { cmd, .. }
            if matches!(**cmd, TorrentCommand::RequestDownload(0, _, _)))
        });

        assert!(request.is_some(), "Should request piece 0 from peer_A");
        assert!(!state.peers["peer_A"].pending_requests.is_empty());
    }

    #[test]
    fn test_piece_verified_valid_trigger_write() {
        let mut state = create_empty_state();
        state.block_manager.set_geometry(16384, 16384 * 5, vec![[0;20]; 5], HashMap::new(), false);
        
        let data = vec![1, 2, 3, 4];
        let effects = state.update(Action::PieceVerified {
            peer_id: "peer_1".into(),
            piece_index: 1,
            valid: true,
            data: data.clone(),
        });

        assert!(effects.iter().any(|e| matches!(e, Effect::WriteToDisk { piece_index: 1, .. })));
        assert!(state.block_manager.is_piece_complete(1));
    }

    #[test]
    fn test_piece_verified_invalid_disconnects_peer() {
        let mut state = create_empty_state();
        // Fix: Must provide at least 2 pieces so index 1 is valid
        state.block_manager.set_geometry(16384, 32768, vec![[0;20]; 2], HashMap::new(), false);

        let effects = state.update(Action::PieceVerified {
            peer_id: "bad_peer".into(),
            piece_index: 1,
            valid: false,
            data: vec![],
        });

        assert!(effects.iter().any(|e| matches!(e, Effect::DisconnectPeer { .. })));
    }

    #[test]
    fn test_check_completion_transitions_to_done() {
        let mut state = create_empty_state();
        state.block_manager.set_geometry(16384, 16384 * 3, vec![[0;20]; 3], HashMap::new(), false);
        state.torrent_status = TorrentStatus::Standard;
        
        // Add tracker so AnnounceCompleted is emitted
        state.trackers.insert(
            "http://tracker".into(),
            TrackerState {
                next_announce_time: Instant::now(),
                leeching_interval: None,
                seeding_interval: None,
            },
        );

        // Manually mark all blocks as Done
        state.block_manager.block_bitfield.fill(true);

        let effects = state.update(Action::CheckCompletion);

        assert_eq!(state.torrent_status, TorrentStatus::Done);
        assert!(effects.iter().any(|e| matches!(e, Effect::AnnounceCompleted { .. })));
    }

    #[test]
    fn test_invariant_pending_removed_on_disk_write() {
        let mut state = create_empty_state();
        let torrent = create_dummy_torrent(1);
        state.torrent = Some(torrent);
        state.block_manager.set_geometry(16384, 16384, vec![[0;20]], HashMap::new(), false);
        state.torrent_status = TorrentStatus::Standard;
        
        add_peer(&mut state, "peer_A");
        let peer = state.peers.get_mut("peer_A").unwrap();
        peer.bitfield = vec![true]; 
        peer.peer_choking = ChokeStatus::Unchoke;

        state.block_manager.update_rarity(state.peers.values().map(|p| &p.bitfield));

        // 1. Assign Work (Puts block 0 in pending)
        let _ = state.update(Action::AssignWork { peer_id: "peer_A".into() });
        assert!(!state.peers["peer_A"].pending_requests.is_empty(), "Setup failed: Work not assigned");

        // 2. Simulate Write
        state.update(Action::PieceWrittenToDisk {
            peer_id: "peer_A".into(),
            piece_index: 0,
        });

        // 3. Assert pending is cleared
        assert!(state.peers["peer_A"].pending_requests.is_empty(), "Invariant failed: Pending request persisted after write");
    }

    #[test]
    fn regression_delete_clears_state() {
        let mut state = create_empty_state();
        state.block_manager.set_geometry(16384, 16384, vec![[0;20]], HashMap::new(), false);
        
        // Pollute state
        state.block_manager.block_bitfield[0] = true;
        state.block_manager.mark_pending(0);

        // DELETE
        state.update(Action::Delete);

        assert!(state.block_manager.pending_blocks.is_empty());
        // Since Delete re-inits BlockManager, bitfield should be empty until MetadataReceived is processed again
        assert!(state.block_manager.block_bitfield.is_empty()); 
    }

    #[test]
    fn test_download_starts_immediately_after_validation() {
        let mut state = create_empty_state();
        // Fix: 2 pieces geometry matches bitfield 0xC0 (11000000) for 2 pieces
        let torrent = create_dummy_torrent(2); 
        state.torrent = Some(torrent);
        state.block_manager.set_geometry(
            16384, 
            32768, 
            vec![[0;20]; 2], 
            HashMap::new(), 
            false
        );
        state.torrent_status = TorrentStatus::Validating;

        add_peer(&mut state, "seeder");
        state.update(Action::PeerBitfieldReceived {
            peer_id: "seeder".into(),
            bitfield: vec![0xC0], // Has piece 0 and 1
        });
        state.update(Action::PeerUnchoked { peer_id: "seeder".into() });

        // WHEN: Validation completes
        let effects = state.update(Action::ValidationComplete { completed_pieces: vec![] });

        // THEN: RequestDownload
        let request_sent = effects.iter().any(|e| {
            matches!(e, Effect::SendToPeer { cmd, .. }
            if matches!(**cmd, TorrentCommand::RequestDownload(0, _, _)))
        });
        assert!(request_sent, "Download did not trigger after validation");
    }

    #[test]
    fn test_partial_piece_request() {
        // GIVEN: 2-block piece (32KB). We have the first block.
        let mut state = create_empty_state();
        // 32KB piece, 1 file
        state.block_manager.set_geometry(32768, 32768, vec![[0;20]], HashMap::new(), false);
        state.torrent = Some(create_dummy_torrent(1));
        
        state.torrent_status = TorrentStatus::Standard;

        // Mark Block 0 as Done
        state.block_manager.block_bitfield[0] = true;

        add_peer(&mut state, "seeder");
        state.peers.get_mut("seeder").unwrap().bitfield = vec![true];
        state.peers.get_mut("seeder").unwrap().peer_choking = ChokeStatus::Unchoke;

        state.block_manager.update_rarity(state.peers.values().map(|p| &p.bitfield));

        // WHEN: Assign Work
        let effects = state.update(Action::AssignWork { peer_id: "seeder".into() });

        // THEN: Should request Block 1 (Offset 16384), NOT Block 0
        let req = effects.iter().find(|e| {
            matches!(e, Effect::SendToPeer { cmd, .. }
            if matches!(**cmd, TorrentCommand::RequestDownload(0, 16384, 16384)))
        });

        assert!(req.is_some(), "Should request 2nd block (offset 16384)");
    }

    #[test]
    fn test_final_piece_write_transitions_to_done() {
        // GIVEN: A torrent with 5 pieces, 4 of which are already complete
        let mut state = create_empty_state();
        let piece_count = 5;
        let piece_len = 16384;
        let total_len = piece_len as u64 * piece_count as u64;
        
        // 1. Setup Torrent Metadata and BlockManager Geometry
        let torrent = create_dummy_torrent(piece_count);
        state.torrent = Some(torrent);
        state.block_manager.set_geometry(
            piece_len, 
            total_len, 
            vec![[0;20]; piece_count], // 5 V1 piece hashes
            HashMap::new(), 
            false
        );
        state.torrent_status = TorrentStatus::Standard;

        // 2. Add a peer (required by Action::PieceWrittenToDisk signature)
        add_peer(&mut state, "writer_peer");
        
        // 3. Manually mark pieces 0 through 3 as complete in the BlockManager
        for i in 0..4 {
            state.block_manager.commit_v1_piece(i);
        }

        state.trackers.insert(
            "http://tracker.test/announce".to_string(),
            TrackerState {
                next_announce_time: state.now, // Use state.now as placeholder
                leeching_interval: None,
                seeding_interval: None,
            },
        );
        
        // Sanity check: Should not be Done yet
        assert_eq!(state.torrent_status, TorrentStatus::Standard);
        
        // WHEN: The final piece (index 4) is written to disk
        let effects = state.update(Action::PieceWrittenToDisk {
            peer_id: "writer_peer".into(),
            piece_index: 4,
        });

        // THEN: The status must transition to Done
        assert_eq!(state.torrent_status, TorrentStatus::Done, 
            "Status failed to transition from Standard to Done after final piece write.");

        // And it should have triggered AnnounceCompleted
        assert!(effects.iter().any(|e| matches!(e, Effect::AnnounceCompleted { .. })));
        
        // And the BlockManager bitfield must be complete
        let all_blocks_set = state.block_manager.block_bitfield.iter().all(|&b| b);
        assert!(all_blocks_set, "BlockManager bitfield is not fully set (internal geometry bug likely).");
    }

#[test]
fn test_assign_work_respects_existing_blocks() {
    // GIVEN: A torrent with one piece made up of 3 blocks.
    let piece_len = 3 * BLOCK_SIZE; // 3 blocks total
    let total_len = piece_len as u64;
    let piece_count = 1;

    let mut state = create_empty_state();
    // Set up BlockManager geometry for 1 piece (3 blocks total: 0, 1, 2)
    state.block_manager.set_geometry(piece_len, total_len, vec![[0;20]; piece_count], HashMap::new(), false);
    state.torrent = Some(create_dummy_torrent(piece_count));
    state.torrent_status = TorrentStatus::Standard;

    // Manually mark the middle block (global index 1) as already received.
    // Block 0: Missing, Block 1: DONE, Block 2: Missing
    state.block_manager.block_bitfield[1] = true; 

    add_peer(&mut state, "peer_A");
    let peer = state.peers.get_mut("peer_A").unwrap();
    peer.peer_choking = ChokeStatus::Unchoke;
    // Peer has Piece 0
    peer.bitfield[0] = true; 

    state.block_manager.update_rarity(state.peers.values().map(|p| &p.bitfield));

    // WHEN: We assign work
    let effects = state.update(Action::AssignWork {
        peer_id: "peer_A".to_string(),
    });

    // THEN: Expect requests for Block 0 and Block 2, but NOT Block 1.
    // Block 0: piece 0, offset 0, len 16384
    // Block 1: piece 0, offset 16384, len 16384 (SKIPPED)
    // Block 2: piece 0, offset 32768, len 16384

    let requested_offsets: Vec<i64> = effects.iter().filter_map(|e| {
        if let Effect::SendToPeer { cmd, .. } = e {
            if let TorrentCommand::RequestDownload(0, offset, 16384) = **cmd {
                Some(offset)
            } else {
                None
            }
        } else {
            None
        }
    }).collect();

    let expected_offsets = vec![0, 32768]; // Global blocks 0 and 2

    // Check that exactly the missing blocks were requested
    assert_eq!(requested_offsets.len(), 2, "Should request exactly 2 blocks (0 and 2). Requested: {:?}", requested_offsets);
    assert!(requested_offsets.contains(&0i64), "Should request the first block (offset 0)");
    assert!(requested_offsets.contains(&32768i64), "Should request the third block (offset 32768)");
    assert!(!requested_offsets.contains(&16384i64), "Should NOT request the second block (offset 16384) which is already complete");
    
    // Check that the correct blocks are marked as pending
    let pending_global_indices: Vec<u32> = state.peers["peer_A"].pending_requests.iter()
        .map(|addr| state.block_manager.flatten_address(*addr))
        .collect();
    
    assert!(pending_global_indices.contains(&0), "Global block 0 should be pending");
    assert!(!pending_global_indices.contains(&1), "Global block 1 should NOT be pending");
    assert!(pending_global_indices.contains(&2), "Global block 2 should be pending");
}
}

// -----------------------------------------------------------------------------
// PROPERTY TESTS (Fuzzers)
// -----------------------------------------------------------------------------
#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;
    use tokio::sync::mpsc;
    use crate::torrent_manager::block_manager::{BLOCK_SIZE};
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    // --- Constants for Consistent Fuzzing ---
    const PIECE_LEN: u32 = 16384;
    const NUM_PIECES: usize = 5; // Small number for state machine speed
    const MAX_BLOCK: u32 = 131_072;

    #[derive(Clone, Debug)]
    enum NetworkFault {
        None,
        Drop,
        Duplicate,
        Delay(u64),
        Corrupt,
    }

    fn inject_reordering_faults(actions: Vec<Action>, seed: u64) -> Vec<Action> {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut pending = Vec::new();
        let mut result = Vec::new();

        for action in actions {
            if rng.random_bool(0.02) { continue; } // Drop
            if rng.random_bool(0.01) { // Dupe
                let delay = rng.random_range(10..400);
                pending.push((delay, action.clone()));
            }
            let delay = rng.random_range(10..400);
            pending.push((delay, action));
        }

        pending.sort_by_key(|(delay, _)| *delay);

        let mut current_time = 0;
        for (arrival_time, action) in pending {
            if arrival_time > current_time {
                result.push(Action::Tick { dt_ms: arrival_time - current_time });
                current_time = arrival_time;
            }
            result.push(action);
        }
        result
    }

    fn inject_network_faults(actions: Vec<Action>, fault_entropy: Vec<u8>) -> Vec<Action> {
        let mut final_actions = Vec::new();
        let mut entropy_iter = fault_entropy.iter().cycle();

        for action in actions {
            let seed = *entropy_iter.next().unwrap();
            let fault = match seed {
                0..=4 => NetworkFault::Drop,
                5..=9 => NetworkFault::Duplicate,
                10..=20 => NetworkFault::Delay(seed as u64 * 50),
                21..=25 => NetworkFault::Corrupt,
                _ => NetworkFault::None,
            };

            match fault {
                NetworkFault::Drop => continue,
                NetworkFault::Duplicate => {
                    final_actions.push(action.clone());
                    final_actions.push(action);
                }
                NetworkFault::Delay(ms) => {
                    final_actions.push(Action::Tick { dt_ms: ms });
                    final_actions.push(action);
                }
                NetworkFault::Corrupt => {
                    match action {
                        Action::IncomingBlock { peer_id, piece_index, block_offset, mut data } => {
                            if !data.is_empty() {
                                let len = data.len();
                                data[len - 1] = !data[len - 1]; 
                            }
                            final_actions.push(Action::IncomingBlock { peer_id, piece_index, block_offset, data });
                        }
                        _ => continue, 
                    }
                }
                NetworkFault::None => {
                    final_actions.push(action);
                }
            }
        }
        final_actions
    }

    // =========================================================================
    // 1. STRATEGIES
    // =========================================================================

    fn tit_for_tat_strategy() -> impl Strategy<Value = TorrentState> {
        let num_peers = 5usize;
        proptest::collection::vec(0..100_000u64, num_peers).prop_map(move |speeds| {
            let mut state = super::tests::create_empty_state();
            state.block_manager.set_geometry(PIECE_LEN, (PIECE_LEN * NUM_PIECES as u32) as u64, vec![[0;20]; NUM_PIECES], HashMap::new(), false);
            state.torrent_status = TorrentStatus::Standard;

            for (i, &speed) in speeds.iter().enumerate() {
                let id = format!("peer_{}", i);
                let (tx, _) = mpsc::channel(1);
                let mut peer = PeerState::new(id.clone(), tx, state.now);
                peer.peer_id = id.as_bytes().to_vec();
                peer.peer_is_interested_in_us = true;
                peer.am_choking = super::ChokeStatus::Choke;
                peer.bytes_downloaded_from_peer = speed;
                state.peers.insert(id, peer);
            }
            state.number_of_successfully_connected_peers = state.peers.len();
            state
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::default())]

        #[test]
        fn test_fuzz_assign_work_never_panics(mut state in tit_for_tat_strategy()) {
            if let Some(peer_id) = state.peers.keys().next().cloned() {
                state.block_manager.block_bitfield.fill(false);
                state.block_manager.pending_blocks.clear();
                state.torrent = Some(super::tests::create_dummy_torrent(NUM_PIECES));
                let _ = state.update(Action::AssignWork { peer_id });
                super::check_invariants(&state);
            }
        }

        #[test]
        fn test_fuzz_incoming_block_bounds(
            mut state in tit_for_tat_strategy(),
            piece_idx in 0..10u32,
            offset in 0..100_000u32, 
            len in 0..20_000usize
        ) {
            let data = vec![0u8; len];
            if let Some(peer_id) = state.peers.keys().next().cloned() {
                let _ = state.update(Action::IncomingBlock {
                    peer_id,
                    piece_index: piece_idx,
                    block_offset: offset,
                    data
                });
            }
        }
    }

    // =========================================================================
    // STATE MACHINE FUZZER
    // =========================================================================
    mod state_machine {
        use super::*;
        use super::{inject_network_faults, inject_reordering_faults};
        use crate::torrent_manager::state::tests::create_dummy_torrent;
        use proptest_state_machine::{ReferenceStateMachine, StateMachineTest};
        use std::collections::HashSet;

        #[derive(Clone, Debug)]
        pub struct TorrentModel {
            pub connected_peers: HashSet<String>,
            pub total_pieces: u32,
            pub paused: bool,
            pub status: TorrentStatus,
            pub has_metadata: bool,
            pub downloaded_pieces: HashSet<u32>,
        }

        impl TorrentModel {
            fn new_file(pieces: u32) -> Self {
                Self {
                    connected_peers: HashSet::new(),
                    total_pieces: pieces,
                    paused: false,
                    status: TorrentStatus::Validating,
                    has_metadata: true,
                    downloaded_pieces: HashSet::new(),
                }
            }

            fn new_magnet(pieces: u32) -> Self {
                Self {
                    connected_peers: HashSet::new(),
                    total_pieces: pieces,
                    paused: false,
                    status: TorrentStatus::AwaitingMetadata,
                    has_metadata: false,
                    downloaded_pieces: HashSet::new(),
                }
            }
        }

        impl ReferenceStateMachine for TorrentModel {
            type State = Self;
            type Transition = Action;

            fn init_state() -> BoxedStrategy<Self::State> {
                prop_oneof![
                    Just(TorrentModel::new_file(NUM_PIECES as u32)),
                    Just(TorrentModel::new_magnet(NUM_PIECES as u32))
                ].boxed()
            }

            fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
                let mut strategies = vec![
                    Just(Action::Tick { dt_ms: 1000 }).boxed(),
                    Just(Action::Cleanup).boxed(),
                    Just(Action::FatalError).boxed(),
                    Just(Action::Shutdown).boxed(),
                    Just(Action::Delete).boxed(),
                    Just(Action::ConnectToWebSeeds).boxed(),
                ];

                strategies.push(
                    any::<bool>().prop_map(|paused| Action::TorrentManagerInit {
                        is_paused: paused,
                        announce_immediately: !paused,
                    }).boxed(),
                );

                if state.paused {
                    strategies.push(Just(Action::Resume).boxed());
                } else {
                    strategies.push(Just(Action::Pause).boxed());
                }

                if state.status == TorrentStatus::AwaitingMetadata {
                    strategies.push(
                        Just(Action::MetadataReceived {
                            torrent: Box::new(create_dummy_torrent(state.total_pieces as usize)),
                            metadata_length: (state.total_pieces * 16384) as i64,
                        }).boxed(),
                    );
                }

                if state.status == TorrentStatus::Validating {
                    let max_pieces = state.total_pieces;
                    strategies.push(
                        proptest::collection::vec(0..max_pieces, 0..max_pieces as usize)
                            .prop_map(|pieces| Action::ValidationComplete { completed_pieces: pieces }).boxed(),
                    );
                }

                if state.status == TorrentStatus::Standard || state.status == TorrentStatus::Endgame {
                    strategies.push(Just(Action::CheckCompletion).boxed());
                }

                strategies.push(
                    any::<String>().prop_map(|id| Action::PeerSuccessfullyConnected { peer_id: id }).boxed(),
                );

                if !state.connected_peers.is_empty() && state.has_metadata {
                    let peer_strategy = prop::sample::select(Vec::from_iter(state.connected_peers.clone()));
                    let piece_strategy = 0..state.total_pieces;

                    strategies.push(peer_strategy.clone().prop_map(|id| Action::PeerDisconnected { peer_id: id }).boxed());
                    strategies.push(peer_strategy.clone().prop_map(|id| Action::PeerUnchoked { peer_id: id }).boxed());

                    if state.status != TorrentStatus::Validating && state.status != TorrentStatus::AwaitingMetadata {
                        strategies.push((peer_strategy.clone(), piece_strategy.clone()).prop_map(|(id, idx)| Action::PeerHavePiece { peer_id: id, piece_index: idx }).boxed());
                        strategies.push(peer_strategy.clone().prop_map(|id| Action::AssignWork { peer_id: id }).boxed());
                        strategies.push((peer_strategy.clone(), piece_strategy.clone(), any::<u32>(), prop::collection::vec(any::<u8>(), 1..1024))
                            .prop_map(|(id, idx, offset, data)| Action::IncomingBlock { peer_id: id, piece_index: idx, block_offset: offset, data }).boxed());
                        strategies.push((peer_strategy.clone(), piece_strategy.clone()).prop_map(|(id, idx)| Action::PieceWrittenToDisk { peer_id: id, piece_index: idx }).boxed());
                    }
                }
                prop::strategy::Union::new(strategies).boxed()
            }

            fn apply(mut state: Self::State, trans: &Self::Transition) -> Self::State {



                match trans {
                    Action::PeerSuccessfullyConnected { peer_id } => {
                        state.connected_peers.insert(peer_id.clone());
                    }
                    Action::PeerDisconnected { peer_id } => {
                        state.connected_peers.remove(peer_id);
                    }
                    Action::Pause | Action::FatalError => {
                        state.paused = true;
                        state.connected_peers.clear();
                    }
                    Action::Resume => {
                        state.paused = false;
                    }
                    Action::TorrentManagerInit { is_paused, .. } => {
                        state.paused = *is_paused;
                    }
                    Action::Shutdown => {
                        state.paused = true;
                        state.connected_peers.clear();
                    }
                    Action::Delete => {
                        state.paused = true;
                        state.connected_peers.clear();
                        state.downloaded_pieces.clear(); // <-- Ensure this is explicitly cleared
                        if state.has_metadata {
                            state.status = TorrentStatus::Validating;
                        } else {
                            state.status = TorrentStatus::AwaitingMetadata;
                        }
                    }
                    Action::MetadataReceived { .. } => {
                        if !state.has_metadata {
                            state.has_metadata = true;
                            state.status = TorrentStatus::Validating;
                            state.downloaded_pieces.clear();
                        }
                    }
                    Action::ValidationComplete { completed_pieces } => {
                        if state.status == TorrentStatus::Validating {
                            state.status = TorrentStatus::Standard; 

                            for p in completed_pieces {
                                state.downloaded_pieces.insert(*p);
                            }
                            
                            if state.downloaded_pieces.len() as u32 == state.total_pieces {
                                state.status = TorrentStatus::Done;
                            }
                        }
                    }
                    Action::PieceWrittenToDisk { piece_index, .. } => {
                        if state.status == TorrentStatus::Standard || state.status == TorrentStatus::Endgame {
                            state.downloaded_pieces.insert(*piece_index);
                            if state.downloaded_pieces.len() as u32 == state.total_pieces {
                                state.status = TorrentStatus::Done;
                            }
                        }
                    }
                    Action::CheckCompletion => {
                        if state.status == TorrentStatus::Standard || state.status == TorrentStatus::Endgame {
                            if state.downloaded_pieces.len() as u32 == state.total_pieces {
                                state.status = TorrentStatus::Done;
                            }
                        }
                    }
                    _ => {}
                }
                state
            }
        }

        impl StateMachineTest for TorrentModel {
            type SystemUnderTest = TorrentState;
            type Reference = TorrentModel;


fn init_test(ref_state: &TorrentModel) -> Self::SystemUnderTest {
    let piece_count = ref_state.total_pieces as usize;
    
    let torrent_info = super::super::tests::create_dummy_torrent(piece_count);

    let v1_hashes_list: Vec<[u8; 20]> = vec![[0; 20]; piece_count];

    let mut block_manager = BlockManager::new();
    let total_bytes = (super::PIECE_LEN as u64) * (piece_count as u64);

    let torrent_validation_status = false;
    
    if ref_state.has_metadata {
        block_manager.set_geometry(
            super::PIECE_LEN, 
            total_bytes, 
            v1_hashes_list,   // Use the guaranteed 5-entry hash list
            HashMap::new(), 
            torrent_validation_status
        );
        
        block_manager.block_bitfield.fill(false); // <--- ADD THIS EXPLICIT CLEAR

        for &piece in &ref_state.downloaded_pieces {
            block_manager.commit_v1_piece(piece);
        }
    }


let initial_status = ref_state.status.clone();


    TorrentState {
        torrent: if ref_state.has_metadata { Some(torrent_info) } else { None }, 
        torrent_status: initial_status,
        is_paused: ref_state.paused,
        block_manager,
        torrent_validation_status,
        ..Default::default()
    }
}
            fn apply(
                mut sut: Self::SystemUnderTest,
                ref_state: &TorrentModel,
                transition: Action,
            ) -> Self::SystemUnderTest {
                if let Action::PeerSuccessfullyConnected { peer_id } = &transition {
                    if !sut.peers.contains_key(peer_id) {
                        let (tx, _) = tokio::sync::mpsc::channel(1);
                        let mut peer = PeerState::new(peer_id.clone(), tx, sut.now);
                        peer.peer_id = peer_id.as_bytes().to_vec();
                        if ref_state.has_metadata {
                            peer.bitfield = vec![false; ref_state.total_pieces as usize];
                        }
                        sut.peers.insert(peer_id.clone(), peer);
                        sut.number_of_successfully_connected_peers = sut.peers.len();
                    }
                }

                let _ = sut.update(transition.clone());

                let expected_state = <TorrentModel as ReferenceStateMachine>::apply(ref_state.clone(), &transition);

                assert_eq!(sut.torrent.is_some(), expected_state.has_metadata, "Metadata mismatch");

println!("--- ACTION: {:?} ---", transition);
println!("SUT Status: {:?}", sut.torrent_status);
println!("SUT Completed Pieces: {}", sut.block_manager.piece_hashes_v1.iter().enumerate()
    .filter(|(i, _)| sut.block_manager.is_piece_complete(*i as u32)).count());
println!("Model Status: {:?}", expected_state.status);
println!("Model Completed Pieces: {}", expected_state.downloaded_pieces.len());

                let sut_status_norm = if sut.torrent_status == TorrentStatus::Endgame { TorrentStatus::Standard } else { sut.torrent_status.clone() };
                let model_status_norm = if expected_state.status == TorrentStatus::Endgame { TorrentStatus::Standard } else { expected_state.status.clone() };

assert_eq!(sut_status_norm, model_status_norm, "Status Mismatch! SUT: {:?}, Model: {:?}. Total Pieces: {}, Completed Model Pieces: {}. Action: {:?}", sut.torrent_status, expected_state.status, expected_state.total_pieces, expected_state.downloaded_pieces.len(), transition);

                if !matches!(transition, Action::Cleanup) {
                    assert_eq!(sut.peers.len(), expected_state.connected_peers.len(), "Peer Count Mismatch");
                }

                sut
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig::default())]

            #[test]
            fn test_lifecycle_state_machine(
                (initial_state, transitions, tracker) in TorrentModel::sequential_strategy(20)
            ) {
                TorrentModel::test_sequential(
                    proptest::test_runner::Config::default(),
                    initial_state,
                    transitions,
                    tracker
                );
            }

            #[test]
            fn test_state_machine_network_faults(
                (initial_state, clean_actions, _) in TorrentModel::sequential_strategy(20),
                fault_entropy in proptest::collection::vec(any::<u8>(), 50)
            ) {
                let faulty_actions = inject_network_faults(clean_actions, fault_entropy);
                let mut ref_state = initial_state.clone();
                let mut sut = TorrentModel::init_test(&ref_state);

                for action in faulty_actions {
                    let sut_clone = sut.clone();
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        <TorrentModel as StateMachineTest>::apply(sut_clone, &ref_state, action.clone())
                    }));

                    match result {
                        Ok(new_sut) => {
                            sut = new_sut;
                            ref_state = <TorrentModel as ReferenceStateMachine>::apply(ref_state, &action);
                            if matches!(action, Action::Cleanup) {
                                ref_state.connected_peers = sut.peers.keys().cloned().collect();
                            }
                        }
                        Err(_) => { panic!("SUT Panicked during Network Fault Injection!\nAction: {:?}", action); }
                    }
                }
            }

            #[test]
            fn test_state_machine_network_reordering(
                (initial_state, clean_actions, _) in TorrentModel::sequential_strategy(20),
                seed in any::<u64>()
            ) {
                let chaotic_actions = inject_reordering_faults(clean_actions, seed);
                let mut ref_state = initial_state.clone();
                let mut sut = TorrentModel::init_test(&ref_state);

                for action in chaotic_actions {
                    let sut_clone = sut.clone();
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        <TorrentModel as StateMachineTest>::apply(sut_clone, &ref_state, action.clone())
                    }));

                    match result {
                        Ok(new_sut) => {
                            sut = new_sut;
                            ref_state = <TorrentModel as ReferenceStateMachine>::apply(ref_state, &action);
                            if matches!(action, Action::Cleanup) {
                                ref_state.connected_peers = sut.peers.keys().cloned().collect();
                            }
                        }
                        Err(_) => { panic!("SUT Panicked during Network Reordering!\nAction: {:?}", action); }
                    }
                }
            }
        }
    }
}
