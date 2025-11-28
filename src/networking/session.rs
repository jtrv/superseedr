// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::torrent_file::Info;
use crate::torrent_file::Torrent;

use super::protocol::{
    parse_message, writer_task, ClientExtendedId, ExtendedHandshakePayload, Message,
    MetadataMessage,
};

#[cfg(feature = "pex")]
use super::protocol::PexMessage;

use crate::token_bucket::consume_tokens;
use crate::token_bucket::TokenBucket;

use crate::command::TorrentCommand;

use std::collections::HashMap;
use std::error::Error as StdError;
use std::net::Ipv4Addr;
use std::sync::Arc;

use tokio::io::AsyncReadExt;

use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio::time::Duration;
use tokio::time::Instant;

use tracing::{event, instrument, Level};

struct DisconnectGuard {
    peer_ip_port: String,
    manager_tx: Sender<TorrentCommand>,
}

impl Drop for DisconnectGuard {
    fn drop(&mut self) {
        let _ = self
            .manager_tx
            .try_send(TorrentCommand::Disconnect(self.peer_ip_port.clone()));
    }
}

struct AbortOnDrop(JoinHandle<()>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ConnectionType {
    Outgoing,
    Incoming,
}

pub struct PeerSessionParameters {
    pub info_hash: Vec<u8>,
    pub torrent_metadata_length: Option<i64>,
    pub connection_type: ConnectionType,
    pub torrent_manager_rx: Receiver<TorrentCommand>,
    pub torrent_manager_tx: Sender<TorrentCommand>,
    pub peer_ip_port: String,
    pub client_id: Vec<u8>,
    pub global_dl_bucket: Arc<Mutex<TokenBucket>>,
    pub global_ul_bucket: Arc<Mutex<TokenBucket>>,
    pub shutdown_tx: broadcast::Sender<()>,
}

pub struct PeerSession {
    info_hash: Vec<u8>,
    peer_session_established: bool,
    torrent_metadata_length: Option<i64>,
    connection_type: ConnectionType,
    torrent_manager_rx: Receiver<TorrentCommand>,
    torrent_manager_tx: Sender<TorrentCommand>,
    client_id: Vec<u8>,
    peer_ip_port: String,

    writer_rx: Receiver<Message>,
    writer_tx: Sender<Message>,

    peer_extended_id_mappings: HashMap<String, u8>,
    peer_extended_handshake_payload: Option<ExtendedHandshakePayload>,
    peer_torrent_metadata_piece_count: usize,
    peer_torrent_metadata_pieces: Vec<u8>,

    global_dl_bucket: Arc<Mutex<TokenBucket>>,
    global_ul_bucket: Arc<Mutex<TokenBucket>>,

    shutdown_tx: broadcast::Sender<()>,
}

impl PeerSession {
    pub fn new(params: PeerSessionParameters) -> Self {
        let (writer_tx, writer_rx) = mpsc::channel::<Message>(100);

        Self {
            info_hash: params.info_hash,
            peer_session_established: false,
            torrent_metadata_length: params.torrent_metadata_length,
            connection_type: params.connection_type,
            torrent_manager_rx: params.torrent_manager_rx,
            torrent_manager_tx: params.torrent_manager_tx,
            client_id: params.client_id,
            peer_ip_port: params.peer_ip_port,
            writer_rx,
            writer_tx,
            peer_extended_id_mappings: HashMap::new(),
            peer_extended_handshake_payload: None,
            peer_torrent_metadata_piece_count: 0,
            peer_torrent_metadata_pieces: Vec::new(),
            global_dl_bucket: params.global_dl_bucket,
            global_ul_bucket: params.global_ul_bucket,
            shutdown_tx: params.shutdown_tx,
        }
    }

    #[instrument(skip(self, current_bitfield))]
    pub async fn run(
        mut self,
        stream: TcpStream,
        handshake_response: Vec<u8>,
        current_bitfield: Option<Vec<u8>>,
    ) -> Result<(), Box<dyn StdError + Send + Sync>> {
        let _guard = DisconnectGuard {
            peer_ip_port: self.peer_ip_port.clone(),
            manager_tx: self.torrent_manager_tx.clone(),
        };

        let (mut stream_read_half, stream_write_half) = stream.into_split();
        let (error_tx, mut error_rx) = oneshot::channel();

        let global_ul_bucket_clone = self.global_ul_bucket.clone();
        let writer_shutdown_rx = self.shutdown_tx.subscribe();
        let writer_handle = tokio::spawn(writer_task(
            stream_write_half,
            self.writer_rx,
            error_tx,
            global_ul_bucket_clone,
            writer_shutdown_rx,
        ));
        let _writer_abort_guard = AbortOnDrop(writer_handle);

        let handshake_response = match self.connection_type {
            ConnectionType::Outgoing => {
                let _ = self.writer_tx.try_send(Message::Handshake(
                    self.info_hash.clone(),
                    self.client_id.clone(),
                ));

                let mut buffer = vec![0u8; 68];
                stream_read_half.read_exact(&mut buffer).await?;
                buffer
            }
            ConnectionType::Incoming => {
                let _ = self.writer_tx.try_send(Message::Handshake(
                    self.info_hash.clone(),
                    self.client_id.clone(),
                ));
                handshake_response
            }
        };

        let peer_info_hash = &handshake_response[28..48];
        if self.info_hash != peer_info_hash {
            return Err("Info hash mismatch with peer".into());
        }

        let peer_id = handshake_response[48..68].to_vec();
        let _ = self
            .torrent_manager_tx
            .try_send(TorrentCommand::PeerId(self.peer_ip_port.clone(), peer_id));

        let reserved_bytes = &handshake_response[20..28];
        const EXTENSION_FLAG: u8 = 0x10;
        let peer_supports_extended = (reserved_bytes[5] & EXTENSION_FLAG) != 0;
        if peer_supports_extended {
            let mut torrent_metadata_len = None;
            if let Some(torrent_metadata_length) = self.torrent_metadata_length {
                torrent_metadata_len = Some(torrent_metadata_length);
            }
            let _ = self
                .writer_tx
                .try_send(Message::ExtendedHandshake(torrent_metadata_len));
        }

        if let Some(bitfield) = current_bitfield {
            self.peer_session_established = true;
            let _ = self.writer_tx.try_send(Message::Bitfield(bitfield.clone()));
            let _ = self
                .torrent_manager_tx
                .try_send(TorrentCommand::SuccessfullyConnected(
                    self.peer_ip_port.clone(),
                ));
        }

        let mut keep_alive_timer = tokio::time::interval(Duration::from_secs(60));
        let inactivity_timeout = tokio::time::sleep(Duration::from_secs(120));
        tokio::pin!(inactivity_timeout);

        let _result: Result<(), Box<dyn StdError + Send + Sync>> = 'session: loop {
            const READ_TIMEOUT: Duration = Duration::from_secs(2);

            tokio::select! {
                _ = &mut inactivity_timeout => {
                    break 'session Err("Peer connection timed out".into());
                },

                _ = keep_alive_timer.tick() => {
                    let _ = self.writer_tx.try_send(Message::KeepAlive);
                },

                Ok(message_from_peer) = timeout(READ_TIMEOUT, parse_message(&mut stream_read_half)) => {
                    if let Ok(ref message) = message_from_peer {
                        inactivity_timeout.as_mut().reset(Instant::now() + Duration::from_secs(120));
                        match message {
                            Message::KeepAlive => {},
                            _ => {
                                event!(Level::TRACE, ?message);
                            }
                        }
                    }

                    match message_from_peer {
                        Ok(Message::Bitfield(value)) => {
                            let _ = self.torrent_manager_tx
                                .try_send(TorrentCommand::PeerBitfield(self.peer_ip_port.clone(), value));
                        }
                        Ok(Message::NotInterested) => {}
                        Ok(Message::Interested) => {
                            let _ = self.torrent_manager_tx
                                .try_send(TorrentCommand::PeerInterested(self.peer_ip_port.clone()));
                        }
                        Ok(Message::Choke) => {
                            let _ = self.torrent_manager_tx
                                .try_send(TorrentCommand::Choke(self.peer_ip_port.clone()));
                        }
                        Ok(Message::Unchoke) => {
                            let _ = self.torrent_manager_tx
                                .try_send(TorrentCommand::Unchoke(self.peer_ip_port.clone()));
                        },
                        Ok(Message::Piece(piece_index, block_offset , block_data)) => {
                            let peer_ip_port_clone = self.peer_ip_port.clone();
                            let torrent_manager_tx_clone = self.torrent_manager_tx.clone();
                            let global_dl_bucket_clone = self.global_dl_bucket.clone();

                            // Spawn to avoid blocking the read loop with token bucket wait
                            tokio::spawn(async move {
                                consume_tokens(&global_dl_bucket_clone, block_data.len() as f64).await;
                                let _ = torrent_manager_tx_clone
                                    .send(TorrentCommand::Block(peer_ip_port_clone, piece_index, block_offset, block_data))
                                    .await;
                            });
                        },
                        Ok(Message::Request(piece_index, block_offset, block_length)) => {
                            let _ = self.torrent_manager_tx
                                .try_send(TorrentCommand::RequestUpload(self.peer_ip_port.clone(), piece_index, block_offset, block_length));
                        },
                        Ok(Message::Cancel(piece_index, block_offset, block_length)) => {
                            let _ = self.torrent_manager_tx
                                .try_send(TorrentCommand::CancelUpload(self.peer_ip_port.clone(), piece_index, block_offset, block_length));
                        },
                        Ok(Message::Have(piece_index)) => {
                            let _ = self.torrent_manager_tx
                                .try_send(TorrentCommand::Have(self.peer_ip_port.clone(), piece_index));
                        }
                        Ok(Message::Extended(extended_id, payload)) => {
                            if extended_id == ClientExtendedId::Handshake.id() {
                                if let Ok(handshake_data) = serde_bencode::from_bytes::<ExtendedHandshakePayload>(&payload) {
                                    self.peer_extended_id_mappings = handshake_data.m.clone();
                                    if !handshake_data.m.is_empty() {
                                        self.peer_extended_handshake_payload = Some(handshake_data.clone());
                                        if !self.peer_session_established {
                                            if let Some(_torrent_metadata_len) = handshake_data.metadata_size {
                                                let request = MetadataMessage {
                                                    msg_type: 0,
                                                    piece: 0,
                                                    total_size: None,
                                                };
                                                if let Ok(payload_bytes) = serde_bencode::to_bytes(&request) {
                                                    let _ = self.writer_tx.try_send(
                                                        Message::Extended(ClientExtendedId::UtMetadata.id(), payload_bytes)
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            #[cfg(feature = "pex")]
                            {
                                if extended_id == ClientExtendedId::UtPex.id() {
                                    if let Ok(pex_data) = serde_bencode::from_bytes::<PexMessage>(&payload) {
                                        let mut new_peers = Vec::new();
                                        for chunk in pex_data.added.chunks_exact(6) {
                                            let ip = Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
                                            let port = u16::from_be_bytes([chunk[4], chunk[5]]);
                                            new_peers.push((ip.to_string(), port));
                                        }
                                        if !new_peers.is_empty() {
                                            let _ = self.torrent_manager_tx
                                                .try_send(TorrentCommand::AddPexPeers(self.peer_ip_port.clone(), new_peers));
                                        }
                                    }
                                }
                            }
                            if extended_id == ClientExtendedId::UtMetadata.id() && !self.peer_session_established {
                                if let Some(ref handshake_data) = self.peer_extended_handshake_payload {
                                    if let Some(torrent_metadata_len) = handshake_data.metadata_size {
                                        let torrent_metadata_len_usize = torrent_metadata_len as usize;
                                        let current_offset = self.peer_torrent_metadata_piece_count * 16384;
                                        let expected_data_len = std::cmp::min(16384, torrent_metadata_len_usize - current_offset);
                                        let header_len = payload.len() - expected_data_len;
                                        let metadata_binary = &payload[header_len..];
                                        self.peer_torrent_metadata_pieces.extend(metadata_binary);

                                        if torrent_metadata_len_usize == self.peer_torrent_metadata_pieces.len() {
                                            let dht_info_result: Result<Info, _> = serde_bencode::from_bytes(&self.peer_torrent_metadata_pieces[..]);
                                            if let Ok(dht_info) = dht_info_result {
                                                let _ = self.torrent_manager_tx
                                                    .try_send(TorrentCommand::DhtTorrent(
                                                        Torrent {
                                                            info_dict_bencode: self.peer_torrent_metadata_pieces.clone(),
                                                            info: dht_info,
                                                            announce: None,
                                                            announce_list: None,
                                                            url_list: None,
                                                            creation_date: None,
                                                            comment: None,
                                                            created_by: None,
                                                            encoding: None
                                                        },
                                                        torrent_metadata_len
                                                    ));
                                            }
                                        } else {
                                            self.peer_torrent_metadata_piece_count += 1;
                                            let request = MetadataMessage {
                                                msg_type: 0,
                                                piece: self.peer_torrent_metadata_piece_count,
                                                total_size: None,
                                            };
                                            if let Ok(payload_bytes) = serde_bencode::to_bytes(&request) {
                                                let _ = self.writer_tx.try_send(
                                                    Message::Extended(ClientExtendedId::UtMetadata.id(), payload_bytes)
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(Message::KeepAlive) => {}
                        Ok(Message::Port(_)) => {}
                        Err(e) => {
                            break 'session Err(e.into());
                        }
                        _ => {},
                    }
                },

                Some(command) = self.torrent_manager_rx.recv() => {
                    match command {
                        #[cfg(feature = "pex")]
                        TorrentCommand::SendPexPeers(peers_list) => {
                            if let Some(pex_id) = self.peer_extended_id_mappings.get(ClientExtendedId::UtPex.as_str()).copied() {
                                let pex_list_for_this_peer: Vec<u8> = peers_list.iter()
                                    .filter(|&peer_ip| *peer_ip != self.peer_ip_port)
                                    .filter_map(|ip_port| ip_port.parse::<std::net::SocketAddr>().ok())
                                    .filter_map(|addr| {
                                        if let std::net::SocketAddr::V4(v4_addr) = addr {
                                            let mut peer_bytes = Vec::with_capacity(6);
                                            peer_bytes.extend_from_slice(&v4_addr.ip().octets());
                                            peer_bytes.extend_from_slice(&v4_addr.port().to_be_bytes());
                                            Some(peer_bytes)
                                        } else {
                                            None
                                        }
                                    })
                                    .flatten()
                                    .collect();

                                if !pex_list_for_this_peer.is_empty() {
                                    let pex_message = PexMessage {
                                        added: pex_list_for_this_peer,
                                        ..Default::default()
                                    };
                                    if let Ok(bencoded_payload) = serde_bencode::to_bytes(&pex_message) {
                                        let _ = self.writer_tx.try_send(
                                            Message::Extended(pex_id, bencoded_payload)
                                        );
                                    }
                                }
                            }
                        }
                        TorrentCommand::PeerUnchoke => {
                            let _ = self.writer_tx.try_send(Message::Unchoke);
                        }
                        TorrentCommand::PeerChoke => {
                            let _ = self.writer_tx.try_send(Message::Choke);
                        }
                        TorrentCommand::Disconnect(_) => {
                            break 'session Err("DISCONNECTING PEER".into());
                        }
                        TorrentCommand::ClientInterested => {
                            let _ = self.writer_tx.try_send(Message::Interested);
                        }
                        TorrentCommand::NotInterested => {
                            let _ = self.writer_tx.try_send(Message::NotInterested);
                        }
                        TorrentCommand::Cancel(piece_index, offset, length) => {
                            let _ = self.writer_tx.try_send(Message::Cancel(
                                piece_index,
                                offset,
                                length
                            ));
                        }
                        TorrentCommand::Have(_peer_id, piece_index) => {
                            let _ = self.writer_tx.try_send(Message::Have(piece_index));
                        }
                        TorrentCommand::RequestDownload(piece_index, block_offset, block_length) => {
                            let _ = self.writer_tx.try_send(Message::Request(
                                piece_index,
                                block_offset as u32,
                                block_length as u32,
                            ));
                        }
                        TorrentCommand::Upload(piece_index, block_offset, block_data) => {
                            let writer_tx_clone = self.writer_tx.clone();
                            let _ = writer_tx_clone.try_send(Message::Piece(
                                piece_index,
                                block_offset,
                                block_data,
                            ));
                        }
                        _ => {}
                    }
                }

                writer_error = &mut error_rx => {
                    match writer_error {
                        Ok(err) => break 'session Err(err),
                        Err(_) => break 'session Err("Writer task panicked".into()),
                    }
                },

                else => {
                    break 'session Err("ALL CHANNELS CLOSED".into());
                }
            }
        };

        Ok(())
    }
}
