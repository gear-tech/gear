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

/// Network identifier committed into the leaf, distinguishing Vara from other chains that
/// may reuse the same leaf-extra encoding.
const NETWORK_ID: [u8; 4] = *b"vara";

/// `version(1B) || network_id(4B) || queue_id(8B) || merkle_root(32B)`.
const ENCODED_LEN: usize = 1 + NETWORK_ID.len() + 8 + 32;

/// Version byte for a snapshot taken while the bridge is initialized.
const VERSION_INITIALIZED: u8 = 0x00;

/// Version byte for the "bridge not yet initialized" case — distinct from a real,
/// initialized snapshot with a zero root (e.g. right after a session-boundary clear).
const VERSION_NOT_INITIALIZED: u8 = 0xFF;

pub struct VaraBridgeProvider;
fn encode_snapshot(snapshot: Option<(u64, sp_core::H256)>) -> [u8; ENCODED_LEN] {
    let (version, queue_id, root) = match snapshot {
        Some((queue_id, root)) => (VERSION_INITIALIZED, queue_id, root),
        None => (VERSION_NOT_INITIALIZED, 0, sp_core::H256::zero()),
    };

    let mut encoded = [0u8; ENCODED_LEN];
    encoded[0] = version;
    encoded[1..5].copy_from_slice(&NETWORK_ID);
    encoded[5..13].copy_from_slice(&queue_id.to_le_bytes());
    encoded[13..].copy_from_slice(root.as_bytes());
    encoded
}

impl sp_consensus_beefy::mmr::BeefyDataProvider<[u8; 32]> for VaraBridgeProvider {
    fn extra_data() -> [u8; 32] {
        Keccak256::hash(&encode_snapshot(GearEthBridge::bridge_snapshot())).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_support::traits::OnRuntimeUpgrade;
    use sp_consensus_beefy::mmr::BeefyDataProvider;
    use std::str::FromStr;

    #[test]
    fn initialized_commitment_matches_wire_fixture() {
        let [queue_id, root, preimage, commitment]: [String; 4] =
            serde_json_wasm::from_str(include_str!("../tests/fixtures/bridge_commitment.json"))
                .expect("fixture is valid JSON");
        let queue_id = u64::from_str_radix(
            queue_id.strip_prefix("0x").expect("queue id has 0x prefix"),
            16,
        )
        .expect("queue id is valid hex");
        let root = sp_core::Bytes::from_str(&root).expect("root is valid hex");
        let preimage = sp_core::Bytes::from_str(&preimage).expect("preimage is valid hex");
        let commitment = sp_core::Bytes::from_str(&commitment).expect("commitment is valid hex");

        let encoded = encode_snapshot(Some((queue_id, sp_core::H256::from_slice(&root))));
        assert_eq!(encoded.as_slice(), &preimage[..]);
        assert_eq!(Keccak256::hash(&encoded).as_bytes(), &commitment[..]);
    }

    #[test]
    fn root_without_initialization_uses_uninitialized_commitment() {
        sp_io::TestExternalities::default().execute_with(|| {
            pallet_gear_eth_bridge::migrations::reset::ResetMigration::<crate::Runtime>::
                on_runtime_upgrade();

            assert!(GearEthBridge::bridge_snapshot().is_none());
            assert_eq!(
                VaraBridgeProvider::extra_data(),
                sp_core::H256::from_str(
                    "0xc2fe11d2d2b8e3dc8102033a2dad52ad4a61e6e15e6037273f9132a85293c2de"
                )
                .expect("commitment is valid hex")
                .0
            );
        });
    }
}
