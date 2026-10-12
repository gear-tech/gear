// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Ownership commitments use activated keys, never mutable registrations for future sessions.

use crate::{
    AccountId, Historical, Runtime, Session, SessionKeys, Staking, pallet_session_historical,
};
use frame_support::{Twox64Concat, pallet_prelude::ValueQuery, traits::KeyOwnerProofSystem};
use parity_scale_codec::{DecodeAll, Encode};
use sp_core::crypto::KeyTypeId;
use sp_runtime::traits::{BlakeTwo256, Hash, OpaqueKeys};
use sp_std::prelude::*;
use sp_trie::{
    LayoutV0, MemoryDB, Recorder, StorageProof, Trie, TrieConfiguration, TrieDBBuilder, TrieMut,
    accessed_nodes_tracker::AccessedNodesTracker, recorder_ext::RecorderExt,
    trie_types::TrieDBMutBuilderV0,
};

pub(crate) type HistoricalRoot = (sp_core::H256, u32);

#[frame_support::storage_alias]
pub(crate) type ActiveSessionKeys =
    StorageValue<Historical, Vec<(AccountId, SessionKeys)>, ValueQuery>;

#[frame_support::storage_alias]
pub(crate) type LegacySessionRoots = StorageMap<Historical, Twox64Concat, u32, HistoricalRoot>;

fn entries<'a>(
    keys: &'a [(AccountId, SessionKeys)],
    era: Option<u32>,
    key_ids: &'static [KeyTypeId],
) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + 'a {
    era.into_iter().flat_map(move |era| {
        keys.iter()
            .enumerate()
            .flat_map(move |(index, (account, keys))| {
                let index = index as u32;
                key_ids
                    .iter()
                    .map(move |kind| ((*kind, keys.get_raw(*kind)).encode(), index.encode()))
                    .chain(core::iter::once_with(move || {
                        (
                            index.encode(),
                            (account, Staking::eras_stakers(era, account)).encode(),
                        )
                    }))
            })
    })
}

pub(crate) fn root(keys: &[(AccountId, SessionKeys)], era: Option<u32>) -> HistoricalRoot {
    root_with_key_ids(keys, era, SessionKeys::key_ids())
}

pub(crate) fn root_with_key_ids(
    keys: &[(AccountId, SessionKeys)],
    era: Option<u32>,
    key_ids: &'static [KeyTypeId],
) -> HistoricalRoot {
    (
        // Avoid a monolithic SCALE host-call buffer for large exposure histories.
        LayoutV0::<BlakeTwo256>::trie_root(entries(keys, era, key_ids)),
        if era.is_some() { keys.len() as u32 } else { 0 },
    )
}

/// Delegates validator selection unchanged; commits the exact keys when they become active.
pub struct SessionManager;

impl pallet_session::SessionManager<AccountId> for SessionManager {
    fn new_session(index: u32) -> Option<Vec<AccountId>> {
        <Staking as pallet_session::SessionManager<AccountId>>::new_session(index)
    }

    fn new_session_genesis(index: u32) -> Option<Vec<AccountId>> {
        <Staking as pallet_session::SessionManager<AccountId>>::new_session_genesis(index)
    }

    fn start_session(index: u32) {
        <Staking as pallet_session::SessionManager<AccountId>>::start_session(index);
        // Session has activated this queue, but has not overwritten it with future registrations.
        let keys = pallet_session::QueuedKeys::<Runtime>::get();
        let root = root(&keys, Staking::active_era().map(|era| era.index));
        ActiveSessionKeys::put(keys);
        pallet_session_historical::HistoricalSessions::<Runtime>::insert(index, root);
        pallet_session_historical::StoredRange::<Runtime>::mutate(|range| {
            let end = index.checked_add(1).expect("session index cannot overflow");
            match range {
                Some((_, previous_end)) => *previous_end = (*previous_end).max(end),
                None => *range = Some((index, end)),
            }
        });
        for legacy in LegacySessionRoots::iter_keys() {
            if !pallet_session_historical::HistoricalSessions::<Runtime>::contains_key(legacy) {
                LegacySessionRoots::remove(legacy);
            }
        }
    }

    fn end_session(index: u32) {
        pallet_session_historical::onchain::store_session_validator_set_to_offchain::<Runtime>(
            index,
        );
        <Staking as pallet_session::SessionManager<AccountId>>::end_session(index);
    }
}

/// Both new and migration-boundary proofs are checked against immutable session commitments.
pub struct SessionKeyOwnerProof;

impl<D: AsRef<[u8]>> KeyOwnerProofSystem<(KeyTypeId, D)> for SessionKeyOwnerProof {
    type Proof = sp_session::MembershipProof;
    type IdentificationTuple = pallet_session_historical::IdentificationTuple<Runtime>;

    fn prove(key: (KeyTypeId, D)) -> Option<Self::Proof> {
        let session = Session::current_index();
        let (expected_root, validator_count) = Historical::historical_root(session)?;
        let keys = ActiveSessionKeys::get();
        let entries = entries(
            &keys,
            Staking::active_era().map(|era| era.index),
            SessionKeys::key_ids(),
        );
        let mut db = MemoryDB::<BlakeTwo256>::default();
        let mut root = Default::default();
        {
            let mut trie = TrieDBMutBuilderV0::<BlakeTwo256>::new(&mut db, &mut root).build();
            for (key, value) in entries {
                trie.insert(&key, &value).ok()?;
            }
        }
        if root != expected_root {
            return None;
        }
        let mut recorder = Recorder::<LayoutV0<BlakeTwo256>>::new();
        {
            let trie = TrieDBBuilder::<LayoutV0<BlakeTwo256>>::new(&db, &root)
                .with_recorder(&mut recorder)
                .build();
            let index = trie.get(&(key.0, key.1.as_ref()).encode()).ok()??;
            trie.get(&index).ok()??;
        }
        Some(sp_session::MembershipProof {
            session,
            validator_count,
            trie_nodes: recorder.into_raw_storage_proof(),
        })
    }

    fn check_proof(key: (KeyTypeId, D), proof: Self::Proof) -> Option<Self::IdentificationTuple> {
        if proof.session > Session::current_index() {
            return None;
        }
        let mut commitment = Historical::historical_root(proof.session)?;
        if let Some(legacy) = LegacySessionRoots::get(proof.session)
            && proof
                .trie_nodes
                .iter()
                .any(|node| BlakeTwo256::hash(node) == legacy.0)
        {
            commitment = legacy;
        }
        let (root, count) = commitment;
        if count != proof.validator_count {
            return None;
        }
        let proof = StorageProof::new_with_duplicate_nodes_check(proof.trie_nodes).ok()?;
        let mut tracker = AccessedNodesTracker::new(proof.len());
        let db = proof.into_memory_db::<BlakeTwo256>();
        let trie = TrieDBBuilder::<LayoutV0<BlakeTwo256>>::new(&db, &root)
            .with_recorder(&mut tracker)
            .build();
        let index = trie.get(&(key.0, key.1.as_ref()).encode()).ok()??;
        if u32::decode_all(&mut &index[..]).ok()? >= count {
            return None;
        }
        let owner = trie.get(&index).ok()??;
        let owner = Self::IdentificationTuple::decode_all(&mut &owner[..]).ok()?;
        tracker.ensure_no_unused_nodes().ok()?;
        Some(owner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BuildStorage, RuntimeOrigin, System,
        genesis_config_presets::{authority_keys_from_seed, local_testnet_genesis},
    };
    use frame_support::traits::{OnFinalize, OnInitialize};
    use sp_runtime::RuntimeAppPublic;

    fn ownership_api(method: &str, context: u64, key: &[u8]) -> Option<Vec<u8>> {
        let input = [context.encode(), key.to_vec()].concat();
        let output = crate::api::dispatch(method, &input).unwrap();
        Option::<Vec<u8>>::decode_all(&mut output.as_slice()).unwrap()
    }

    fn assert_authorities(
        active: &[(AccountId, SessionKeys)],
        queued: &[(AccountId, SessionKeys)],
    ) {
        use crate::{Babe, Grandpa};

        assert_eq!(
            Session::validators(),
            active
                .iter()
                .map(|(owner, _)| owner.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(pallet_session::QueuedKeys::<Runtime>::get(), queued);
        assert_eq!(
            Babe::authorities()
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>(),
            active
                .iter()
                .map(|(_, keys)| keys.babe.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            Grandpa::grandpa_authorities()
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>(),
            active
                .iter()
                .map(|(_, keys)| keys.grandpa.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            pallet_im_online::Keys::<Runtime>::get().to_vec(),
            active
                .iter()
                .map(|(_, keys)| keys.im_online.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            crate::AuthorityDiscovery::current_authorities().to_vec(),
            active
                .iter()
                .map(|(_, keys)| keys.authority_discovery.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            crate::AuthorityDiscovery::next_authorities().to_vec(),
            queued
                .iter()
                .map(|(_, keys)| keys.authority_discovery.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            pallet_beefy::Authorities::<Runtime>::get().to_vec(),
            active
                .iter()
                .map(|(_, keys)| keys.beefy.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            pallet_beefy::NextAuthorities::<Runtime>::get().to_vec(),
            queued
                .iter()
                .map(|(_, keys)| keys.beefy.clone())
                .collect::<Vec<_>>()
        );
    }

    fn proofs(keys: &SessionKeys) -> Vec<(KeyTypeId, Vec<u8>, sp_session::MembershipProof)> {
        SessionKeys::key_ids()
            .iter()
            .map(|kind| {
                let raw = keys.get_raw(*kind).to_vec();
                let proof = SessionKeyOwnerProof::prove((*kind, &raw)).unwrap();
                let api = match *kind {
                    sp_consensus_babe::KEY_TYPE => Some((
                        "BabeApi_generate_key_ownership_proof",
                        u64::from(crate::Babe::current_slot()),
                    )),
                    sp_consensus_grandpa::KEY_TYPE => Some((
                        "GrandpaApi_generate_key_ownership_proof",
                        crate::Grandpa::current_set_id(),
                    )),
                    sp_consensus_beefy::KEY_TYPE => Some((
                        "BeefyApi_generate_key_ownership_proof",
                        crate::Beefy::validator_set().unwrap().id(),
                    )),
                    _ => None,
                };
                if let Some((method, context)) = api {
                    assert_eq!(ownership_api(method, context, &raw), Some(proof.encode()));
                    if *kind != sp_consensus_babe::KEY_TYPE {
                        assert!(ownership_api(method, context + 1, &raw).is_none());
                    }
                }
                (*kind, raw, proof)
            })
            .collect()
    }

    fn assert_owner(
        proofs: &[(KeyTypeId, Vec<u8>, sp_session::MembershipProof)],
        owner: &AccountId,
    ) {
        for (kind, raw, proof) in proofs {
            assert_eq!(
                SessionKeyOwnerProof::check_proof((*kind, raw), proof.clone())
                    .unwrap()
                    .0,
                *owner
            );
        }
    }

    #[test]
    fn registered_keys_never_replace_active_ownership_before_queue_activation() {
        for beefy_only in [true, false] {
            let mut ext =
                sp_io::TestExternalities::new(local_testnet_genesis().build_storage().unwrap());
            ext.register_extension(sp_keystore::KeystoreExt::new(
                sp_keystore::testing::MemoryKeystore::new(),
            ));
            ext.execute_with(|| {
                System::set_block_number(1);
                frame_system::BlockHash::<Runtime>::insert(0, sp_core::H256::repeat_byte(7));
                let (owner, ..) = authority_keys_from_seed("Alice");
                let controller = pallet_staking::Bonded::<Runtime>::get(&owner).unwrap();
                let old = pallet_session::NextKeys::<Runtime>::get(&owner).unwrap();
                let original = proofs(&old);
                let initial = pallet_session::QueuedKeys::<Runtime>::get();
                let mut rotated = initial.clone();
                let initial_set_id = crate::Beefy::validator_set().unwrap().id();
                assert_authorities(&initial, &initial);
                let old_root = Historical::historical_root(Session::current_index()).unwrap();
                let mut replacement =
                    SessionKeys::generate(&controller.encode(), Some(b"//Alice".to_vec())).keys;
                assert_eq!(replacement, old);
                if beefy_only {
                    replacement.beefy =
                        crate::BeefyId::generate_pair(Some(b"//replacement".to_vec()));
                } else {
                    replacement = SessionKeys::generate(
                        &controller.encode(),
                        Some(b"//replacement".to_vec()),
                    )
                    .keys;
                }
                rotated
                    .iter_mut()
                    .find(|(account, _)| account == &owner)
                    .unwrap()
                    .1 = replacement.clone();
                let proof = replacement
                    .create_ownership_proof(&controller.encode())
                    .unwrap()
                    .encode();
                Session::set_keys(
                    RuntimeOrigin::signed(controller.clone()),
                    replacement.clone(),
                    proof,
                )
                .unwrap();
                assert_eq!(
                    Historical::historical_root(Session::current_index()),
                    Some(old_root)
                );
                assert_eq!(proofs(&old), original);
                assert!(
                    SessionKeyOwnerProof::prove((sp_consensus_beefy::KEY_TYPE, &replacement.beefy))
                        .is_none()
                );
                assert_owner(&original, &owner);
                crate::Babe::on_initialize(System::block_number());
                Session::rotate_session();
                crate::Grandpa::on_finalize(System::block_number());
                assert_authorities(&initial, &rotated);
                assert_eq!(proofs(&old)[0].2.session, 1);
                assert_owner(&original, &owner);
                assert!(
                    SessionKeyOwnerProof::prove((sp_consensus_beefy::KEY_TYPE, &replacement.beefy))
                        .is_none()
                );
                System::set_block_number(2);
                crate::Babe::on_initialize(System::block_number());
                Session::rotate_session();
                crate::Grandpa::on_finalize(System::block_number());
                assert_authorities(&rotated, &rotated);
                let rotated_set_id = crate::Beefy::validator_set().unwrap().id();
                assert!(rotated_set_id > initial_set_id);
                assert!(
                    ownership_api(
                        "BeefyApi_generate_key_ownership_proof",
                        rotated_set_id,
                        old.beefy.as_ref()
                    )
                    .is_none()
                );
                if !beefy_only {
                    assert!(
                        ownership_api(
                            "BabeApi_generate_key_ownership_proof",
                            u64::from(crate::Babe::current_slot()),
                            old.babe.as_ref()
                        )
                        .is_none()
                    );
                    assert!(
                        ownership_api(
                            "GrandpaApi_generate_key_ownership_proof",
                            crate::Grandpa::current_set_id(),
                            old.grandpa.as_ref()
                        )
                        .is_none()
                    );
                }
                let activated = proofs(&replacement);
                assert_owner(&activated, &owner);
                assert_owner(&original, &owner);
                assert!(
                    SessionKeyOwnerProof::prove((sp_consensus_beefy::KEY_TYPE, &old.beefy))
                        .is_none()
                );
                assert_eq!(Staking::active_era().unwrap().index, 0);
                // Removing a future registration must not erase the active or already queued owner.
                Session::purge_keys(RuntimeOrigin::signed(controller)).unwrap();
                assert!(pallet_session::NextKeys::<Runtime>::get(&owner).is_none());
                assert_owner(&proofs(&replacement), &owner);
                System::set_block_number(3);
                crate::Babe::on_initialize(System::block_number());
                Session::rotate_session();
                crate::Grandpa::on_finalize(System::block_number());
                let remaining: Vec<_> = rotated
                    .iter()
                    .filter(|(account, _)| account != &owner)
                    .cloned()
                    .collect();
                assert_authorities(&rotated, &remaining);
                assert_owner(&proofs(&replacement), &owner);
                System::set_block_number(4);
                crate::Babe::on_initialize(System::block_number());
                Session::rotate_session();
                crate::Grandpa::on_finalize(System::block_number());
                assert_authorities(&remaining, &remaining);
                let purged_set_id = crate::Beefy::validator_set().unwrap().id();
                assert!(purged_set_id > rotated_set_id);
                for (method, context, key) in [
                    (
                        "BabeApi_generate_key_ownership_proof",
                        u64::from(crate::Babe::current_slot()),
                        replacement.babe.as_ref(),
                    ),
                    (
                        "GrandpaApi_generate_key_ownership_proof",
                        crate::Grandpa::current_set_id(),
                        replacement.grandpa.as_ref(),
                    ),
                    (
                        "BeefyApi_generate_key_ownership_proof",
                        purged_set_id,
                        replacement.beefy.as_ref(),
                    ),
                ] {
                    assert!(ownership_api(method, context, key).is_none());
                }
                assert!(
                    SessionKeyOwnerProof::prove((sp_consensus_beefy::KEY_TYPE, &replacement.beefy))
                        .is_none()
                );
                assert_owner(&activated, &owner);
                assert_owner(&original, &owner);
                let mut future = activated[0].2.clone();
                future.session = Session::current_index() + 1;
                assert!(
                    SessionKeyOwnerProof::check_proof((activated[0].0, &activated[0].1), future)
                        .is_none()
                );
                Historical::prune_up_to(3);
                for (kind, raw, proof) in original.iter().chain(&activated) {
                    assert!(
                        SessionKeyOwnerProof::check_proof((*kind, raw), proof.clone()).is_none()
                    );
                }
            });
        }
    }
}
