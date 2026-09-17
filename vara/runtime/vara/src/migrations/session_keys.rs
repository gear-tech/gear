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
        let validators_migrated = pallet_session::NextKeys::<Runtime>::iter_keys().count() as u64;

        pallet_session::Pallet::<Runtime>::upgrade_keys::<SessionKeysOld, _>(|validator, old| {
            SessionKeys {
                babe: old.babe,
                grandpa: old.grandpa,
                im_online: old.im_online,
                authority_discovery: old.authority_discovery,
                beefy: placeholder_beefy_key(&validator),
            }
        });

        db_weight.reads_writes(validators_migrated, validators_migrated)
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

    #[test]
    fn migrates_next_and_queued_keys() {
        sp_io::TestExternalities::default().execute_with(|| {
            let alice = validator(1);
            let bob = validator(2);
            let alice_old = old_keys(1);
            let bob_old = old_keys(2);

            // Write pre-migration `NextKeys` entries using the old 4-key encoding, at
            // the exact storage keys `pallet_session::NextKeys` will later read as the
            // current 5-key `SessionKeys`.
            frame_support::storage::unhashed::put(
                &pallet_session::NextKeys::<Runtime>::hashed_key_for(&alice),
                &alice_old,
            );
            frame_support::storage::unhashed::put(
                &pallet_session::NextKeys::<Runtime>::hashed_key_for(&bob),
                &bob_old,
            );

            // Same for `QueuedKeys`, a single `Vec<(ValidatorId, Keys)>` value.
            let queued_old = vec![
                (alice.clone(), alice_old.clone()),
                (bob.clone(), bob_old.clone()),
            ];
            frame_support::storage::unhashed::put(
                &pallet_session::QueuedKeys::<Runtime>::hashed_key().to_vec(),
                &queued_old,
            );

            let weight = MigrateSessionKeys::on_runtime_upgrade();
            assert_eq!(
                weight,
                <Runtime as frame_system::Config>::DbWeight::get().reads_writes(2, 2)
            );

            for (id, old) in [(alice, alice_old), (bob, bob_old)] {
                let migrated = pallet_session::NextKeys::<Runtime>::get(&id)
                    .expect("migration must preserve every existing NextKeys entry");

                assert_eq!(migrated.babe, old.babe);
                assert_eq!(migrated.grandpa, old.grandpa);
                assert_eq!(migrated.im_online, old.im_online);
                assert_eq!(migrated.authority_discovery, old.authority_discovery);
                assert_eq!(migrated.beefy, placeholder_beefy_key(&id));
            }

            let queued = pallet_session::QueuedKeys::<Runtime>::get();
            assert_eq!(queued.len(), 2);
            for (id, keys) in &queued {
                assert_eq!(keys.beefy, placeholder_beefy_key(id));
            }
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
