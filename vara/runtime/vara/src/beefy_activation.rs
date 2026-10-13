// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Consensus key suitability and fail-closed activation, including Root dispatch.
use crate::{AccountId, BeefyId, BlockNumber, Runtime, RuntimeOrigin, SessionKeys, System};
use frame_support::{
    crypto::ecdsa::ECDSAExt,
    traits::{EnsureOriginWithArg, Get},
    weights::Weight,
};

use sp_core::ecdsa;
use sp_std::collections::btree_set::BTreeSet;

fn operational_key(validator: &AccountId, key: &BeefyId) -> bool {
    key != &crate::migrations::placeholder_beefy_key(validator)
        && ecdsa::Public::from(key.clone()).to_eth_address().is_ok()
}

fn valid_set(keys: &[(AccountId, SessionKeys)], authorities: &[BeefyId]) -> bool {
    if keys.is_empty() || keys.len() != authorities.len() {
        return false;
    }
    let mut unique_keys = BTreeSet::new();
    let mut unique_validators = BTreeSet::new();
    keys.iter()
        .zip(authorities)
        .all(|((validator, keys), authority)| {
            &keys.beefy == authority
                && operational_key(validator, authority)
                && unique_validators.insert(validator)
                && unique_keys.insert(authority)
        })
}

fn keyset_commitment(authorities: &[BeefyId]) -> sp_core::H256 {
    binary_merkle_tree::merkle_root::<<Runtime as pallet_mmr::Config>::Hashing, _>(
        authorities.iter().map(|key| {
            ecdsa::Public::from(key.clone())
                .to_eth_address()
                .expect("readiness validates every ECDSA key before computing its commitment")
        }),
    )
}

/// Validate actual consensus committees, independently of destination and election policy.
pub(crate) fn ready(max_authorities: u32) -> bool {
    // Bound all four storage values before decoding or allocating any committee.
    if [
        crate::session_history::ActiveSessionKeys::decode_len(),
        pallet_session::QueuedKeys::<Runtime>::decode_len(),
        pallet_beefy::Authorities::<Runtime>::decode_len(),
        pallet_beefy::NextAuthorities::<Runtime>::decode_len(),
    ]
    .into_iter()
    .any(|len| !matches!(len, Some(len) if len > 0 && len <= max_authorities as usize))
    {
        return false;
    }
    let (Ok(active), Ok(queued), Ok(current), Ok(next)) = (
        crate::session_history::ActiveSessionKeys::try_get(),
        pallet_session::QueuedKeys::<Runtime>::try_get(),
        pallet_beefy::Authorities::<Runtime>::try_get(),
        pallet_beefy::NextAuthorities::<Runtime>::try_get(),
    ) else {
        return false;
    };
    let (Ok(set_id), Ok(session_index), Ok(current_leaf), Ok(next_leaf), Ok(leaves), Ok(root)) = (
        pallet_beefy::ValidatorSetId::<Runtime>::try_get(),
        pallet_session::CurrentIndex::<Runtime>::try_get(),
        pallet_beefy_mmr::BeefyAuthorities::<Runtime>::try_get(),
        pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::try_get(),
        pallet_mmr::NumberOfLeaves::<Runtime>::try_get(),
        pallet_mmr::RootHash::<Runtime>::try_get(),
    ) else {
        return false;
    };
    valid_set(&active, &current)
        && valid_set(&queued, &next)
        && leaves > 0
        && !root.is_zero()
        && pallet_beefy::SetIdSession::<Runtime>::get(set_id) == Some(session_index)
        && current_leaf.id == set_id
        && current_leaf.len as usize == current.len()
        && !current_leaf.keyset_commitment.is_zero()
        && current_leaf.keyset_commitment == keyset_commitment(&current)
        && set_id.checked_add(1) == Some(next_leaf.id)
        && next_leaf.len as usize == next.len()
        && !next_leaf.keyset_commitment.is_zero()
        && next_leaf.keyset_commitment == keyset_commitment(&next)
}

/// Initial activation and restarts enforce the same bounded consensus readiness.
pub struct ActivationOrigin;
impl EnsureOriginWithArg<RuntimeOrigin, BlockNumber> for ActivationOrigin {
    type Success = ();
    fn try_origin(origin: RuntimeOrigin, delay: &BlockNumber) -> Result<(), RuntimeOrigin> {
        if frame_system::ensure_root(origin.clone()).is_ok()
            && *delay > 0
            && System::block_number().checked_add(*delay).is_some()
            && ready(crate::MaxActiveValidators::get())
        {
            Ok(())
        } else {
            Err(origin)
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn try_successful_origin(_: &BlockNumber) -> Result<RuntimeOrigin, ()> {
        Err(())
    }
}

pub struct ActivationWeight;
impl Get<Weight> for ActivationWeight {
    fn get() -> Weight {
        // Reserve half the operational maximum, leaving room for governance wrappers.
        // This conservative allowance is not production benchmark evidence.
        <Runtime as frame_system::Config>::BlockWeights::get()
            .get(frame_support::dispatch::DispatchClass::Operational)
            .max_extrinsic
            .expect("operational calls have a configured maximum extrinsic weight")
            .saturating_div(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Beefy, GearEthBridge, Mmr, RuntimeCall, Session, Utility, genesis_config_presets};
    use frame_support::{
        assert_noop, assert_ok,
        traits::{Hooks, UnfilteredDispatchable},
    };
    use parity_scale_codec::Encode;
    use sp_core::{H160, H256, Pair, U256, ed25519, sr25519};
    use sp_runtime::{
        BuildStorage, DispatchError,
        traits::{Hash, Keccak256, OpaqueKeys},
    };

    fn ext() -> sp_io::TestExternalities {
        let mut ext: sp_io::TestExternalities = genesis_config_presets::local_testnet_genesis()
            .build_storage()
            .unwrap()
            .into();
        ext.register_extension(sp_keystore::KeystoreExt::new(
            sp_keystore::testing::MemoryKeystore::new(),
        ));
        ext.execute_with(|| {
            frame_system::BlockHash::<Runtime>::insert(0, H256::repeat_byte(9));
            System::set_block_number(10);
        });
        ext
    }

    fn native_keys(seed: &str, owner: &AccountId) -> (SessionKeys, Vec<u8>) {
        let generated =
            SessionKeys::generate(&owner.encode(), Some(format!("//{seed}").into_bytes()));
        (generated.keys, generated.proof.encode())
    }

    fn register(seed: &str) {
        let (stash, ..) = genesis_config_presets::authority_keys_from_seed(seed);
        let account = pallet_staking::Bonded::<Runtime>::get(&stash).unwrap();
        let (keys, proof) = native_keys(seed, &account);
        assert_eq!(
            pallet_session::NextKeys::<Runtime>::get(&stash),
            Some(keys.clone())
        );
        assert_ok!(Session::set_keys(
            RuntimeOrigin::signed(account),
            keys,
            proof
        ));
    }

    fn prepare() {
        register("Alice");
        register("Bob");
        pallet_session::CurrentIndex::<Runtime>::put(Session::current_index());
        Mmr::on_initialize(System::block_number());
    }

    fn install_committees(
        active: &[(AccountId, SessionKeys)],
        queued: &[(AccountId, SessionKeys)],
    ) {
        let current: frame_support::BoundedVec<_, crate::MaxAuthorities> = active
            .iter()
            .map(|(_, keys)| keys.beefy.clone())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let next: frame_support::BoundedVec<_, crate::MaxAuthorities> = queued
            .iter()
            .map(|(_, keys)| keys.beefy.clone())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        crate::session_history::ActiveSessionKeys::put(active);
        pallet_session::QueuedKeys::<Runtime>::put(queued);
        pallet_beefy::Authorities::<Runtime>::put(&current);
        pallet_beefy::NextAuthorities::<Runtime>::put(&next);
        let id = pallet_beefy::ValidatorSetId::<Runtime>::get();
        pallet_beefy_mmr::BeefyAuthorities::<Runtime>::mutate(|descriptor| {
            descriptor.id = id;
            descriptor.len = current.len() as u32;
            descriptor.keyset_commitment = keyset_commitment(&current);
        });
        pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::mutate(|descriptor| {
            descriptor.id = id + 1;
            descriptor.len = next.len() as u32;
            descriptor.keyset_commitment = keyset_commitment(&next);
        });
    }

    #[test]
    fn native_proofs_use_pop_domain_for_every_key() {
        ext().execute_with(|| {
            let account = AccountId::new([6; 32]);
            let (keys, proof) = native_keys("Native", &account);
            assert_eq!(keys.encode().len(), 161);
            assert_eq!(proof.len(), 321);
            let statement = [b"POP_".as_slice(), account.as_ref()].concat();
            let verify_sr = |offset, public| {
                sp_io::crypto::sr25519_verify(
                    &sr25519::Signature::from_raw(proof[offset..offset + 64].try_into().unwrap()),
                    &statement,
                    &public,
                )
            };
            assert!(verify_sr(0, sr25519::Public::from(keys.babe.clone())));
            assert!(sp_io::crypto::ed25519_verify(
                &ed25519::Signature::from_raw(proof[64..128].try_into().unwrap()),
                &statement,
                &ed25519::Public::from(keys.grandpa.clone()),
            ));
            assert!(verify_sr(
                128,
                sr25519::Public::from(keys.im_online.clone())
            ));
            assert!(verify_sr(
                192,
                sr25519::Public::from(keys.authority_discovery.clone())
            ));
            let signature = ecdsa::Signature::from_raw(proof[256..].try_into().unwrap());
            let public = ecdsa::Public::from(keys.beefy.clone());
            assert!(sp_io::crypto::ecdsa_verify(&signature, &statement, &public));
            assert!(!sp_io::crypto::ecdsa_verify(
                &signature,
                account.as_ref(),
                &public
            ));
            assert!(!sp_io::crypto::ecdsa_verify_prehashed(
                &signature,
                &Keccak256::hash(&statement).0,
                &public,
            ));
            assert!(keys.ownership_proof_is_valid(&account.encode(), &proof));
        });
    }

    #[test]
    fn permissionless_standbys_and_desired_count_do_not_block_source_activation() {
        ext().execute_with(|| {
            prepare();
            let seed = "Standby";
            let (stash, ..) = genesis_config_presets::authority_keys_from_seed(seed);
            let (keys, proof) = native_keys(seed, &stash);
            assert_ok!(crate::Balances::force_set_balance(
                RuntimeOrigin::root(),
                stash.clone().into(),
                1_000_000_000_000_000_000,
            ));
            assert_ok!(crate::Staking::bond(
                RuntimeOrigin::signed(stash.clone()),
                100_000_000_000_000,
                pallet_staking::RewardDestination::Staked,
            ));
            assert_ok!(Session::set_keys(
                RuntimeOrigin::signed(stash.clone()),
                keys.clone(),
                proof
            ));
            assert_eq!(pallet_session::NextKeys::<Runtime>::get(&stash), Some(keys));
            pallet_staking::ValidatorCount::<Runtime>::put(1001);
            assert!(ready(crate::MaxActiveValidators::get()));
            assert_ok!(Beefy::set_new_genesis(RuntimeOrigin::root(), 1));
        });
    }

    #[test]
    fn untouched_genesis_standbys_and_repeated_registration_do_not_block_activation() {
        let mut genesis = genesis_config_presets::local_testnet_genesis();
        let (standby, _, babe, grandpa, im_online, authority_discovery, beefy) =
            genesis_config_presets::authority_keys_from_seed("Charlie");
        genesis.session.non_authority_keys.push((
            standby.clone(),
            standby.clone(),
            SessionKeys {
                babe,
                grandpa,
                im_online,
                authority_discovery,
                beefy,
            },
        ));
        let mut ext: sp_io::TestExternalities = genesis.build_storage().unwrap().into();
        ext.register_extension(sp_keystore::KeystoreExt::new(
            sp_keystore::testing::MemoryKeystore::new(),
        ));
        ext.execute_with(|| {
            frame_system::BlockHash::<Runtime>::insert(0, H256::repeat_byte(9));
            System::set_block_number(10);
            let untouched = pallet_session::NextKeys::<Runtime>::get(&standby).unwrap();
            prepare();
            assert!(ready(crate::MaxActiveValidators::get()));
            register("Alice");
            assert!(ready(crate::MaxActiveValidators::get()));
            assert_eq!(
                pallet_session::NextKeys::<Runtime>::get(&standby),
                Some(untouched)
            );
            assert_ok!(Beefy::set_new_genesis(RuntimeOrigin::root(), 1));
        });
    }

    #[test]
    fn activation_is_fail_closed_even_for_root_bypass_and_then_supports_restart() {
        ext().execute_with(|| {
            let call =
                RuntimeCall::Beefy(pallet_beefy::Call::set_new_genesis { delay_in_blocks: 1 });
            assert!(
                call.clone()
                    .dispatch_bypass_filter(RuntimeOrigin::root())
                    .is_err()
            );
            assert_noop!(
                Beefy::set_new_genesis(RuntimeOrigin::root(), 1),
                DispatchError::BadOrigin
            );
            prepare();
            let current = pallet_beefy::Authorities::<Runtime>::get();
            pallet_beefy::Authorities::<Runtime>::kill();
            assert!(
                call.clone()
                    .dispatch_bypass_filter(RuntimeOrigin::root())
                    .is_err()
            );
            assert_noop!(
                Beefy::set_new_genesis(RuntimeOrigin::root(), 1),
                DispatchError::BadOrigin
            );
            pallet_beefy::Authorities::<Runtime>::put(current);
            assert_noop!(
                Beefy::set_new_genesis(RuntimeOrigin::root(), 0),
                DispatchError::BadOrigin
            );
            assert_noop!(
                Beefy::set_new_genesis(RuntimeOrigin::root(), u32::MAX),
                DispatchError::BadOrigin
            );
            assert_noop!(
                Beefy::set_new_genesis(RuntimeOrigin::signed(AccountId::new([5; 32])), 1),
                DispatchError::BadOrigin,
            );
            assert_ok!(call.clone().dispatch_bypass_filter(RuntimeOrigin::root()));
            assert_eq!(pallet_beefy::GenesisBlock::<Runtime>::get(), Some(11));
            assert_ok!(Utility::batch_all(
                RuntimeOrigin::root(),
                vec![RuntimeCall::Beefy(pallet_beefy::Call::set_new_genesis {
                    delay_in_blocks: 2
                }),]
            ));
            assert_eq!(pallet_beefy::GenesisBlock::<Runtime>::get(), Some(12));
            System::set_block_number(12);
            assert_ok!(Beefy::set_new_genesis(RuntimeOrigin::root(), 3));
            assert_eq!(pallet_beefy::GenesisBlock::<Runtime>::get(), Some(15));
            pallet_mmr::RootHash::<Runtime>::kill();
            assert!(call.dispatch_bypass_filter(RuntimeOrigin::root()).is_err());
            assert_noop!(
                Beefy::set_new_genesis(RuntimeOrigin::root(), 1),
                DispatchError::BadOrigin
            );
        });
    }

    #[test]
    fn source_activation_requires_no_destination_and_ignores_bridge_policy() {
        ext().execute_with(|| {
            prepare();
            assert!(pallet_gear_eth_bridge::DestinationBinding::<Runtime>::get().is_none());
            assert!(GearEthBridge::bridge_domain().is_zero());
            assert!(ready(crate::MaxActiveValidators::get()));
            assert_ok!(Beefy::set_new_genesis(RuntimeOrigin::root(), 1));
            // Even malformed destination state is outside source consensus readiness.
            pallet_gear_eth_bridge::DestinationBinding::<Runtime>::put((
                H256::zero(),
                H256::zero(),
                H160::zero(),
            ));
            pallet_gear_eth_bridge::BridgeDomain::<Runtime>::put(H256::repeat_byte(1));
            pallet_staking::ValidatorCount::<Runtime>::put(257);
            assert!(ready(crate::MaxActiveValidators::get()));
            assert_ok!(Beefy::set_new_genesis(RuntimeOrigin::root(), 2));
        });
    }

    #[test]
    fn native_registration_rejects_wrong_owner_corrupt_proofs_and_changed_keys_without_mutation() {
        ext().execute_with(|| {
            let (stash, ..) = genesis_config_presets::authority_keys_from_seed("Alice");
            let account = pallet_staking::Bonded::<Runtime>::get(&stash).unwrap();
            let (keys, proof) = native_keys("AliceRotation", &account);
            let (other, _) = native_keys("OtherRotation", &account);
            let (bob, ..) = genesis_config_presets::authority_keys_from_seed("Bob");
            let bob = pallet_staking::Bonded::<Runtime>::get(&bob).unwrap();
            assert_noop!(
                Session::set_keys(RuntimeOrigin::signed(bob), keys.clone(), proof.clone()),
                pallet_session::Error::<Runtime>::InvalidProof
            );
            for invalid in [
                Vec::new(),
                proof[..proof.len() - 1].to_vec(),
                [proof.as_slice(), &[0]].concat(),
            ] {
                assert_noop!(
                    Session::set_keys(
                        RuntimeOrigin::signed(account.clone()),
                        keys.clone(),
                        invalid
                    ),
                    pallet_session::Error::<Runtime>::InvalidProof
                );
            }
            for offset in [0, 64, 128, 192, 256] {
                let mut invalid = proof.clone();
                invalid[offset] ^= 1;
                assert_noop!(
                    Session::set_keys(
                        RuntimeOrigin::signed(account.clone()),
                        keys.clone(),
                        invalid
                    ),
                    pallet_session::Error::<Runtime>::InvalidProof
                );
            }
            for index in 0..5 {
                let mut changed = keys.clone();
                match index {
                    0 => changed.babe = other.babe.clone(),
                    1 => changed.grandpa = other.grandpa.clone(),
                    2 => changed.im_online = other.im_online.clone(),
                    3 => changed.authority_discovery = other.authority_discovery.clone(),
                    _ => changed.beefy = other.beefy.clone(),
                }
                assert_noop!(
                    Session::set_keys(
                        RuntimeOrigin::signed(account.clone()),
                        changed,
                        proof.clone()
                    ),
                    pallet_session::Error::<Runtime>::InvalidProof
                );
            }
            let mut invalid_scalar = proof.clone();
            invalid_scalar[256..288].fill(0);
            assert_noop!(
                Session::set_keys(
                    RuntimeOrigin::signed(account.clone()),
                    keys.clone(),
                    invalid_scalar
                ),
                pallet_session::Error::<Runtime>::InvalidProof
            );
            // Produce the malleable counterpart of the native low-S ECDSA signature.
            let order = U256::from_big_endian(&[
                0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
                0xff, 0xfe, 0xba, 0xae, 0xdc, 0xe6, 0xaf, 0x48, 0xa0, 0x3b, 0xbf, 0xd2, 0x5e, 0x8c,
                0xd0, 0x36, 0x41, 0x41,
            ]);
            let mut high_s = proof.clone();
            let s = U256::from_big_endian(&high_s[288..320]);
            (order - s).to_big_endian(&mut high_s[288..320]);
            high_s[320] ^= 1;
            assert_noop!(
                Session::set_keys(RuntimeOrigin::signed(account.clone()), keys.clone(), high_s),
                pallet_session::Error::<Runtime>::InvalidProof
            );
            assert_ok!(Session::set_keys(
                RuntimeOrigin::signed(account),
                keys.clone(),
                proof
            ));
            assert_eq!(pallet_session::NextKeys::<Runtime>::get(&stash), Some(keys));
        });
    }

    #[test]
    fn native_registration_uses_effective_controller_but_stores_under_stash() {
        ext().execute_with(|| {
            let (stash, ..) = genesis_config_presets::authority_keys_from_seed("Alice");
            let controller = AccountId::new([42; 32]);
            assert_ne!(controller, stash);
            pallet_staking::Bonded::<Runtime>::insert(&stash, &controller);
            pallet_staking::Ledger::<Runtime>::insert(
                &controller,
                pallet_staking::StakingLedger::<Runtime>::new(stash.clone(), 1),
            );
            frame_system::Account::<Runtime>::mutate(&controller, |info| {
                info.providers = 1;
                info.consumers = 1;
            });
            let (mut keys, stash_proof) = native_keys("ControllerRotation", &stash);
            assert_noop!(
                Session::set_keys(
                    RuntimeOrigin::signed(controller.clone()),
                    keys.clone(),
                    stash_proof
                ),
                pallet_session::Error::<Runtime>::InvalidProof
            );
            let proof = keys
                .create_ownership_proof(&controller.encode())
                .unwrap()
                .encode();
            assert_ok!(Session::set_keys(
                RuntimeOrigin::signed(controller.clone()),
                keys.clone(),
                proof
            ));
            assert_eq!(
                pallet_session::NextKeys::<Runtime>::get(&stash),
                Some(keys.clone())
            );
            assert!(!pallet_session::NextKeys::<Runtime>::contains_key(
                &controller
            ));
            for kind in SessionKeys::key_ids() {
                assert_eq!(
                    Session::key_owner(*kind, keys.get_raw(*kind)),
                    Some(stash.clone())
                );
            }
        });
    }

    #[test]
    fn native_proofs_are_composable_and_not_genesis_or_bundle_bound() {
        ext().execute_with(|| {
            let (stash, ..) = genesis_config_presets::authority_keys_from_seed("Alice");
            let account = pallet_staking::Bonded::<Runtime>::get(&stash).unwrap();
            let (mut keys, mut proof) = native_keys("CompositionA", &account);
            let (other, other_proof) = native_keys("CompositionB", &account);
            keys.grandpa = other.grandpa;
            keys.authority_discovery = other.authority_discovery;
            proof[64..128].copy_from_slice(&other_proof[64..128]);
            proof[192..256].copy_from_slice(&other_proof[192..256]);
            assert!(keys.ownership_proof_is_valid(&account.encode(), &proof));
            frame_system::BlockHash::<Runtime>::insert(0, H256::repeat_byte(8));
            assert_ok!(Session::set_keys(
                RuntimeOrigin::signed(account.clone()),
                keys.clone(),
                proof.clone()
            ));
            frame_system::BlockHash::<Runtime>::insert(0, H256::zero());
            assert_ok!(Session::set_keys(
                RuntimeOrigin::signed(account),
                keys.clone(),
                proof
            ));
            assert_eq!(pallet_session::NextKeys::<Runtime>::get(&stash), Some(keys));
        });
    }

    #[test]
    fn source_authority_bound_accepts_257_and_1000_but_rejects_1001() {
        ext().execute_with(|| {
            prepare();
            assert_eq!(crate::MaxActiveValidators::get(), 1000);
            let template = crate::session_history::ActiveSessionKeys::get()[0]
                .1
                .clone();
            // Synthetic consensus state uses real distinct curve keys and exact commitments;
            // native five-key ownership admission is exercised by the registration tests above.
            let keys: Vec<_> = (0u32..1001)
                .map(|index| {
                    let seed = sp_core::hashing::blake2_256(&index.to_le_bytes());
                    let mut keys = template.clone();
                    keys.beefy = ecdsa::Pair::from_seed(&seed).public().into();
                    (AccountId::new(seed), keys)
                })
                .collect();
            for count in [257, 1000] {
                install_committees(&keys[..count], &keys[..count]);
                assert!(ready(crate::MaxActiveValidators::get()));
                assert!(!ready(256));
                assert_ok!(Beefy::set_new_genesis(RuntimeOrigin::root(), 1));
            }
            for (active, queued) in [(1001, 1000), (1000, 1001)] {
                install_committees(&keys[..active], &keys[..queued]);
                assert!(!ready(crate::MaxActiveValidators::get()));
                assert_noop!(
                    Beefy::set_new_genesis(RuntimeOrigin::root(), 1),
                    DispatchError::BadOrigin
                );
            }
        });
    }

    #[test]
    fn activation_rejects_invalid_placeholder_duplicate_and_unordered_sets() {
        ext().execute_with(|| {
            prepare();
            let active = crate::session_history::ActiveSessionKeys::get();
            let queued = pallet_session::QueuedKeys::<Runtime>::get();
            let current = pallet_beefy::Authorities::<Runtime>::get();
            // Select a placeholder which really is a curve point: ECC validity alone is insufficient.
            let owner = (0u32..100)
                .map(|index| AccountId::new(sp_core::hashing::blake2_256(&index.to_le_bytes())))
                .find(|owner| {
                    ecdsa::Public::from(crate::migrations::placeholder_beefy_key(owner))
                        .to_eth_address()
                        .is_ok()
                })
                .unwrap();
            let mut bad = active.clone();
            bad[0].0 = owner;
            bad[0].1.beefy = crate::migrations::placeholder_beefy_key(&bad[0].0);
            assert!(!operational_key(&bad[0].0, &bad[0].1.beefy));
            install_committees(&bad, &queued);
            assert!(!ready(crate::MaxActiveValidators::get()));
            install_committees(&active, &bad);
            assert!(!ready(crate::MaxActiveValidators::get()));

            bad = active.clone();
            bad[0].1.beefy = ecdsa::Public::from_raw([0; 33]).into();
            crate::session_history::ActiveSessionKeys::put(&bad);
            let authorities: frame_support::BoundedVec<_, crate::MaxAuthorities> = bad
                .iter()
                .map(|(_, keys)| keys.beefy.clone())
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
            pallet_beefy::Authorities::<Runtime>::put(authorities);
            assert!(!ready(crate::MaxActiveValidators::get()));
            install_committees(&active, &queued);
            pallet_session::QueuedKeys::<Runtime>::put(&bad);
            let authorities: frame_support::BoundedVec<_, crate::MaxAuthorities> = bad
                .iter()
                .map(|(_, keys)| keys.beefy.clone())
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
            pallet_beefy::NextAuthorities::<Runtime>::put(authorities);
            assert!(!ready(crate::MaxActiveValidators::get()));

            for duplicate_identity in [false, true] {
                bad = active.clone();
                if duplicate_identity {
                    bad[1].0 = bad[0].0.clone();
                } else {
                    bad[1].1.beefy = bad[0].1.beefy.clone();
                }
                install_committees(&bad, &queued);
                assert!(!ready(crate::MaxActiveValidators::get()));
                install_committees(&active, &bad);
                assert!(!ready(crate::MaxActiveValidators::get()));
            }
            install_committees(&active, &queued);
            let mut reversed = current.clone().into_inner();
            reversed.reverse();
            pallet_beefy::Authorities::<Runtime>::put(
                frame_support::BoundedVec::<_, crate::MaxAuthorities>::try_from(reversed).unwrap(),
            );
            assert!(!ready(crate::MaxActiveValidators::get()));
            install_committees(&active, &queued);
            let mut reversed = queued.clone();
            reversed.reverse();
            pallet_session::QueuedKeys::<Runtime>::put(reversed);
            assert!(!ready(crate::MaxActiveValidators::get()));
            install_committees(&active, &queued);
            let unknown = AccountId::new([7; 32]);
            let mut stale = active[0].1.clone();
            stale.beefy = crate::migrations::placeholder_beefy_key(&unknown);
            pallet_session::NextKeys::<Runtime>::insert(&unknown, stale);
            // Dormant registrations, even unusable ones, are not either actual committee.
            assert!(ready(crate::MaxActiveValidators::get()));
            let (stash, ..) = genesis_config_presets::authority_keys_from_seed("Bob");
            let account = pallet_staking::Bonded::<Runtime>::get(&stash).unwrap();
            assert_ok!(Session::purge_keys(RuntimeOrigin::signed(account)));
            assert!(ready(crate::MaxActiveValidators::get()));
        });
    }

    #[test]
    fn activation_rejects_missing_malformed_empty_and_oversized_storage() {
        ext().execute_with(|| {
            prepare();
            assert!(ready(crate::MaxActiveValidators::get()));
            let vector_keys = [
                crate::session_history::ActiveSessionKeys::hashed_key().to_vec(),
                pallet_session::QueuedKeys::<Runtime>::hashed_key().to_vec(),
                pallet_beefy::Authorities::<Runtime>::hashed_key().to_vec(),
                pallet_beefy::NextAuthorities::<Runtime>::hashed_key().to_vec(),
            ];
            let scalar_keys = [
                pallet_beefy::ValidatorSetId::<Runtime>::hashed_key().to_vec(),
                pallet_session::CurrentIndex::<Runtime>::hashed_key().to_vec(),
                pallet_beefy::SetIdSession::<Runtime>::hashed_key_for(0).to_vec(),
                pallet_beefy_mmr::BeefyAuthorities::<Runtime>::hashed_key().to_vec(),
                pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::hashed_key().to_vec(),
                pallet_mmr::NumberOfLeaves::<Runtime>::hashed_key().to_vec(),
                pallet_mmr::RootHash::<Runtime>::hashed_key().to_vec(),
            ];
            for key in vector_keys.iter().chain(&scalar_keys) {
                let bytes = sp_io::storage::get(key).unwrap();
                sp_io::storage::clear(key);
                assert!(!ready(crate::MaxActiveValidators::get()));
                sp_io::storage::set(key, &[0xff]);
                assert!(!ready(crate::MaxActiveValidators::get()));
                sp_io::storage::set(key, &bytes);
            }
            for key in &vector_keys {
                let bytes = sp_io::storage::get(key).unwrap();
                for invalid in [
                    vec![0],
                    vec![4],
                    parity_scale_codec::Compact(1001u32).encode(),
                ] {
                    // One member without its bytes proves a valid decode_len is not sufficient;
                    // the oversized prefix is rejected before any vector can be loaded.
                    sp_io::storage::set(key, &invalid);
                    assert!(!ready(crate::MaxActiveValidators::get()));
                }
                sp_io::storage::set(key, &bytes);
            }
            assert!(!ready(0));
            assert!(ready(crate::MaxActiveValidators::get()));
        });
    }

    #[test]
    fn activation_rejects_descriptor_session_and_mmr_inconsistency() {
        ext().execute_with(|| {
            prepare();
            let current = pallet_beefy_mmr::BeefyAuthorities::<Runtime>::get();
            let next = pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::get();
            for (is_next, descriptor) in [(false, current.clone()), (true, next.clone())] {
                for mismatch in 0..4 {
                    let mut bad = descriptor.clone();
                    match mismatch {
                        0 => bad.id += 1,
                        1 => bad.len += 1,
                        2 => bad.keyset_commitment = H256::zero(),
                        _ => bad.keyset_commitment = H256::repeat_byte(1),
                    }
                    if is_next {
                        pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::put(bad);
                    } else {
                        pallet_beefy_mmr::BeefyAuthorities::<Runtime>::put(bad);
                    }
                    assert!(!ready(crate::MaxActiveValidators::get()));
                }
                if is_next {
                    pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::put(descriptor);
                } else {
                    pallet_beefy_mmr::BeefyAuthorities::<Runtime>::put(descriptor);
                }
            }
            pallet_beefy::SetIdSession::<Runtime>::insert(0, 1);
            assert!(!ready(crate::MaxActiveValidators::get()));
            pallet_beefy::SetIdSession::<Runtime>::insert(0, 0);
            pallet_session::CurrentIndex::<Runtime>::put(1);
            assert!(!ready(crate::MaxActiveValidators::get()));
            pallet_session::CurrentIndex::<Runtime>::put(0);
            let leaves = pallet_mmr::NumberOfLeaves::<Runtime>::get();
            let root = pallet_mmr::RootHash::<Runtime>::get();
            pallet_mmr::NumberOfLeaves::<Runtime>::put(0);
            assert!(!ready(crate::MaxActiveValidators::get()));
            pallet_mmr::NumberOfLeaves::<Runtime>::put(leaves);
            pallet_mmr::RootHash::<Runtime>::put(H256::zero());
            assert!(!ready(crate::MaxActiveValidators::get()));
            pallet_mmr::RootHash::<Runtime>::put(root);
            assert!(ready(crate::MaxActiveValidators::get()));
            pallet_beefy::ValidatorSetId::<Runtime>::put(u64::MAX);
            pallet_beefy::SetIdSession::<Runtime>::insert(u64::MAX, 0);
            let mut last = current;
            last.id = u64::MAX;
            pallet_beefy_mmr::BeefyAuthorities::<Runtime>::put(last);
            let mut overflow = next;
            overflow.id = 0;
            pallet_beefy_mmr::BeefyNextAuthorities::<Runtime>::put(overflow);
            assert!(!ready(crate::MaxActiveValidators::get()));
        });
    }
}
