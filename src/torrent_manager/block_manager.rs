// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::{HashMap, HashSet};

pub const BLOCK_SIZE: u32 = 16_384; 
pub const V2_HASH_LEN: usize = 32;

#[derive(Debug, Clone)]
pub struct LegacyAssembler {
    pub buffer: Vec<u8>,          // Pre-allocated flat buffer
    pub received_blocks: usize,   // Count of blocks received
    pub total_blocks: usize,      // Total expected blocks
    pub mask: Vec<bool>,          // Tracks which blocks are filled
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockAddress {
    pub piece_index: u32,
    pub block_index: u32,
    pub byte_offset: u32,
    pub global_offset: u64,
    pub length: u32,
}

#[derive(Debug, PartialEq)]
pub enum BlockResult {
    Accepted,
    Duplicate,
    V1BlockBuffered,
    V1PieceVerified { piece_index: u32, data: Vec<u8> },
}

#[derive(Debug, PartialEq)]
pub enum BlockDecision {
    VerifyV2 { root_hash: [u8; 32], proof: Vec<[u8; 32]> },
    BufferV1,
    Duplicate,
    Error,
}

#[derive(Default, Debug, Clone)]
pub struct BlockManager {
    // --- STATE ---
    pub block_bitfield: Vec<bool>,
    pub pending_blocks: HashSet<u32>,
    pub piece_rarity: HashMap<u32, usize>,

    // --- METADATA ---
    pub piece_hashes_v1: Vec<[u8; 20]>,
    pub file_merkle_roots: HashMap<usize, [u8; 32]>, 
    
    // --- V1 COMPATIBILITY ---
    pub legacy_buffers: HashMap<u32, LegacyAssembler>,

    // --- GEOMETRY ---
    pub piece_length: u32,
    pub total_length: u64,
    pub total_blocks: u32,
}

impl BlockManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_geometry(
        &mut self, 
        piece_length: u32, 
        total_length: u64, 
        v1_hashes: Vec<[u8; 20]>,
        v2_roots: HashMap<usize, [u8; 32]>,
        validation_complete: bool
    ) {
        self.piece_length = piece_length;
        self.total_length = total_length;
        self.piece_hashes_v1 = v1_hashes;
        self.file_merkle_roots = v2_roots;
        self.total_blocks = (total_length as f64 / BLOCK_SIZE as f64).ceil() as u32;
        self.block_bitfield = vec![validation_complete; self.total_blocks as usize];
    }

    // --- WORK SELECTION ---
    pub fn pick_blocks_for_peer(
        &self,
        peer_bitfield: &[bool],
        count: usize,
        rarest_pieces: &[u32],
        endgame_mode: bool, // <--- NEW ARGUMENT
    ) -> Vec<BlockAddress> {
        let mut picked = Vec::with_capacity(count);

        for &piece_idx in rarest_pieces {
            if picked.len() >= count { break; }

            // Skip if peer doesn't have it
            if !peer_bitfield.get(piece_idx as usize).unwrap_or(&false) {
                continue;
            }

            let (start_blk, end_blk) = self.get_block_range(piece_idx);

            for global_idx in start_blk..end_blk {
                if picked.len() >= count { break; }

                let already_have = self.block_bitfield.get(global_idx as usize).copied().unwrap_or(true);
                let is_pending = self.pending_blocks.contains(&global_idx);

                if !already_have {
                    if !is_pending || endgame_mode {
                        picked.push(self.inflate_address(global_idx));
                    }
                }
            }
        }
        picked
    }
    pub fn mark_pending(&mut self, global_idx: u32) {
        self.pending_blocks.insert(global_idx);
    }

    pub fn unmark_pending(&mut self, global_idx: u32) {
        self.pending_blocks.remove(&global_idx);
    }

    // --- STATE COMMITMENT ---

    pub fn commit_verified_block(&mut self, addr: BlockAddress) -> BlockResult {
        let global_idx = self.flatten_address(addr);

        if global_idx as usize >= self.block_bitfield.len() {
             return BlockResult::Duplicate; 
        }

        if self.block_bitfield[global_idx as usize] {
            return BlockResult::Duplicate;
        }

        self.block_bitfield[global_idx as usize] = true;
        self.pending_blocks.remove(&global_idx);

        BlockResult::Accepted
    }

    // --- GEOMETRY HELPERS ---
    fn blocks_in_piece(&self, piece_len: u32) -> u32 {
        // Equivalent to ceil(len / 16384) but using pure integers
        (piece_len + BLOCK_SIZE - 1) / BLOCK_SIZE
    }
    pub fn get_block_range(&self, piece_idx: u32) -> (u32, u32) {
        let piece_len = self.calculate_piece_size(piece_idx);
        // USE SAFE MATH
        let blocks_in_piece = self.blocks_in_piece(piece_len);
        
        let piece_start_offset = piece_idx as u64 * self.piece_length as u64;
        let start_blk = (piece_start_offset / BLOCK_SIZE as u64) as u32;
        
        (start_blk, start_blk + blocks_in_piece)
    }

    fn calculate_piece_size(&self, piece_idx: u32) -> u32 {
        let offset = piece_idx as u64 * self.piece_length as u64;
        let remaining = self.total_length.saturating_sub(offset);
        std::cmp::min(self.piece_length as u64, remaining) as u32
    }

    pub fn inflate_address(&self, global_idx: u32) -> BlockAddress {
        let global_offset = global_idx as u64 * BLOCK_SIZE as u64;
        let piece_index = (global_offset / self.piece_length as u64) as u32;
        let byte_offset_in_piece = (global_offset % self.piece_length as u64) as u32;
        
        let remaining_len = self.total_length.saturating_sub(global_offset);
        let length = std::cmp::min(BLOCK_SIZE as u64, remaining_len) as u32;

        BlockAddress {
            piece_index,
            block_index: (byte_offset_in_piece / BLOCK_SIZE),
            byte_offset: byte_offset_in_piece,
            global_offset,
            length,
        }
    }

    pub fn flatten_address(&self, addr: BlockAddress) -> u32 {
        (addr.global_offset / BLOCK_SIZE as u64) as u32
    }

    /// V1 HELPER: Check if a full piece is complete.
    pub fn is_piece_complete(&self, piece_index: u32) -> bool {
        let (start, end) = self.get_block_range(piece_index);
        for i in start..end {
            if !self.block_bitfield.get(i as usize).copied().unwrap_or(false) {
                return false;
            }
        }
        true
    }

    /// V1 HELPER: Buffer a block for legacy assembly.
    pub fn handle_v1_block_buffering(&mut self, addr: BlockAddress, data: &[u8]) -> Option<Vec<u8>> {
        let piece_len = self.calculate_piece_size(addr.piece_index);
        let num_blocks = self.blocks_in_piece(piece_len);

        // Get or Create Assembler
        let assembler = self.legacy_buffers.entry(addr.piece_index).or_insert_with(|| {
             LegacyAssembler {
                 buffer: vec![0u8; piece_len as usize],
                 received_blocks: 0,
                 total_blocks: num_blocks as usize,
                 mask: vec![false; num_blocks as usize],
             }
        });

        // Write Data (Flat Copy)
        let offset = addr.byte_offset as usize;
        let end = offset + data.len();
        
        // Safety Check
        if end <= assembler.buffer.len() && !assembler.mask[addr.block_index as usize] {
            assembler.buffer[offset..end].copy_from_slice(data);
            assembler.mask[addr.block_index as usize] = true;
            assembler.received_blocks += 1;
        }

        // Check Completion
        if assembler.received_blocks == assembler.total_blocks {
             if let Some(finished) = self.legacy_buffers.remove(&addr.piece_index) {
                 return Some(finished.buffer);
             }
        }
        None
    }

    pub fn inflate_address_from_overlay(
        &self, 
        piece_index: u32, 
        byte_offset: u32, 
        length: u32
    ) -> Option<BlockAddress> { // <--- Returns Option now
        
        let piece_len = self.calculate_piece_size(piece_index);

        // SECURITY GUARD: Ensure the block fits INSIDE the piece boundaries.
        // This prevents "Overlay Attacks" where offset points to a different piece.
        if byte_offset.saturating_add(length) > piece_len {
            return None;
        }

        let piece_start = piece_index as u64 * self.piece_length as u64;
        let global_offset = piece_start + byte_offset as u64;
        
        Some(BlockAddress {
            piece_index,
            block_index: byte_offset / BLOCK_SIZE,
            byte_offset,
            global_offset,
            length,
        })
    }

    pub fn total_pieces(&self) -> usize {
        self.piece_hashes_v1.len()
    }

    pub fn handle_incoming_block_decision(&self, addr: BlockAddress) -> BlockDecision {
        let global_idx = self.flatten_address(addr);

        if global_idx as usize >= self.block_bitfield.len() {
            return BlockDecision::Error;
        }
        if self.block_bitfield[global_idx as usize] {
            return BlockDecision::Duplicate;
        }

        if let Some(root) = self.get_root_for_offset(addr.global_offset) {
             return BlockDecision::VerifyV2 { 
                 root_hash: root, 
                 proof: Vec::new() 
             };
        }

        BlockDecision::BufferV1
    }

    pub fn update_rarity<'a, I>(&mut self, peer_bitfields: I)
    where
        I: Iterator<Item = &'a Vec<bool>>,
    {
        self.piece_rarity.clear();
        for bitfield in peer_bitfields {
            for (index, &has_piece) in bitfield.iter().enumerate() {
                if has_piece {
                    *self.piece_rarity.entry(index as u32).or_insert(0) += 1;
                }
            }
        }
    }

    pub fn release_pending_blocks_for_peer(&mut self, pending: &HashSet<BlockAddress>) {
        for addr in pending {
            let global_idx = self.flatten_address(*addr);
            self.unmark_pending(global_idx);
        }
    }

    pub fn get_rarest_pieces(&self) -> Vec<u32> {
        let mut pieces: Vec<u32> = (0..self.total_pieces() as u32).collect();
        pieces.retain(|&idx| !self.is_piece_complete(idx));
        pieces.sort_by_key(|idx| self.piece_rarity.get(idx).copied().unwrap_or(0));
        pieces
    }

    pub fn commit_v1_piece(&mut self, piece_index: u32) {
        let (start, end) = self.get_block_range(piece_index);
        for global_idx in start..end {
            if (global_idx as usize) < self.block_bitfield.len() {
                self.block_bitfield[global_idx as usize] = true;
            }
            self.pending_blocks.remove(&global_idx);
        }
        self.legacy_buffers.remove(&piece_index);
    }

    /// Reverts a piece status to Incomplete (e.g. after Disk Write Failure)
    pub fn revert_v1_piece_completion(&mut self, piece_index: u32) {
        let (start, end) = self.get_block_range(piece_index);
        for global_idx in start..end {
            if (global_idx as usize) < self.block_bitfield.len() {
                self.block_bitfield[global_idx as usize] = false;
            }
        }
        // Note: we don't restore pending_blocks because we want them to be picked up again
    }

    pub fn reset_v1_buffer(&mut self, piece_index: u32) {
        self.legacy_buffers.remove(&piece_index);
    }

    // Helper to map global offset to a file's merkle root
    fn get_root_for_offset(&self, _offset: u64) -> Option<[u8; 32]> {
        None 
    }
}
