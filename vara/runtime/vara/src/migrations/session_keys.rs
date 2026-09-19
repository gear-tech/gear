// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! One-off migration translating the pre-BEEFY `SessionKeys` (4 keys) into the current
//! 5-key struct, appending a deterministic placeholder `beefy` key per validator.
//!
//! Required alongside the runtime composition change in `lib.rs`: appending `beefy` to
//! `SessionKeys` changes the SCALE encoding of every already-stored `NextKeys`/`QueuedKeys`
//! entry, so existing validators' stored keys fail to decode without this migration running
//! in the same runtime upgrade. Safe because BEEFY stays inactive (`genesis_block: None`)
//! through Phase 1/2, so the placeholder key never has to sign anything.

use crate::{AuthorityDiscovery, Babe, BeefyId, Grandpa, ImOnline, Runtime, SessionKeys};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use parity_scale_codec::Encode;
use sp_runtime::{
    impl_opaque_keys,
    traits::{Hash, Keccak256},
};
use sp_std::prelude::*;

#[cfg(feature = "try-runtime")]
use {
    frame_support::ensure,
    parity_scale_codec::Decode,
    sp_runtime::{TryRuntimeError, traits::OpaqueKeys},
};

const MIGRATION_SPEC_VERSION: u32 = 2_01_00;

impl_opaque_keys! {
    /// Mirrors `SessionKeys` as it existed before BEEFY was added.
    pub struct SessionKeysOld {
        pub babe: Babe,
        pub grandpa: Grandpa,
        pub im_online: ImOnline,
        pub authority_discovery: AuthorityDiscovery,
    }
}

pub struct MigrateSessionKeys;

impl OnRuntimeUpgrade for MigrateSessionKeys {
    fn on_runtime_upgrade() -> Weight {
        let db_weight = <Runtime as frame_system::Config>::DbWeight::get();
        let mut weight = db_weight.reads(1);

        if !should_migrate() {
            return weight;
        }

        let validators_migrated = pallet_session::NextKeys::<Runtime>::iter_keys().count() as u64;
        weight = weight.saturating_add(db_weight.reads(validators_migrated));

        let queued_keys = frame_support::storage::unhashed::get::<
            Vec<(crate::AccountId, SessionKeysOld)>,
        >(&pallet_session::QueuedKeys::<Runtime>::hashed_key())
        .map_or(0, |keys| keys.len() as u64);
        weight = weight.saturating_add(db_weight.reads(1));

        pallet_session::Pallet::<Runtime>::upgrade_keys::<SessionKeysOld, _>(migrate_keys);

        // `upgrade_keys` reads and rewrites every `NextKeys` entry, removes four old
        // ownership mappings, installs five new mappings, and translates `QueuedKeys`.
        weight = weight.saturating_add(db_weight.reads_writes(
            validators_migrated.saturating_add(1),
            validators_migrated.saturating_mul(10).saturating_add(1),
        ));
        // Conservatively charge one database read per placeholder derivation; a read is
        // more expensive than the fixed-size SCALE encoding and Keccak hash performed.
        weight.saturating_add(db_weight.reads(validators_migrated.saturating_add(queued_keys)))
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        if !should_migrate() {
            return Ok(None::<(u64, u64)>.encode());
        }

        let validators = pallet_session::NextKeys::<Runtime>::iter_keys().collect::<Vec<_>>();
        for validator in &validators {
            ensure!(
                frame_support::storage::unhashed::get::<SessionKeysOld>(
                    &pallet_session::NextKeys::<Runtime>::hashed_key_for(validator)
                )
                .is_some(),
                "NextKeys contains a value that does not decode as the old four-key layout"
            );
        }

        let queued =
            frame_support::storage::unhashed::get::<Vec<(crate::AccountId, SessionKeysOld)>>(
                &pallet_session::QueuedKeys::<Runtime>::hashed_key(),
            )
            .unwrap_or_default();

        Ok(Some((validators.len() as u64, queued.len() as u64)).encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
        let Some((registered_count, queued_count)) =
            Option::<(u64, u64)>::decode(&mut state.as_ref())
                .map_err(|_| "`pre_upgrade` provided an invalid state")?
        else {
            return Ok(());
        };

        let registered = pallet_session::NextKeys::<Runtime>::iter().collect::<Vec<_>>();
        ensure!(
            registered.len() as u64 == registered_count,
            "registered validator count changed during session-key migration"
        );
        ensure!(
            pallet_session::QueuedKeys::<Runtime>::get().len() as u64 == queued_count,
            "queued validator count changed during session-key migration"
        );

        for (validator, keys) in registered {
            for key_type in SessionKeys::key_ids() {
                ensure!(
                    pallet_session::KeyOwner::<Runtime>::get((
                        *key_type,
                        keys.get_raw(*key_type).to_vec()
                    )) == Some(validator.clone()),
                    "migrated session key has an incorrect owner"
                );
            }
        }

        Ok(())
    }
}

fn should_migrate() -> bool {
    frame_system::Pallet::<Runtime>::last_runtime_upgrade_spec_version() < MIGRATION_SPEC_VERSION
}

fn migrate_keys(validator: crate::AccountId, old: SessionKeysOld) -> SessionKeys {
    SessionKeys {
        babe: old.babe,
        grandpa: old.grandpa,
        im_online: old.im_online,
        authority_discovery: old.authority_discovery,
        beefy: placeholder_beefy_key(&validator),
    }
}

/// Deterministic placeholder BEEFY key derived from the validator id, distinct per
/// validator so `NextKeys`/`QueuedKeys` stay free of duplicate-key collisions. Never
/// asked to sign anything while BEEFY is inactive, so it need not decode to a valid
/// secp256k1 point — only be distinct and stable.
fn placeholder_beefy_key(validator: &crate::AccountId) -> BeefyId {
    let hash = Keccak256::hash(&validator.encode());

    let mut bytes = [0u8; 33];
    bytes[0] = 0x02;
    bytes[1..].copy_from_slice(hash.as_bytes());

    BeefyId::from(sp_core::ecdsa::Public::from(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp_core::Pair;
    use sp_runtime::traits::OpaqueKeys;

    fn validator(seed: u8) -> crate::AccountId {
        crate::AccountId::new([seed; 32])
    }

    fn old_keys(seed: u8) -> SessionKeysOld {
        SessionKeysOld {
            babe: sp_consensus_babe::AuthorityId::from(sp_core::sr25519::Public::from_raw(
                [seed; 32],
            )),
            grandpa: sp_consensus_grandpa::AuthorityId::from(sp_core::ed25519::Public::from_raw(
                [seed; 32],
            )),
            im_online: pallet_im_online::sr25519::AuthorityId::from(
                sp_core::sr25519::Public::from_raw([seed; 32]),
            ),
            authority_discovery: sp_authority_discovery::AuthorityId::from(
                sp_core::sr25519::Public::from_raw([seed; 32]),
            ),
        }
    }

    fn current_keys(seed: u8) -> SessionKeys {
        migrate_keys(validator(seed), old_keys(seed))
    }

    fn set_last_runtime_upgrade(spec_version: u32) {
        frame_system::LastRuntimeUpgrade::<Runtime>::put(frame_system::LastRuntimeUpgradeInfo {
            spec_version: spec_version.into(),
            spec_name: crate::VERSION.spec_name.clone(),
        });
    }

    fn seed_old_next_keys(entries: &[(crate::AccountId, SessionKeysOld)]) {
        for (validator, keys) in entries {
            frame_support::storage::unhashed::put(
                &pallet_session::NextKeys::<Runtime>::hashed_key_for(validator),
                keys,
            );
            for key_type in SessionKeysOld::key_ids() {
                pallet_session::KeyOwner::<Runtime>::insert(
                    (*key_type, keys.get_raw(*key_type).to_vec()),
                    validator,
                );
            }
        }
    }

    fn seed_old_queued_keys(entries: &[(crate::AccountId, SessionKeysOld)]) {
        frame_support::storage::unhashed::put(
            &pallet_session::QueuedKeys::<Runtime>::hashed_key(),
            &entries.to_vec(),
        );
    }

    fn assert_migrated(validator: &crate::AccountId, old: &SessionKeysOld) {
        let migrated = pallet_session::NextKeys::<Runtime>::get(validator)
            .expect("migration must preserve every existing NextKeys entry");

        assert_eq!(migrated.babe, old.babe);
        assert_eq!(migrated.grandpa, old.grandpa);
        assert_eq!(migrated.im_online, old.im_online);
        assert_eq!(migrated.authority_discovery, old.authority_discovery);
        assert_eq!(migrated.beefy, placeholder_beefy_key(validator));

        for key_type in SessionKeysOld::key_ids() {
            assert_eq!(
                pallet_session::KeyOwner::<Runtime>::get((
                    *key_type,
                    old.get_raw(*key_type).to_vec()
                )),
                Some(validator.clone())
            );
        }
        assert_eq!(
            pallet_session::KeyOwner::<Runtime>::get((
                sp_consensus_beefy::KEY_TYPE,
                migrated.get_raw(sp_consensus_beefy::KEY_TYPE).to_vec()
            )),
            Some(validator.clone())
        );
    }

    #[test]
    fn migrates_next_queued_and_key_ownership_once() {
        sp_io::TestExternalities::default().execute_with(|| {
            let entries = [(validator(1), old_keys(1)), (validator(2), old_keys(2))];
            seed_old_next_keys(&entries);
            seed_old_queued_keys(&entries);

            let db_weight = <Runtime as frame_system::Config>::DbWeight::get();
            assert_eq!(
                MigrateSessionKeys::on_runtime_upgrade(),
                db_weight.reads_writes(11, 21)
            );

            for (validator, old) in &entries {
                assert_migrated(validator, old);
            }
            let mut queued = pallet_session::QueuedKeys::<Runtime>::get();
            assert_eq!(queued.len(), entries.len());
            for (validator, keys) in &queued {
                assert_eq!(keys.beefy, placeholder_beefy_key(validator));
            }

            for (index, (validator, _)) in entries.iter().enumerate() {
                let mut keys = pallet_session::NextKeys::<Runtime>::get(validator)
                    .expect("migrated keys exist");
                pallet_session::KeyOwner::<Runtime>::remove((
                    sp_consensus_beefy::KEY_TYPE,
                    keys.get_raw(sp_consensus_beefy::KEY_TYPE).to_vec(),
                ));
                keys.beefy = BeefyId::from(
                    sp_core::ecdsa::Pair::from_seed(&[(index + 10) as u8; 32]).public(),
                );
                pallet_session::KeyOwner::<Runtime>::insert(
                    (
                        sp_consensus_beefy::KEY_TYPE,
                        keys.get_raw(sp_consensus_beefy::KEY_TYPE).to_vec(),
                    ),
                    validator,
                );
                pallet_session::NextKeys::<Runtime>::insert(validator, &keys);
                queued[index].1 = keys;
            }
            pallet_session::QueuedKeys::<Runtime>::put(queued);
            set_last_runtime_upgrade(MIGRATION_SPEC_VERSION);

            let next_before = entries
                .iter()
                .map(|(validator, _)| {
                    sp_io::storage::get(&pallet_session::NextKeys::<Runtime>::hashed_key_for(
                        validator,
                    ))
                })
                .collect::<Vec<_>>();
            let queued_before =
                sp_io::storage::get(&pallet_session::QueuedKeys::<Runtime>::hashed_key());
            let owners_before = pallet_session::KeyOwner::<Runtime>::iter().collect::<Vec<_>>();

            assert_eq!(MigrateSessionKeys::on_runtime_upgrade(), db_weight.reads(1));
            assert_eq!(
                entries
                    .iter()
                    .map(|(validator, _)| {
                        sp_io::storage::get(&pallet_session::NextKeys::<Runtime>::hashed_key_for(
                            validator,
                        ))
                    })
                    .collect::<Vec<_>>(),
                next_before
            );
            assert_eq!(
                sp_io::storage::get(&pallet_session::QueuedKeys::<Runtime>::hashed_key()),
                queued_before
            );
            assert_eq!(
                pallet_session::KeyOwner::<Runtime>::iter().collect::<Vec<_>>(),
                owners_before
            );
        });
    }

    #[test]
    fn migrates_nonempty_queue_without_next_keys() {
        sp_io::TestExternalities::default().execute_with(|| {
            let queued = [(validator(1), old_keys(1)), (validator(2), old_keys(2))];
            seed_old_queued_keys(&queued);

            let db_weight = <Runtime as frame_system::Config>::DbWeight::get();
            assert_eq!(
                MigrateSessionKeys::on_runtime_upgrade(),
                db_weight.reads_writes(5, 1)
            );
            assert_eq!(
                pallet_session::QueuedKeys::<Runtime>::get(),
                queued
                    .into_iter()
                    .map(|(validator, keys)| {
                        let migrated = migrate_keys(validator.clone(), keys);
                        (validator, migrated)
                    })
                    .collect::<Vec<_>>()
            );
        });
    }

    #[test]
    fn skips_fresh_five_key_genesis() {
        sp_io::TestExternalities::default().execute_with(|| {
            let validator = validator(1);
            let keys = current_keys(1);
            pallet_session::NextKeys::<Runtime>::insert(&validator, &keys);
            pallet_session::QueuedKeys::<Runtime>::put(vec![(validator.clone(), keys)]);
            set_last_runtime_upgrade(MIGRATION_SPEC_VERSION);

            let next_before = sp_io::storage::get(
                &pallet_session::NextKeys::<Runtime>::hashed_key_for(&validator),
            );
            let queued_before =
                sp_io::storage::get(&pallet_session::QueuedKeys::<Runtime>::hashed_key());

            assert_eq!(
                MigrateSessionKeys::on_runtime_upgrade(),
                <Runtime as frame_system::Config>::DbWeight::get().reads(1)
            );
            assert_eq!(
                sp_io::storage::get(&pallet_session::NextKeys::<Runtime>::hashed_key_for(
                    &validator
                )),
                next_before
            );
            assert_eq!(
                sp_io::storage::get(&pallet_session::QueuedKeys::<Runtime>::hashed_key()),
                queued_before
            );
        });
    }

    #[test]
    fn placeholder_beefy_keys_are_distinct_per_validator() {
        assert_ne!(
            placeholder_beefy_key(&validator(1)),
            placeholder_beefy_key(&validator(2))
        );
    }
}
