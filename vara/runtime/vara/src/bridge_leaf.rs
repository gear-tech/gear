// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! BEEFY MMR leaf-extra provider committing a snapshot of the Ethereum bridge queue.
//!
//! Encodes the Phase-0 wire spec:
//! `Keccak256(version(1B) || network_id(4B) || queue_id(u64 LE) || merkle_root(32B))`,
//! with a distinct version byte for the "bridge not yet initialized" case so it can't be
//! confused with the zero root the bridge writes on session clear.

use crate::GearEthBridge;
use sp_runtime::traits::{Hash, Keccak256};
use sp_std::vec::Vec;

/// Network identifier committed into the leaf, distinguishing Vara from other chains that
/// may reuse the same leaf-extra encoding.
const NETWORK_ID: [u8; 4] = *b"vara";

/// Version byte for a snapshot taken while the bridge is initialized.
const VERSION_INITIALIZED: u8 = 0x00;

/// Version byte for the "bridge not yet initialized" case — distinct from a real,
/// initialized snapshot with a zero root (e.g. right after a session-boundary clear).
const VERSION_NOT_INITIALIZED: u8 = 0xFF;

pub struct VaraBridgeProvider;

impl sp_consensus_beefy::mmr::BeefyDataProvider<[u8; 32]> for VaraBridgeProvider {
    fn extra_data() -> [u8; 32] {
        let (version, queue_id, root) = match GearEthBridge::bridge_snapshot() {
            Some((queue_id, root)) => (VERSION_INITIALIZED, queue_id, root),
            None => (VERSION_NOT_INITIALIZED, 0u64, sp_core::H256::zero()),
        };

        let mut encoded = Vec::with_capacity(1 + NETWORK_ID.len() + 8 + 32);
        encoded.push(version);
        encoded.extend_from_slice(&NETWORK_ID);
        encoded.extend_from_slice(&queue_id.to_le_bytes());
        encoded.extend_from_slice(root.as_bytes());

        Keccak256::hash(&encoded).0
    }
}
