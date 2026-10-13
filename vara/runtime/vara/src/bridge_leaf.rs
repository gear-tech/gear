// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! BEEFY MMR leaf-extra provider committing the destination-bound bridge lane,
//! parent time, and queue state. It runs before the current timestamp inherent
//! and before the bridge's delayed queue clear.

use crate::{GearEthBridge, Runtime, Timestamp};
use frame_support::traits::Get;
use parity_scale_codec::DecodeAll;
use sp_core::{H160, H256};
use sp_runtime::traits::{Hash, Keccak256};

pub struct VaraBridgeProvider;

fn encode_snapshot(
    bridge_domain: sp_core::H256,
    source_timestamp_ms: u64,
    snapshot: Option<(u64, sp_core::H256)>,
) -> [u8; 86] {
    let (initialized, queue_id, root) = match snapshot {
        Some((queue_id, root)) => (1, queue_id, root),
        None => (0, 0, sp_core::H256::zero()),
    };
    let mut encoded = [0u8; 86];
    encoded[0] = 2;
    encoded[1..5].copy_from_slice(b"vara");
    encoded[5..37].copy_from_slice(bridge_domain.as_bytes());
    encoded[37..45].copy_from_slice(&source_timestamp_ms.to_le_bytes());
    encoded[45] = initialized;
    encoded[46..54].copy_from_slice(&queue_id.to_le_bytes());
    encoded[54..].copy_from_slice(root.as_bytes());
    encoded
}

impl sp_consensus_beefy::mmr::BeefyDataProvider<[u8; 32]> for VaraBridgeProvider {
    fn extra_data() -> [u8; 32] {
        Keccak256::hash(&encode_snapshot(
            GearEthBridge::bridge_domain(),
            Timestamp::get(),
            GearEthBridge::bridge_snapshot(),
        ))
        .0
    }
}

/// Full source verification at bridge enablement; never a per-message scan.
pub struct BridgeReadiness;
impl Get<bool> for BridgeReadiness {
    fn get() -> bool {
        MessageReadiness::get()
            && (!pallet_gear_eth_bridge::DestinationBinding::<Runtime>::exists()
                || crate::beefy_activation::ready(256))
    }
}

/// Admission trusts native session/MMR callbacks, not future signing availability.
pub struct MessageReadiness;
impl Get<bool> for MessageReadiness {
    fn get() -> bool {
        let binding = sp_io::storage::get(
            &pallet_gear_eth_bridge::DestinationBinding::<Runtime>::hashed_key(),
        );
        let domain = match sp_io::storage::get(
            &pallet_gear_eth_bridge::BridgeDomain::<Runtime>::hashed_key(),
        ) {
            Some(bytes) => match H256::decode_all(&mut &bytes[..]) {
                Ok(domain) => domain,
                Err(_) => return false,
            },
            None => H256::zero(),
        };
        let Some(binding) = binding else {
            return domain.is_zero();
        };
        let Ok((genesis, chain_id, queue)) = <(H256, H256, H160)>::decode_all(&mut &binding[..])
        else {
            return false;
        };
        if genesis.is_zero()
            || chain_id.is_zero()
            || queue.is_zero()
            || genesis != crate::System::block_hash(0)
            || domain != GearEthBridge::destination_domain(genesis, chain_id, queue)
        {
            return false;
        }
        let Ok(Some(start)) = pallet_beefy::GenesisBlock::<Runtime>::try_get() else {
            return false;
        };
        if crate::System::block_number() <= start
            || !pallet_mmr::NumberOfLeaves::<Runtime>::try_get().is_ok_and(|count| count > 0)
            || !pallet_mmr::RootHash::<Runtime>::try_get().is_ok_and(|root| !root.is_zero())
        {
            return false;
        }
        let (Some(current_len), Some(next_len)) = (
            pallet_beefy::Authorities::<Runtime>::decode_len(),
            pallet_beefy::NextAuthorities::<Runtime>::decode_len(),
        ) else {
            return false;
        };
        if !(1..=256).contains(&current_len) || !(1..=256).contains(&next_len) {
            return false;
        }
        let Ok(current) = pallet_beefy_mmr::BeefyAuthorities::<Runtime>::try_get() else {
            return false;
        };
        let Ok(next) = pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::try_get() else {
            return false;
        };
        let Ok(set_id) = pallet_beefy::ValidatorSetId::<Runtime>::try_get() else {
            return false;
        };
        let Ok(session) = pallet_session::CurrentIndex::<Runtime>::try_get() else {
            return false;
        };
        current.len as usize == current_len
            && next.len as usize == next_len
            && !current.keyset_commitment.is_zero()
            && !next.keyset_commitment.is_zero()
            && current.id == set_id
            && current.id.checked_add(1) == Some(next.id)
            && pallet_beefy::SetIdSession::<Runtime>::try_get(set_id) == Ok(session)
            && pallet_staking::ValidatorCount::<Runtime>::try_get().is_ok_and(|count| count <= 256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_support::{
        assert_noop, assert_ok,
        storage::{storage_prefix, unhashed},
        traits::{OnInitialize, OnRuntimeUpgrade, OneSessionHandler},
    };
    use parity_scale_codec::Encode;
    use sp_consensus_beefy::mmr::BeefyDataProvider;
    use sp_runtime::BuildStorage;
    use std::str::FromStr;

    #[test]
    fn bridge_unpause_weight_fits_normal_dispatch_budget() {
        use frame_support::dispatch::{DispatchClass, GetDispatchInfo};
        let info = crate::RuntimeCall::GearEthBridge(pallet_gear_eth_bridge::Call::unpause {})
            .get_dispatch_info();
        assert_eq!(info.class, DispatchClass::Normal);
        let maximum = <Runtime as frame_system::Config>::BlockWeights::get()
            .get(DispatchClass::Normal)
            .max_extrinsic
            .unwrap();
        assert!(info.weight.all_lte(maximum));
    }

    fn bound_ext() -> sp_io::TestExternalities {
        let mut ext = sp_io::TestExternalities::new(
            crate::genesis_config_presets::local_testnet_genesis()
                .build_storage()
                .unwrap(),
        );
        ext.execute_with(|| {
            frame_system::BlockHash::<Runtime>::insert(0, H256::repeat_byte(9));
            crate::System::set_block_number(1);
            crate::Babe::on_initialize(1);
            crate::Session::rotate_session();
            let active = crate::session_history::ActiveSessionKeys::get();
            let authorities = active
                .iter()
                .map(|(owner, keys)| (owner, keys.grandpa.clone()));
            <GearEthBridge as OneSessionHandler<crate::AccountId>>::on_new_session(
                true,
                authorities.clone(),
                authorities,
            );
            crate::System::set_block_number(2);
            crate::Mmr::on_initialize(2);
            pallet_beefy::GenesisBlock::<Runtime>::put(Some(1));
            assert_ok!(GearEthBridge::bind_destination(
                crate::RuntimeOrigin::root(),
                H256::from_low_u64_be(1),
                H160::repeat_byte(3),
            ));
            assert!(MessageReadiness::get());
            assert!(BridgeReadiness::get());
        });
        ext
    }

    #[test]
    fn bridge_legacy_mode_and_inconsistent_binding_fail_closed() {
        sp_io::TestExternalities::default().execute_with(|| {
            assert!(MessageReadiness::get());
            assert!(BridgeReadiness::get());
            pallet_gear_eth_bridge::BridgeDomain::<Runtime>::put(H256::repeat_byte(1));
            assert!(!MessageReadiness::get());
            assert!(!BridgeReadiness::get());
            pallet_gear_eth_bridge::BridgeDomain::<Runtime>::kill();
            sp_io::storage::set(
                &pallet_gear_eth_bridge::DestinationBinding::<Runtime>::hashed_key(),
                &[0],
            );
            assert!(!MessageReadiness::get());
            assert!(!BridgeReadiness::get());
            pallet_gear_eth_bridge::DestinationBinding::<Runtime>::kill();
            sp_io::storage::set(
                &pallet_gear_eth_bridge::BridgeDomain::<Runtime>::hashed_key(),
                &[0],
            );
            assert!(!MessageReadiness::get());
        });
    }

    #[test]
    fn bridge_admission_requires_present_decodable_live_state() {
        bound_ext().execute_with(|| {
            let set_id = pallet_beefy::ValidatorSetId::<Runtime>::get();
            let keys = [
                pallet_beefy::GenesisBlock::<Runtime>::hashed_key().to_vec(),
                frame_support::storage::storage_prefix(b"System", b"Number").to_vec(),
                frame_system::BlockHash::<Runtime>::hashed_key_for(0),
                pallet_mmr::NumberOfLeaves::<Runtime>::hashed_key().to_vec(),
                pallet_mmr::RootHash::<Runtime>::hashed_key().to_vec(),
                pallet_beefy::Authorities::<Runtime>::hashed_key().to_vec(),
                pallet_beefy::NextAuthorities::<Runtime>::hashed_key().to_vec(),
                pallet_beefy_mmr::BeefyAuthorities::<Runtime>::hashed_key().to_vec(),
                pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::hashed_key().to_vec(),
                pallet_beefy::ValidatorSetId::<Runtime>::hashed_key().to_vec(),
                pallet_session::CurrentIndex::<Runtime>::hashed_key().to_vec(),
                pallet_beefy::SetIdSession::<Runtime>::hashed_key_for(set_id),
                pallet_staking::ValidatorCount::<Runtime>::hashed_key().to_vec(),
            ];
            for key in keys {
                let original = sp_io::storage::get(&key).unwrap();
                sp_io::storage::clear(&key);
                assert!(!MessageReadiness::get(), "missing {key:?}");
                sp_io::storage::set(&key, &[0xff]);
                assert!(!MessageReadiness::get(), "malformed {key:?}");
                sp_io::storage::set(&key, &original);
                assert!(MessageReadiness::get());
            }
        });
    }

    #[test]
    fn bridge_full_enablement_checks_are_not_repeated_per_message() {
        bound_ext().execute_with(|| {
            assert_ok!(GearEthBridge::unpause(crate::RuntimeOrigin::root()));
            crate::session_history::ActiveSessionKeys::mutate(|keys| {
                keys[0].1.beefy = crate::migrations::placeholder_beefy_key(&keys[0].0);
            });
            assert!(MessageReadiness::get());
            assert!(!BridgeReadiness::get());
            assert_noop!(
                GearEthBridge::unpause(crate::RuntimeOrigin::root()),
                pallet_gear_eth_bridge::Error::<Runtime>::BridgeNotReady
            );
        });
    }

    #[test]
    fn bridge_same_block_restart_immediately_blocks_and_refunds_bound_admission() {
        bound_ext().execute_with(|| {
            assert_ok!(GearEthBridge::unpause(crate::RuntimeOrigin::root()));
            pallet_gear_eth_bridge::TransportFee::<Runtime>::put(1_000);
            let sender = sp_keyring::AccountKeyring::Alice.to_account_id();
            assert_ok!(GearEthBridge::send_eth_message(
                crate::RuntimeOrigin::signed(sender.clone()),
                H160::repeat_byte(8),
                vec![1],
            ));
            assert_ok!(crate::Beefy::set_new_genesis(
                crate::RuntimeOrigin::root(),
                10
            ));
            assert!(!MessageReadiness::get());
            assert!(!BridgeReadiness::get());
            assert_noop!(
                GearEthBridge::send_eth_message(
                    crate::RuntimeOrigin::signed(sender),
                    H160::repeat_byte(8),
                    vec![2],
                ),
                pallet_gear_eth_bridge::Error::<Runtime>::BridgeNotReady
            );
        });
    }

    #[test]
    fn bridge_actual_and_desired_capacity_and_descriptor_failures_block_admission() {
        bound_ext().execute_with(|| {
            let current = pallet_beefy::Authorities::<Runtime>::get();
            let next = pallet_beefy::NextAuthorities::<Runtime>::get();
            let excess: frame_support::BoundedVec<_, crate::MaxAuthorities> =
                vec![current[0].clone(); 1_000].try_into().unwrap();
            pallet_beefy::Authorities::<Runtime>::put(&excess);
            assert!(!MessageReadiness::get());
            pallet_beefy::Authorities::<Runtime>::put(&current);
            pallet_beefy::NextAuthorities::<Runtime>::put(&excess);
            assert!(!MessageReadiness::get());
            pallet_beefy::NextAuthorities::<Runtime>::put(&next);
            let desired = pallet_staking::ValidatorCount::<Runtime>::get();
            pallet_staking::ValidatorCount::<Runtime>::put(257);
            assert!(!MessageReadiness::get());
            assert!(crate::beefy_activation::ready(
                crate::MaxActiveValidators::get()
            ));
            pallet_staking::ValidatorCount::<Runtime>::put(desired);
            let current_leaf = pallet_beefy_mmr::BeefyAuthorities::<Runtime>::get();
            let next_leaf = pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::get();
            for next_side in [false, true] {
                let key = if next_side {
                    pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::hashed_key()
                } else {
                    pallet_beefy_mmr::BeefyAuthorities::<Runtime>::hashed_key()
                };
                let original = if next_side {
                    next_leaf.clone()
                } else {
                    current_leaf.clone()
                };
                for case in 0..3 {
                    let mut bad = original.clone();
                    match case {
                        0 => bad.len += 1,
                        1 => bad.id += 1,
                        _ => bad.keyset_commitment = H256::zero(),
                    }
                    sp_io::storage::set(&key, &bad.encode());
                    assert!(!MessageReadiness::get());
                }
                sp_io::storage::set(&key, &original.encode());
            }
            pallet_beefy::GenesisBlock::<Runtime>::put(Some(crate::System::block_number()));
            assert!(!MessageReadiness::get());
            pallet_beefy::GenesisBlock::<Runtime>::put(Some(1));
            pallet_beefy::SetIdSession::<Runtime>::insert(
                current_leaf.id,
                crate::Session::current_index() + 1,
            );
            assert!(!MessageReadiness::get());
        });
    }

    #[test]
    fn bridge_bound_identity_and_set_id_overflow_fail_closed() {
        bound_ext().execute_with(|| {
            let binding = pallet_gear_eth_bridge::DestinationBinding::<Runtime>::get().unwrap();
            let domain = GearEthBridge::bridge_domain();
            for invalid in [
                (H256::zero(), binding.1, binding.2),
                (H256::repeat_byte(8), binding.1, binding.2),
                (binding.0, H256::zero(), binding.2),
                (binding.0, binding.1, H160::zero()),
            ] {
                pallet_gear_eth_bridge::DestinationBinding::<Runtime>::put(invalid);
                assert!(!MessageReadiness::get());
            }
            pallet_gear_eth_bridge::DestinationBinding::<Runtime>::put(binding);
            pallet_gear_eth_bridge::BridgeDomain::<Runtime>::put(H256::repeat_byte(7));
            assert!(!MessageReadiness::get());
            pallet_gear_eth_bridge::BridgeDomain::<Runtime>::put(domain);
            pallet_beefy_mmr::BeefyAuthorities::<Runtime>::mutate(|leaf| leaf.id = u64::MAX);
            pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::mutate(|leaf| leaf.id = 0);
            pallet_beefy::ValidatorSetId::<Runtime>::put(u64::MAX);
            pallet_beefy::SetIdSession::<Runtime>::insert(
                u64::MAX,
                crate::Session::current_index(),
            );
            assert!(!MessageReadiness::get());
        });
    }

    #[test]
    fn initialized_commitment_matches_wire_fixture() {
        let [
            source_genesis,
            destination_chain_id,
            destination_queue,
            bridge_domain,
            timestamp,
            initialized,
            queue_id,
            root,
            preimage,
            commitment,
            uninitialized_preimage,
            uninitialized_commitment,
        ]: [String; 12] =
            serde_json_wasm::from_str(include_str!("../tests/fixtures/bridge_commitment.json"))
                .expect("fixture is valid JSON");

        let source_genesis =
            H256::from_str(&source_genesis).expect("raw source genesis is valid hex");
        let destination_chain_id = H256::from_str(&destination_chain_id)
            .expect("destination chain id is 32-byte big-endian hex");
        let destination_queue =
            H160::from_str(&destination_queue).expect("destination queue is valid hex");
        let bridge_domain = H256::from_str(&bridge_domain).expect("bridge domain is valid hex");
        let source_timestamp_ms = timestamp.parse().expect("timestamp is valid decimal");
        let initialized: u8 = initialized.parse().expect("initialized is valid decimal");
        let queue_id = u64::from_str_radix(
            queue_id.strip_prefix("0x").expect("queue id has 0x prefix"),
            16,
        )
        .expect("queue id is valid hex");
        let root = H256::from_str(&root).expect("root is valid hex");
        let preimage = sp_core::Bytes::from_str(&preimage).expect("preimage is valid hex");
        let commitment = H256::from_str(&commitment).expect("commitment is valid hex");
        let uninitialized_preimage = sp_core::Bytes::from_str(&uninitialized_preimage)
            .expect("uninitialized preimage is valid hex");
        let uninitialized_commitment = H256::from_str(&uninitialized_commitment)
            .expect("uninitialized commitment is valid hex");

        assert_ne!(source_genesis, bridge_domain);
        assert_eq!(initialized, 1);
        assert_eq!(
            GearEthBridge::destination_domain(
                source_genesis,
                destination_chain_id,
                destination_queue
            ),
            bridge_domain,
        );
        let encoded = encode_snapshot(bridge_domain, source_timestamp_ms, Some((queue_id, root)));
        assert_eq!(encoded.as_slice(), &preimage[..]);
        assert_eq!(Keccak256::hash(&encoded), commitment);
        let uninitialized = encode_snapshot(bridge_domain, source_timestamp_ms, None);
        assert_eq!(uninitialized.as_slice(), &uninitialized_preimage[..]);
        assert_eq!(Keccak256::hash(&uninitialized), uninitialized_commitment);

        sp_io::TestExternalities::default().execute_with(|| {
            frame_system::BlockHash::<Runtime>::insert(0, source_genesis);
            pallet_timestamp::Now::<Runtime>::put(source_timestamp_ms);
            assert_ok!(GearEthBridge::bind_destination(
                crate::RuntimeOrigin::root(),
                destination_chain_id,
                destination_queue,
            ));
            assert_eq!(GearEthBridge::bridge_domain(), bridge_domain);
            assert_eq!(VaraBridgeProvider::extra_data(), uninitialized_commitment.0);

            unhashed::put(&storage_prefix(b"GearEthBridge", b"Initialized"), &true);
            unhashed::put(&storage_prefix(b"GearEthBridge", b"QueueId"), &queue_id);
            unhashed::put(&storage_prefix(b"GearEthBridge", b"QueueMerkleRoot"), &root);
            assert_eq!(GearEthBridge::bridge_snapshot(), Some((queue_id, root)));
            assert_eq!(VaraBridgeProvider::extra_data(), commitment.0);
        });
    }

    #[test]
    fn reset_preserves_bridge_domain_and_uninitialized_snapshot_is_distinct() {
        let timestamp = 1_800_000_000_000_u64;

        sp_io::TestExternalities::default().execute_with(|| {
            frame_system::BlockHash::<crate::Runtime>::insert(0, sp_core::H256::repeat_byte(0x99));
            pallet_timestamp::Now::<crate::Runtime>::put(timestamp);
            assert_ok!(GearEthBridge::bind_destination(
                crate::RuntimeOrigin::root(),
                H256::from_low_u64_be(1),
                H160::repeat_byte(3),
            ));
            let bridge_domain = GearEthBridge::bridge_domain();
            pallet_gear_eth_bridge::migrations::reset::ResetMigration::<crate::Runtime>::
                on_runtime_upgrade();

            assert!(GearEthBridge::bridge_snapshot().is_none());
            assert_eq!(GearEthBridge::bridge_domain(), bridge_domain);
            assert_eq!(
                VaraBridgeProvider::extra_data(),
                Keccak256::hash(&encode_snapshot(bridge_domain, timestamp, None)).0,
            );
            assert_ne!(
                encode_snapshot(bridge_domain, timestamp, None),
                encode_snapshot(bridge_domain, timestamp, Some((0, sp_core::H256::zero()))),
            );
        });
    }
}
