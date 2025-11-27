// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::command::TorrentCommand;
use reqwest::header::RANGE;
use tokio::sync::broadcast;
use tokio::sync::mpsc::{Receiver, Sender};

pub async fn web_seed_worker(
    url: String,
    peer_id: String,
    piece_length: u64,
    total_size: u64,
    mut peer_rx: Receiver<TorrentCommand>,
    manager_tx: Sender<TorrentCommand>,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let client = reqwest::Client::new();

    // 1. Handshake sequence to register as a "Peer"
    if manager_tx
        .send(TorrentCommand::SuccessfullyConnected(peer_id.clone()))
        .await
        .is_err()
    {
        return;
    }

    // Web seeds effectively have the whole file (Bitfield of 1s)
    let num_pieces = total_size.div_ceil(piece_length);
    let bitfield_len = num_pieces.div_ceil(8);
    let full_bitfield = vec![255u8; bitfield_len as usize];

    if manager_tx
        .send(TorrentCommand::PeerBitfield(peer_id.clone(), full_bitfield))
        .await
        .is_err()
    {
        return;
    }

    // Auto-unchoke (Web seeds don't choke)
    if manager_tx
        .send(TorrentCommand::Unchoke(peer_id.clone()))
        .await
        .is_err()
    {
        return;
    }

    // 2. Main Command Loop
    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                break;
            }
            cmd = peer_rx.recv() => {
                match cmd {
                    // UPDATED: Handle granular block requests (index, offset, length)
                    Some(TorrentCommand::RequestDownload(piece_index, block_offset_i64, block_length_i64)) => {
                        
                        let block_offset = block_offset_i64 as u64;
                        let block_length = block_length_i64 as u64;

                        // 1. Calculate Absolute Byte Range for the HTTP request
                        let piece_start = piece_index as u64 * piece_length;
                        let request_start = piece_start + block_offset;
                        let request_end = request_start + block_length - 1;
                        
                        let range_header = format!("bytes={}-{}", request_start, request_end);

                        let request = client.get(&url).header(RANGE, range_header).send();

                        // Await the Response Header (cancellable)
                        let mut response = match tokio::select! {
                            res = request => res,
                            _ = shutdown_rx.recv() => break,
                        } {
                            Ok(resp) if resp.status().is_success() => resp,
                            _ => {
                                // 404 or connection error -> Disconnect
                                let _ = manager_tx.send(TorrentCommand::Disconnect(peer_id)).await;
                                break;
                            }
                        };

                        // 3. Accumulate Body
                        // Since we requested a specific block (e.g., 16KB), we stream it 
                        // entirely into a buffer before sending it back to the manager.
                        let mut block_data = Vec::with_capacity(block_length as usize);
                        
                        let mut failed = false;
                        loop {
                            let chunk_option = tokio::select! {
                                res = response.chunk() => res,
                                _ = shutdown_rx.recv() => return, // Stop immediately on shutdown
                            };

                            match chunk_option {
                                Ok(Some(chunk)) => {
                                    block_data.extend_from_slice(&chunk);
                                }
                                Ok(None) => {
                                    break; // Stream finished
                                }
                                Err(_) => {
                                    failed = true;
                                    break;
                                }
                            }
                        }

                        if failed {
                            let _ = manager_tx.send(TorrentCommand::Disconnect(peer_id)).await;
                            break;
                        }

                        // 4. Send Block back to Manager
                        // Note: block_offset cast back to u32 for the Block command
                        if !block_data.is_empty() {
                            let send_result = manager_tx.send(TorrentCommand::Block(
                                peer_id.clone(),
                                piece_index,
                                block_offset as u32,
                                block_data,
                            )).await;

                            if send_result.is_err() {
                                return; // Manager channel closed
                            }
                        }
                    }
                    Some(TorrentCommand::Disconnect(_)) => break,
                    Some(_) => {} // Ignore Choke, Interested, etc. (Web seeds are stateless)
                    None => break, // Channel closed
                }
            }
        }
    }
}
