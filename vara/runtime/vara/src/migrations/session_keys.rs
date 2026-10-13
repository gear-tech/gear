// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Converts the supported four-key predecessor into the five-key runtime without
//! activating BEEFY. Placeholders retain the legacy derivation and must be replaced
//! before governance activation. Release try-runtime must reject an unsupported
//! predecessor: a production validation panic halts execution, not a safe upgrade.
//!
//! Rebuilds the current and queued historical commitments with five keys. Their
//! original roots remain available for already-issued ownership proofs, but only
//! while the corresponding historical sessions remain unpruned. Older commitments
//! are unchanged. Later roots commit the activated queue, never mutable registrations.
//! Release try-runtime checks both root formats and all three key phases.

use crate::{
    AuthorityDiscovery, Babe, BeefyId, Grandpa, ImOnline, Runtime, SessionKeys,
    pallet_session_historical,
    session_history::{ActiveSessionKeys, HistoricalRoot, LegacySessionRoots},
};
use frame_support::{
    ensure, storage::StoragePrefixedMap, traits::OnRuntimeUpgrade, weights::Weight,
};
use parity_scale_codec::{Decode, DecodeAll};
use sp_runtime::{
    impl_opaque_keys,
    traits::{Hash, Keccak256, OpaqueKeys},
};
use sp_std::prelude::*;

#[cfg(any(test, feature = "try-runtime"))]
use {crate::Historical, parity_scale_codec::Encode};

#[cfg(feature = "try-runtime")]
use {sp_runtime::TryRuntimeError, sp_std::collections::btree_map::BTreeMap};

const PREDECESSOR_SPEC_VERSION: u32 = 11_000;
const MIGRATION_SPEC_VERSION: u32 = 2_01_00;
type HistoricalUpdate = (u32, HistoricalRoot, HistoricalRoot);

impl_opaque_keys! {
    /// Mirrors the exact supported four-key predecessor.
    pub struct SessionKeysOld {
        pub babe: Babe,
        pub grandpa: Grandpa,
        pub im_online: ImOnline,
        pub authority_discovery: AuthorityDiscovery,
    }
}

pub(crate) fn placeholder_beefy_key(validator: &crate::AccountId) -> BeefyId {
    let hash = Keccak256::hash(validator.as_ref());
    let mut bytes = [0u8; 33];
    bytes[0] = 0x02;
    bytes[1..].copy_from_slice(hash.as_bytes());
    BeefyId::from(sp_core::ecdsa::Public::from(bytes))
}

pub struct MigrateSessionKeys;

impl OnRuntimeUpgrade for MigrateSessionKeys {
    fn on_runtime_upgrade() -> Weight {
        let db_weight = <Runtime as frame_system::Config>::DbWeight::get();
        let mut reads = 0;
        let migrate = should_migrate(&mut reads)
            .expect("unsupported session-key predecessor; reject the release in try-runtime");
        if !migrate {
            return db_weight.reads(reads);
        }

        // The SDK skips invalid map values and ignores queued translation failures.
        // Validate all input before its first write; a failure must halt, never skip.
        let (registered, _) = validate_legacy_storage(&mut reads)
            .expect("invalid session-key state; reject the release in try-runtime");
        let active = active_keys(&mut reads)
            .expect("invalid active session keys; reject the release in try-runtime");
        let historical = historical_updates(&mut reads, &active)
            .expect("invalid historical session state; reject the release in try-runtime");
        pallet_session::Pallet::<Runtime>::upgrade_keys::<SessionKeysOld, _>(migrate_keys);
        ActiveSessionKeys::put(active);
        for (index, legacy, updated) in historical.into_iter().flatten() {
            LegacySessionRoots::insert(index, legacy);
            pallet_session_historical::HistoricalSessions::<Runtime>::insert(index, updated);
        }

        // Live upgrades do not run genesis builders. Materialize inactive SDK
        // bookkeeping without activating BEEFY or replacing existing entries.
        if !pallet_beefy::Authorities::<Runtime>::exists() {
            pallet_beefy::Authorities::<Runtime>::set(Default::default());
        }
        if !pallet_beefy::NextAuthorities::<Runtime>::exists() {
            pallet_beefy::NextAuthorities::<Runtime>::set(Default::default());
        }
        let set_id = pallet_beefy::ValidatorSetId::<Runtime>::get();
        if !pallet_beefy::SetIdSession::<Runtime>::contains_key(set_id) {
            pallet_beefy::SetIdSession::<Runtime>::insert(
                set_id,
                pallet_session::Pallet::<Runtime>::current_index(),
            );
        }

        // Keep trie headroom, but never hide permissionless registry growth from upgrade checks.
        let writes = registered.saturating_mul(10).saturating_add(10);
        let reads = reads
            .saturating_add(registered.saturating_mul(2))
            .saturating_add(8);
        <Runtime as frame_system::Config>::BlockWeights::get()
            .max_block
            .saturating_div(2)
            .saturating_add(db_weight.reads_writes(reads, writes))
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        let mut reads = 0;
        let migrate = should_migrate(&mut reads)?;
        let beefy_genesis =
            sp_io::storage::get(&pallet_beefy::GenesisBlock::<Runtime>::hashed_key())
                .map(|bytes| bytes.to_vec());
        validate_beefy_genesis(beefy_genesis.as_deref(), migrate)?;
        let active = if migrate {
            active_keys(&mut reads)?
        } else {
            ActiveSessionKeys::get()
        };
        let historical = if migrate {
            historical_updates(&mut reads, &active)?
        } else {
            [None; 2]
        };

        let mut owners = BTreeMap::new();
        inspect_map(
            &pallet_session::KeyOwner::<Runtime>::final_prefix(),
            &mut reads,
            |key, value, _| {
                validate_owner_entry(key, value)?;
                owners.insert(key.to_vec(), value.to_vec());
                Ok(())
            },
        )?;

        let mut registered = Vec::new();
        inspect_map(
            &pallet_session::NextKeys::<Runtime>::final_prefix(),
            &mut reads,
            |key, value, reads| {
                let validator = decode_validator_key(key)?;
                let expected = if migrate {
                    let old = decode_old_keys(value)?;
                    validate_registered_keys(&validator, &old, reads)?;
                    validate_placeholder_owner(&validator, reads)?;
                    let keys = migrate_keys(validator.clone(), old);
                    owners
                        .entry(pallet_session::KeyOwner::<Runtime>::hashed_key_for((
                            sp_consensus_beefy::KEY_TYPE,
                            keys.get_raw(sp_consensus_beefy::KEY_TYPE),
                        )))
                        .or_insert_with(|| validator.encode());
                    keys.encode()
                } else {
                    let keys = SessionKeys::decode_all(&mut &value[..])
                        .map_err(|_| "NextKeys is not the exact five-key layout")?;
                    validate_registered_keys(&validator, &keys, reads)?;
                    value.to_vec()
                };
                registered.push((key.to_vec(), expected));
                Ok(())
            },
        )?;

        let queued = sp_io::storage::get(&pallet_session::QueuedKeys::<Runtime>::hashed_key())
            .map(|value| -> Result<Vec<u8>, &'static str> {
                if migrate {
                    let old = decode_old_queue(&value)?;
                    Ok(old
                        .into_iter()
                        .map(|(validator, keys)| (validator.clone(), migrate_keys(validator, keys)))
                        .collect::<Vec<_>>()
                        .encode())
                } else {
                    Vec::<(crate::AccountId, SessionKeys)>::decode_all(&mut &value[..])
                        .map_err(|_| "QueuedKeys is not the exact five-key layout")?;
                    Ok(value.to_vec())
                }
            })
            .transpose()?;

        Ok(SessionKeysSnapshot {
            registered,
            queued,
            owners,
            beefy_genesis,
            historical,
            active,
        }
        .encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
        let expected = SessionKeysSnapshot::decode_all(&mut &state[..])
            .map_err(|_| "pre_upgrade provided an invalid or trailing snapshot")?;
        ensure!(
            ActiveSessionKeys::get() == expected.active,
            "active session keys changed"
        );
        let mut reads = 0;
        let mut registered = Vec::new();
        inspect_map(
            &pallet_session::NextKeys::<Runtime>::final_prefix(),
            &mut reads,
            |key, value, _| {
                decode_validator_key(key)?;
                SessionKeys::decode_all(&mut &value[..])
                    .map_err(|_| "NextKeys is not the exact five-key layout")?;
                registered.push((key.to_vec(), value.to_vec()));
                Ok(())
            },
        )?;
        ensure!(
            registered == expected.registered,
            "registered validators or original keys changed"
        );

        let queued = sp_io::storage::get(&pallet_session::QueuedKeys::<Runtime>::hashed_key())
            .map(|bytes| bytes.to_vec());
        if let Some(bytes) = &queued {
            Vec::<(crate::AccountId, SessionKeys)>::decode_all(&mut &bytes[..])
                .map_err(|_| "QueuedKeys is not the exact five-key layout")?;
        }
        ensure!(
            queued == expected.queued,
            "ordered queued validators or original keys changed"
        );

        let mut owners = BTreeMap::new();
        inspect_map(
            &pallet_session::KeyOwner::<Runtime>::final_prefix(),
            &mut reads,
            |key, value, _| {
                validate_owner_entry(key, value)?;
                owners.insert(key.to_vec(), value.to_vec());
                Ok(())
            },
        )?;
        ensure!(
            owners == expected.owners,
            "original ownership mappings changed"
        );
        let beefy_genesis =
            sp_io::storage::get(&pallet_beefy::GenesisBlock::<Runtime>::hashed_key())
                .map(|bytes| bytes.to_vec());
        ensure!(
            beefy_genesis == expected.beefy_genesis,
            "BEEFY activation changed during preparation"
        );
        for (index, legacy, updated) in expected.historical.into_iter().flatten() {
            ensure!(
                LegacySessionRoots::get(index) == Some(legacy),
                "legacy historical commitment changed"
            );
            ensure!(
                Historical::historical_root(index) == Some(updated),
                "five-key historical commitment changed"
            );
        }
        Ok(())
    }
}

#[cfg(feature = "try-runtime")]
#[derive(Encode, Decode)]
struct SessionKeysSnapshot {
    registered: Vec<(Vec<u8>, Vec<u8>)>,
    queued: Option<Vec<u8>>,
    owners: BTreeMap<Vec<u8>, Vec<u8>>,
    beefy_genesis: Option<Vec<u8>>,
    historical: [Option<HistoricalUpdate>; 2],
    active: Vec<(crate::AccountId, SessionKeys)>,
}

fn historical_updates(
    reads: &mut u64,
    active: &[(crate::AccountId, SessionKeys)],
) -> Result<[Option<HistoricalUpdate>; 2], &'static str> {
    *reads = reads.saturating_add(1);
    let current = pallet_session::Pallet::<Runtime>::current_index();
    let queued = current.checked_add(1).ok_or("session index overflow")?;
    let mut updates = [None; 2];
    for (update, index) in updates.iter_mut().zip([current, queued]) {
        *reads = reads.saturating_add(1);
        let Some(bytes) = sp_io::storage::get(&pallet_session_historical::HistoricalSessions::<
            Runtime,
        >::hashed_key_for(index)) else {
            ensure!(active.is_empty(), "missing boundary historical commitment");
            continue;
        };
        let legacy = HistoricalRoot::decode_all(&mut &bytes[..])
            .map_err(|_| "invalid historical session root")?;
        *reads = reads.saturating_add(2);
        let queued_keys;
        let (keys, era) = if index == current {
            (active, crate::Staking::active_era().map(|era| era.index))
        } else {
            queued_keys = sp_io::storage::get(&pallet_session::QueuedKeys::<Runtime>::hashed_key())
                .map(|bytes| decode_old_queue(&bytes))
                .transpose()?
                .into_iter()
                .flatten()
                .map(|(owner, keys)| (owner.clone(), migrate_keys(owner, keys)))
                .collect::<Vec<_>>();
            (&queued_keys[..], crate::Staking::current_era())
        };
        ensure!(
            keys.is_empty() || era.is_some(),
            "missing staking era for active authorities"
        );
        *reads = reads.saturating_add(keys.len() as u64);
        if index == current && !keys.is_empty() {
            // Authenticate positional keys: GRANDPA can lag even after a delayed change applies.
            ensure!(
                crate::session_history::root_with_key_ids(keys, era, SessionKeysOld::key_ids())
                    == legacy,
                "active authority provenance is not authenticated by the current four-key historical commitment"
            );
            *reads = reads.saturating_add(keys.len() as u64);
        }
        *update = Some((index, legacy, crate::session_history::root(keys, era)));
    }
    Ok(updates)
}

fn active_keys(reads: &mut u64) -> Result<Vec<(crate::AccountId, SessionKeys)>, &'static str> {
    *reads = reads.saturating_add(5);
    fn list<T: Decode>(pallet: &[u8], item: &[u8]) -> Result<Vec<T>, &'static str> {
        sp_io::storage::get(&frame_support::storage::storage_prefix(pallet, item))
            .map(|bytes| {
                let count = parity_scale_codec::Compact::<u32>::decode(&mut &bytes[..])
                    .map_err(|_| "invalid active authority length")?
                    .0;
                ensure!(
                    count <= crate::MaxActiveValidators::get(),
                    "active committee exceeds supported capacity"
                );
                Vec::<T>::decode_all(&mut &bytes[..])
                    .map_err(|_| "invalid active authority storage")
            })
            .transpose()
            .map(Option::unwrap_or_default)
    }
    let validators = list::<crate::AccountId>(b"Session", b"Validators")?;
    let babe = list::<(sp_consensus_babe::AuthorityId, u64)>(b"Babe", b"Authorities")?;
    let grandpa = list::<(sp_consensus_grandpa::AuthorityId, u64)>(b"Grandpa", b"Authorities")?;
    let online = list::<pallet_im_online::sr25519::AuthorityId>(b"ImOnline", b"Keys")?;
    let discovery = list::<sp_authority_discovery::AuthorityId>(b"AuthorityDiscovery", b"Keys")?;
    ensure!(
        [babe.len(), grandpa.len(), online.len(), discovery.len()]
            .iter()
            .all(|count| *count == validators.len()),
        "active authority lengths do not match session validators"
    );
    Ok(validators
        .into_iter()
        .enumerate()
        .map(|(i, owner)| {
            let keys = SessionKeys {
                babe: babe[i].0.clone(),
                grandpa: grandpa[i].0.clone(),
                im_online: online[i].clone(),
                authority_discovery: discovery[i].clone(),
                beefy: placeholder_beefy_key(&owner),
            };
            (owner, keys)
        })
        .collect())
}

fn should_migrate(reads: &mut u64) -> Result<bool, &'static str> {
    *reads = reads.saturating_add(1);
    let Some(bytes) =
        sp_io::storage::get(&frame_system::LastRuntimeUpgrade::<Runtime>::hashed_key())
    else {
        // Real genesis installs upgrade metadata. No record is only an empty state,
        // never permission to interpret existing session records as four-key data.
        for prefix in [
            pallet_session::NextKeys::<Runtime>::final_prefix(),
            pallet_session::KeyOwner::<Runtime>::final_prefix(),
        ] {
            *reads = reads.saturating_add(2);
            ensure!(
                sp_io::storage::get(&prefix).is_none()
                    && sp_io::storage::next_key(&prefix)
                        .is_none_or(|key| !key.starts_with(&prefix)),
                "session records exist without runtime-upgrade metadata"
            );
        }
        *reads = reads.saturating_add(1);
        ensure!(
            sp_io::storage::get(&pallet_session::QueuedKeys::<Runtime>::hashed_key()).is_none(),
            "queued keys exist without runtime-upgrade metadata"
        );
        return Ok(false);
    };
    let last = frame_system::LastRuntimeUpgradeInfo::decode_all(&mut &bytes[..])
        .map_err(|_| "invalid runtime-upgrade metadata")?;
    ensure!(
        last.spec_name == crate::VERSION.spec_name,
        "unsupported predecessor spec_name"
    );
    if last.spec_version.0 >= MIGRATION_SPEC_VERSION {
        return Ok(false);
    }
    ensure!(
        last.spec_version.0 == PREDECESSOR_SPEC_VERSION
            && crate::VERSION.spec_version == MIGRATION_SPEC_VERSION,
        "unsupported session-key migration version"
    );
    Ok(true)
}

fn validate_legacy_storage(reads: &mut u64) -> Result<(u64, u64), &'static str> {
    *reads = reads.saturating_add(1);
    let genesis = sp_io::storage::get(&pallet_beefy::GenesisBlock::<Runtime>::hashed_key());
    validate_beefy_genesis(genesis.as_deref(), true)?;
    inspect_map(
        &pallet_session::KeyOwner::<Runtime>::final_prefix(),
        reads,
        |key, value, _| validate_owner_entry(key, value),
    )?;
    let registered = inspect_map(
        &pallet_session::NextKeys::<Runtime>::final_prefix(),
        reads,
        |key, value, reads| {
            let validator = decode_validator_key(key)?;
            let old = decode_old_keys(value)?;
            validate_registered_keys(&validator, &old, reads)?;
            validate_placeholder_owner(&validator, reads)
        },
    )?;
    *reads = reads.saturating_add(1);
    let queued = match sp_io::storage::get(&pallet_session::QueuedKeys::<Runtime>::hashed_key()) {
        Some(bytes) => decode_old_queue(&bytes)?.len() as u64,
        None => 0,
    };
    Ok((registered, queued))
}

fn validate_beefy_genesis(
    bytes: Option<&[u8]>,
    require_inactive: bool,
) -> Result<(), &'static str> {
    if let Some(bytes) = bytes {
        let genesis = Option::<crate::BlockNumber>::decode_all(&mut &bytes[..])
            .map_err(|_| "invalid BEEFY activation storage")?;
        ensure!(
            !require_inactive || genesis.is_none(),
            "BEEFY is active before key preparation"
        );
    }
    Ok(())
}

// Typed SDK iterators skip malformed records. Walk raw keys so no original entry
// can disappear from preflight or its preservation snapshot. Both maps use Twox64Concat.
fn inspect_map(
    prefix: &[u8],
    reads: &mut u64,
    mut inspect: impl FnMut(&[u8], &[u8], &mut u64) -> Result<(), &'static str>,
) -> Result<u64, &'static str> {
    *reads = reads.saturating_add(1);
    ensure!(
        sp_io::storage::get(prefix).is_none(),
        "invalid session-map prefix entry"
    );
    let mut previous = prefix.to_vec();
    let mut count = 0u64;
    loop {
        *reads = reads.saturating_add(1);
        let Some(key) = sp_io::storage::next_key(&previous) else {
            break;
        };
        if !key.starts_with(prefix) {
            break;
        }
        *reads = reads.saturating_add(1);
        let value = sp_io::storage::get(&key).ok_or("session-map entry disappeared")?;
        inspect(&key, &value, reads)?;
        previous = key;
        count = count.saturating_add(1);
    }
    Ok(count)
}

fn decode_map_key<K: DecodeAll>(key: &[u8]) -> Result<K, &'static str> {
    let material = key.get(32 + 8..).ok_or("invalid session-map key")?;
    K::decode_all(&mut &material[..]).map_err(|_| "invalid or trailing session-map key")
}

fn decode_validator_key(key: &[u8]) -> Result<crate::AccountId, &'static str> {
    let validator = decode_map_key::<crate::AccountId>(key)?;
    ensure!(
        key == pallet_session::NextKeys::<Runtime>::hashed_key_for(&validator),
        "noncanonical NextKeys storage key"
    );
    Ok(validator)
}

fn validate_owner_entry(key: &[u8], value: &[u8]) -> Result<(), &'static str> {
    let owner_key = decode_map_key::<(sp_core::crypto::KeyTypeId, Vec<u8>)>(key)?;
    ensure!(
        key == pallet_session::KeyOwner::<Runtime>::hashed_key_for(owner_key),
        "noncanonical KeyOwner storage key"
    );
    crate::AccountId::decode_all(&mut &value[..])
        .map_err(|_| "invalid or trailing KeyOwner value")?;
    Ok(())
}

fn read_owner(
    key_type: sp_core::crypto::KeyTypeId,
    raw: &[u8],
    reads: &mut u64,
) -> Result<Option<crate::AccountId>, &'static str> {
    *reads = reads.saturating_add(1);
    sp_io::storage::get(&pallet_session::KeyOwner::<Runtime>::hashed_key_for((
        key_type, raw,
    )))
    .map(|bytes| {
        crate::AccountId::decode_all(&mut &bytes[..])
            .map_err(|_| "invalid or trailing KeyOwner value")
    })
    .transpose()
}

fn validate_registered_keys<K: OpaqueKeys>(
    validator: &crate::AccountId,
    keys: &K,
    reads: &mut u64,
) -> Result<(), &'static str> {
    for key_type in K::key_ids() {
        ensure!(
            read_owner(*key_type, keys.get_raw(*key_type), reads)?.as_ref() == Some(validator),
            "original session key has an incorrect or missing owner"
        );
    }
    Ok(())
}

fn validate_placeholder_owner(
    validator: &crate::AccountId,
    reads: &mut u64,
) -> Result<(), &'static str> {
    let placeholder = placeholder_beefy_key(validator);
    if let Some(owner) = read_owner(sp_consensus_beefy::KEY_TYPE, placeholder.as_ref(), reads)? {
        ensure!(
            &owner == validator,
            "placeholder BEEFY key collides with an existing owner"
        );
    }
    Ok(())
}

fn decode_old_keys(value: &[u8]) -> Result<SessionKeysOld, &'static str> {
    SessionKeysOld::decode_all(&mut &value[..])
        .map_err(|_| "NextKeys is not the exact four-key layout")
}

fn decode_old_queue(value: &[u8]) -> Result<Vec<(crate::AccountId, SessionKeysOld)>, &'static str> {
    let count = parity_scale_codec::Compact::<u32>::decode(&mut &value[..])
        .map_err(|_| "invalid queued authority length")?
        .0;
    ensure!(
        count <= crate::MaxActiveValidators::get(),
        "queued committee exceeds supported capacity"
    );
    Vec::<(crate::AccountId, SessionKeysOld)>::decode_all(&mut &value[..])
        .map_err(|_| "QueuedKeys is not the exact four-key layout")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BeefyId, session_history::SessionKeyOwnerProof};
    use frame_support::traits::{KeyOwnerProofSystem, OnFinalize, OnInitialize};
    use sp_core::Pair;
    use sp_runtime::{StateVersion, traits::BlakeTwo256};
    use sp_trie::{LayoutV0, Trie, TrieDBBuilder};

    fn validator(seed: u8) -> crate::AccountId {
        crate::AccountId::new([seed; 32])
    }

    fn old_keys(seed: u8) -> SessionKeysOld {
        SessionKeysOld {
            babe: sp_consensus_babe::AuthorityId::from(sp_core::sr25519::Public::from_raw(
                [seed; 32],
            )),
            grandpa: sp_consensus_grandpa::AuthorityId::from(sp_core::ed25519::Public::from_raw(
                [seed.wrapping_add(1); 32],
            )),
            im_online: pallet_im_online::sr25519::AuthorityId::from(
                sp_core::sr25519::Public::from_raw([seed.wrapping_add(2); 32]),
            ),
            authority_discovery: sp_authority_discovery::AuthorityId::from(
                sp_core::sr25519::Public::from_raw([seed.wrapping_add(3); 32]),
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

    fn assert_original_keys(keys: &SessionKeys, old: &SessionKeysOld) {
        assert_eq!(keys.babe, old.babe);
        assert_eq!(keys.grandpa, old.grandpa);
        assert_eq!(keys.im_online, old.im_online);
        assert_eq!(keys.authority_discovery, old.authority_discovery);
    }

    type OwnedProof = (
        sp_core::crypto::KeyTypeId,
        Vec<u8>,
        sp_session::MembershipProof,
        pallet_session_historical::IdentificationTuple<Runtime>,
    );

    fn legacy_history(
        entries: &[(crate::AccountId, SessionKeysOld)],
        era: u32,
        session: u32,
    ) -> (HistoricalRoot, Vec<OwnedProof>) {
        use sp_trie::{
            MemoryDB, Recorder, TrieMut, recorder_ext::RecorderExt, trie_types::TrieDBMutBuilderV0,
        };
        let mut db = MemoryDB::<BlakeTwo256>::default();
        let mut root = Default::default();
        {
            let mut trie = TrieDBMutBuilderV0::<BlakeTwo256>::new(&mut db, &mut root).build();
            for (position, (account, keys)) in entries.iter().enumerate() {
                for key_type in SessionKeysOld::key_ids() {
                    trie.insert(
                        &(*key_type, keys.get_raw(*key_type)).encode(),
                        &(position as u32).encode(),
                    )
                    .unwrap();
                }
                let exposure = pallet_staking::Pallet::<Runtime>::eras_stakers(era, account);
                trie.insert(&(position as u32).encode(), &(account, exposure).encode())
                    .unwrap();
            }
        }
        let mut proofs = Vec::new();
        for (account, keys) in entries {
            for key_type in [sp_consensus_babe::KEY_TYPE, sp_consensus_grandpa::KEY_TYPE] {
                let raw = keys.get_raw(key_type).to_vec();
                let mut recorder = Recorder::<LayoutV0<BlakeTwo256>>::new();
                {
                    let trie = TrieDBBuilder::<LayoutV0<BlakeTwo256>>::new(&db, &root)
                        .with_recorder(&mut recorder)
                        .build();
                    let index = trie.get(&(key_type, &raw[..]).encode()).unwrap().unwrap();
                    trie.get(&index).unwrap().unwrap();
                }
                proofs.push((
                    key_type,
                    raw,
                    sp_session::MembershipProof {
                        session,
                        trie_nodes: recorder.into_raw_storage_proof(),
                        validator_count: entries.len() as u32,
                    },
                    (
                        account.clone(),
                        pallet_staking::Pallet::<Runtime>::eras_stakers(era, account),
                    ),
                ));
            }
        }
        ((root, entries.len() as u32), proofs)
    }

    fn assert_owned_proofs(proofs: &[OwnedProof]) {
        for (key_type, raw, proof, owner) in proofs {
            assert_eq!(
                SessionKeyOwnerProof::check_proof((*key_type, raw), proof.clone()),
                Some(owner.clone())
            );
        }
    }

    #[test]
    fn session_keys_historical_proofs_survive_rotations_and_era_boundary() {
        for (new_era, pending) in [(false, false), (false, true), (true, false), (true, true)] {
            sp_io::TestExternalities::default().execute_with(|| {
                set_last_runtime_upgrade(PREDECESSOR_SPEC_VERSION);
                crate::System::set_block_number(1);
                pallet_session::CurrentIndex::<Runtime>::put(17);
                let entries = [(validator(1), old_keys(1)), (validator(2), old_keys(2))];
                let queued = if pending {
                    [(validator(2), old_keys(22)), (validator(1), old_keys(11))]
                } else {
                    [(validator(2), old_keys(2)), (validator(1), old_keys(1))]
                };
                let registered = if pending {
                    [(validator(1), old_keys(31)), (validator(2), old_keys(32))]
                } else {
                    entries.clone()
                };
                seed_old_next_keys(&registered);
                seed_old_queued_keys(&queued);
                <(Babe, Grandpa, ImOnline, AuthorityDiscovery) as pallet_session::SessionHandler<
                    crate::AccountId,
                >>::on_genesis_session(&entries);
                pallet_session::Validators::<Runtime>::put(
                    entries
                        .iter()
                        .map(|(account, _)| account.clone())
                        .collect::<Vec<_>>(),
                );
                pallet_session::QueuedChanged::<Runtime>::put(true);
                pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
                    index: 0,
                    start: None,
                });
                let queued_era = u32::from(new_era);
                pallet_staking::CurrentEra::<Runtime>::put(queued_era);
                pallet_staking::ErasStartSessionIndex::<Runtime>::insert(0, 0);
                if new_era {
                    pallet_staking::ErasStartSessionIndex::<Runtime>::insert(1, 18);
                }
                pallet_staking::ForceEra::<Runtime>::put(pallet_staking::Forcing::ForceNone);
                frame_support::storage::unhashed::put(
                    &frame_support::storage::storage_prefix(b"Staking", b"BondedEras"),
                    &vec![(0u32, 0u32)],
                );
                for era in 0..=queued_era {
                    for (account, _) in &entries {
                        pallet_staking::ErasStakers::<Runtime>::insert(
                            era,
                            account,
                            pallet_staking::Exposure {
                                total: 200 * (era as u128 + 1),
                                own: 100 * (era as u128 + 1),
                                others: vec![pallet_staking::IndividualExposure {
                                    who: validator(99),
                                    value: 100 * (era as u128 + 1),
                                }],
                            },
                        );
                    }
                }
                let (current_root, legacy_current) = legacy_history(&entries, 0, 17);
                let (queued_root, legacy_queued) = legacy_history(&queued, queued_era, 18);
                for (index, root) in [(16, current_root), (17, current_root), (18, queued_root)] {
                    pallet_session_historical::HistoricalSessions::<Runtime>::insert(index, root);
                }
                pallet_session_historical::StoredRange::<Runtime>::put((16, 19));
                let mut older = legacy_current.clone();
                for (_, _, proof, _) in &mut older {
                    proof.session = 16;
                }
                #[cfg(feature = "try-runtime")]
                let state = MigrateSessionKeys::pre_upgrade().unwrap();
                MigrateSessionKeys::on_runtime_upgrade();
                #[cfg(feature = "try-runtime")]
                {
                    MigrateSessionKeys::post_upgrade(state.clone()).unwrap();
                    for legacy in [false, true] {
                        let original = if legacy {
                            LegacySessionRoots::get(17)
                        } else {
                            Historical::historical_root(17)
                        }
                        .unwrap();
                        let corrupt = (sp_core::H256::repeat_byte(99), original.1);
                        if legacy {
                            LegacySessionRoots::insert(17, corrupt);
                        } else {
                            pallet_session_historical::HistoricalSessions::<Runtime>::insert(
                                17, corrupt,
                            );
                        }
                        assert!(MigrateSessionKeys::post_upgrade(state.clone()).is_err());
                        if legacy {
                            LegacySessionRoots::insert(17, original);
                        } else {
                            pallet_session_historical::HistoricalSessions::<Runtime>::insert(
                                17, original,
                            );
                        }
                    }
                }
                assert_owned_proofs(&older);
                assert_owned_proofs(&legacy_current);
                for (kind, key, proof, _) in &legacy_queued {
                    assert!(
                        SessionKeyOwnerProof::check_proof((*kind, key), proof.clone()).is_none()
                    );
                }

                let mut current = Vec::new();
                for (account, old) in &entries {
                    let keys = migrate_keys(account.clone(), old.clone());
                    for key_type in [
                        sp_consensus_babe::KEY_TYPE,
                        sp_consensus_grandpa::KEY_TYPE,
                        sp_consensus_beefy::KEY_TYPE,
                    ] {
                        let raw = keys.get_raw(key_type).to_vec();
                        let proof = SessionKeyOwnerProof::prove((key_type, &raw)).unwrap();
                        current.push((
                            key_type,
                            raw,
                            proof,
                            (
                                account.clone(),
                                pallet_staking::Pallet::<Runtime>::eras_stakers(0, account),
                            ),
                        ));
                    }
                }
                assert_owned_proofs(&current);
                let (key_type, raw, proof, _) = &legacy_current[0];
                let mut wrong_count = proof.clone();
                wrong_count.validator_count += 1;
                let mut duplicate = proof.clone();
                duplicate.trie_nodes.push(duplicate.trie_nodes[0].clone());
                let mut unused = proof.clone();
                unused.trie_nodes.push(vec![99]);
                for invalid in [wrong_count, duplicate, unused] {
                    assert_eq!(
                        SessionKeyOwnerProof::check_proof((*key_type, raw), invalid),
                        None
                    );
                }
                assert_eq!(
                    SessionKeyOwnerProof::check_proof((*key_type, vec![99; 32]), proof.clone()),
                    None
                );

                crate::Babe::on_initialize(crate::System::block_number());
                crate::Session::rotate_session();
                assert_eq!(crate::Session::current_index(), 18);
                assert_owned_proofs(&older);
                assert_owned_proofs(&legacy_current);
                assert_owned_proofs(&legacy_queued);
                assert_owned_proofs(&current);
                let account = validator(1);
                let keys = migrate_keys(account.clone(), queued[1].1.clone());
                let mut next = Vec::new();
                for key_type in [
                    sp_consensus_babe::KEY_TYPE,
                    sp_consensus_grandpa::KEY_TYPE,
                    sp_consensus_beefy::KEY_TYPE,
                ] {
                    let raw = keys.get_raw(key_type).to_vec();
                    next.push((
                        key_type,
                        raw.clone(),
                        SessionKeyOwnerProof::prove((key_type, &raw)).unwrap(),
                        (
                            account.clone(),
                            pallet_staking::Pallet::<Runtime>::eras_stakers(queued_era, &account),
                        ),
                    ));
                }
                assert_owned_proofs(&next);
                crate::System::set_block_number(2);
                crate::Babe::on_initialize(crate::System::block_number());
                crate::Session::rotate_session();
                assert_eq!(crate::Session::current_index(), 19);
                assert_owned_proofs(&legacy_current);
                assert_owned_proofs(&legacy_queued);
                assert_owned_proofs(&current);
                assert_owned_proofs(&next);
                for (owner, keys) in ActiveSessionKeys::get() {
                    for kind in SessionKeys::key_ids() {
                        let proof =
                            SessionKeyOwnerProof::prove((*kind, keys.get_raw(*kind))).unwrap();
                        assert_eq!(
                            SessionKeyOwnerProof::check_proof((*kind, keys.get_raw(*kind)), proof)
                                .unwrap()
                                .0,
                            owner
                        );
                    }
                }

                Historical::prune_up_to(18);
                for (key_type, raw, proof, _) in legacy_current.iter().chain(&current).chain(&older)
                {
                    assert_eq!(
                        SessionKeyOwnerProof::check_proof((*key_type, raw), proof.clone()),
                        None
                    );
                }
                assert_owned_proofs(&legacy_queued);
                assert_owned_proofs(&next);
            });
        }
    }

    #[test]
    fn session_keys_rejects_unauthenticated_active_grandpa_before_any_write() {
        for applied in [false, true] {
            sp_io::TestExternalities::default().execute_with(|| {
                set_last_runtime_upgrade(PREDECESSOR_SPEC_VERSION);
                crate::System::set_block_number(1);
                let original = [(validator(1), old_keys(1)), (validator(2), old_keys(2))];
                let reordered = [original[1].clone(), original[0].clone()];
                <(Babe, Grandpa, ImOnline, AuthorityDiscovery) as pallet_session::SessionHandler<
                    crate::AccountId,
                >>::on_genesis_session(&original);
                // A supported-predecessor forced change remains pending across a session change.
                Grandpa::schedule_change(Grandpa::grandpa_authorities(), 5, Some(0)).unwrap();
                pallet_session::CurrentIndex::<Runtime>::put(18);
                pallet_session::Validators::<Runtime>::put(
                    reordered
                        .iter()
                        .map(|(owner, _)| owner.clone())
                        .collect::<Vec<_>>(),
                );
                Babe::on_initialize(crate::System::block_number());
                <(Babe, Grandpa, ImOnline, AuthorityDiscovery) as pallet_session::SessionHandler<
                    crate::AccountId,
                >>::on_new_session(true, &reordered, &reordered);
                if applied {
                    crate::System::set_block_number(6);
                    Grandpa::on_finalize(6);
                }
                assert_eq!(Grandpa::pending_change().is_none(), applied);
                assert!(
                    sp_io::storage::get(&frame_support::storage::storage_prefix(
                        b"Grandpa", b"Stalled",
                    ))
                    .is_none()
                );
                pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo {
                    index: 0,
                    start: None,
                });
                pallet_staking::CurrentEra::<Runtime>::put(0);
                // Future registrations cannot authenticate the active keys' owners.
                seed_old_next_keys(&[(validator(1), old_keys(31)), (validator(2), old_keys(32))]);
                seed_old_queued_keys(&reordered);
                let (legacy, proofs) = legacy_history(&reordered, 0, 18);
                pallet_session_historical::HistoricalSessions::<Runtime>::insert(18, legacy);
                pallet_session_historical::HistoricalSessions::<Runtime>::insert(19, legacy);
                pallet_session_historical::StoredRange::<Runtime>::put((18, 20));
                assert_owned_proofs(&proofs);
                // The old positional reconstruction would assign validator 1's GRANDPA key to 2.
                let reconstructed = active_keys(&mut 0).unwrap();
                assert_eq!(reconstructed[0].0, validator(2));
                assert_eq!(reconstructed[0].1.grandpa, original[0].1.grandpa);
                let before = sp_io::storage::root(StateVersion::V1);
                #[cfg(feature = "try-runtime")]
                assert!(MigrateSessionKeys::pre_upgrade().is_err());
                assert!(std::panic::catch_unwind(MigrateSessionKeys::on_runtime_upgrade).is_err());
                assert_eq!(sp_io::storage::root(StateVersion::V1), before);
            });
        }
    }

    #[test]
    fn session_keys_historical_corruption_rejects_before_any_write() {
        let mut keys =
            vec![pallet_session_historical::HistoricalSessions::<Runtime>::hashed_key_for(0)];
        for (pallet, item) in [
            (b"Session".as_slice(), b"Validators".as_slice()),
            (b"Babe", b"Authorities"),
            (b"Grandpa", b"Authorities"),
            (b"ImOnline", b"Keys"),
            (b"AuthorityDiscovery", b"Keys"),
        ] {
            keys.push(frame_support::storage::storage_prefix(pallet, item).to_vec());
        }
        for key in keys {
            sp_io::TestExternalities::default().execute_with(|| {
                set_last_runtime_upgrade(PREDECESSOR_SPEC_VERSION);
                seed_old_next_keys(&[(validator(1), old_keys(1))]);
                sp_io::storage::set(&key, &[1]);
                let before = sp_io::storage::root(StateVersion::V1);
                #[cfg(feature = "try-runtime")]
                assert!(MigrateSessionKeys::pre_upgrade().is_err());
                assert!(std::panic::catch_unwind(MigrateSessionKeys::on_runtime_upgrade).is_err());
                assert_eq!(sp_io::storage::root(StateVersion::V1), before);
            });
        }
    }

    #[test]
    fn session_keys_migration_preserves_legacy_bridge_and_staking_intentions() {
        for paused in [false, true] {
            sp_io::TestExternalities::default().execute_with(|| {
                set_last_runtime_upgrade(PREDECESSOR_SPEC_VERSION);
                let owner = validator(1);
                seed_old_next_keys(&[(owner.clone(), old_keys(1))]);
                let prefs = pallet_staking::ValidatorPrefs {
                    commission: sp_runtime::Perbill::from_percent(7),
                    blocked: true,
                };
                pallet_staking::Validators::<Runtime>::insert(&owner, &prefs);
                let queue = vec![sp_core::H256::repeat_byte(3), sp_core::H256::repeat_byte(4)];
                let bridge = [
                    (b"Initialized".as_slice(), true.encode()),
                    (b"Paused", paused.encode()),
                    (b"Queue", queue.encode()),
                    (b"QueueId", 19u64.encode()),
                    (b"QueueMerkleRoot", sp_core::H256::repeat_byte(5).encode()),
                    (b"AuthoritySetHash", sp_core::H256::repeat_byte(6).encode()),
                    (b"MessageNonce", sp_core::U256::from(23).encode()),
                    (b"ClearTimer", 2u32.encode()),
                ]
                .map(|(item, value)| {
                    (
                        frame_support::storage::storage_prefix(b"GearEthBridge", item),
                        value,
                    )
                });
                for (key, value) in &bridge {
                    sp_io::storage::set(key, value);
                }
                #[cfg(feature = "try-runtime")]
                let snapshot = MigrateSessionKeys::pre_upgrade().unwrap();
                MigrateSessionKeys::on_runtime_upgrade();
                #[cfg(feature = "try-runtime")]
                MigrateSessionKeys::post_upgrade(snapshot).unwrap();
                for (key, value) in &bridge {
                    assert_eq!(sp_io::storage::get(key).unwrap().as_ref(), value.as_slice());
                }
                assert_eq!(pallet_staking::Validators::<Runtime>::get(&owner), prefs);
                assert!(pallet_beefy::GenesisBlock::<Runtime>::get().is_none());
            });
        }
    }

    #[test]
    fn session_keys_oversized_committee_rejects_before_any_write() {
        sp_io::TestExternalities::default().execute_with(|| {
            set_last_runtime_upgrade(PREDECESSOR_SPEC_VERSION);
            seed_old_queued_keys(&vec![(validator(1), old_keys(1)); 1_001]);
            let before = sp_io::storage::root(StateVersion::V1);
            #[cfg(feature = "try-runtime")]
            assert!(MigrateSessionKeys::pre_upgrade().is_err());
            assert!(std::panic::catch_unwind(MigrateSessionKeys::on_runtime_upgrade).is_err());
            assert_eq!(sp_io::storage::root(StateVersion::V1), before);
        });
    }

    #[test]
    fn session_keys_preserves_registered_queued_and_ownership_then_noops() {
        sp_io::TestExternalities::default().execute_with(|| {
            set_last_runtime_upgrade(11_000);
            pallet_session::CurrentIndex::<Runtime>::put(17);
            let entries = [(validator(1), old_keys(1)), (validator(2), old_keys(2))];
            let queued_before = [(validator(2), old_keys(21)), (validator(1), old_keys(11))];
            seed_old_next_keys(&entries);
            seed_old_queued_keys(&queued_before);
            let unrelated = (sp_consensus_babe::KEY_TYPE, vec![99; 32]);
            pallet_session::KeyOwner::<Runtime>::insert(&unrelated, validator(99));
            pallet_beefy::GenesisBlock::<Runtime>::put(None::<crate::BlockNumber>);
            #[cfg(feature = "try-runtime")]
            let state = MigrateSessionKeys::pre_upgrade().unwrap();

            MigrateSessionKeys::on_runtime_upgrade();

            for (validator, old) in &entries {
                let keys = pallet_session::NextKeys::<Runtime>::get(validator).unwrap();
                assert_original_keys(&keys, old);
                assert_eq!(keys.beefy, placeholder_beefy_key(validator));
                for key_type in SessionKeys::key_ids() {
                    assert_eq!(
                        pallet_session::KeyOwner::<Runtime>::get((
                            *key_type,
                            keys.get_raw(*key_type).to_vec(),
                        )),
                        Some(validator.clone())
                    );
                }
            }
            let mut queued = pallet_session::QueuedKeys::<Runtime>::get();
            assert_eq!(queued.len(), queued_before.len());
            for ((account, keys), (original_account, original_keys)) in
                queued.iter().zip(&queued_before)
            {
                assert_eq!(account, original_account);
                assert_original_keys(keys, original_keys);
                assert_eq!(keys.beefy, placeholder_beefy_key(account));
            }
            assert_eq!(
                pallet_session::KeyOwner::<Runtime>::get(&unrelated),
                Some(validator(99))
            );
            assert_eq!(
                sp_io::storage::get(&pallet_beefy::GenesisBlock::<Runtime>::hashed_key())
                    .unwrap()
                    .as_ref(),
                &[0]
            );
            assert!(crate::Beefy::validator_set().is_none());
            #[cfg(feature = "try-runtime")]
            frame_support::assert_ok!(crate::Beefy::do_try_state());
            assert_eq!(pallet_beefy::SetIdSession::<Runtime>::get(0), Some(17));
            #[cfg(feature = "try-runtime")]
            MigrateSessionKeys::post_upgrade(state).unwrap();

            for (index, (account, _)) in entries.iter().enumerate() {
                let mut keys = pallet_session::NextKeys::<Runtime>::get(account).unwrap();
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
                    account,
                );
                pallet_session::NextKeys::<Runtime>::insert(account, &keys);
                queued[index].1.beefy = keys.beefy;
            }
            pallet_session::QueuedKeys::<Runtime>::put(queued);
            pallet_beefy::GenesisBlock::<Runtime>::put(Some(123));
            for version in [MIGRATION_SPEC_VERSION, MIGRATION_SPEC_VERSION + 1] {
                set_last_runtime_upgrade(version);
                let before = sp_io::storage::root(StateVersion::V1);
                #[cfg(feature = "try-runtime")]
                let state = MigrateSessionKeys::pre_upgrade().unwrap();
                MigrateSessionKeys::on_runtime_upgrade();
                assert_eq!(sp_io::storage::root(StateVersion::V1), before);
                #[cfg(feature = "try-runtime")]
                MigrateSessionKeys::post_upgrade(state).unwrap();
            }
        });
    }

    #[test]
    fn session_keys_preserves_existing_inactive_beefy_bookkeeping() {
        sp_io::TestExternalities::default().execute_with(|| {
            set_last_runtime_upgrade(11_000);
            pallet_session::CurrentIndex::<Runtime>::put(17);
            let authorities = vec![current_keys(1).beefy].try_into().unwrap();
            let next_authorities = vec![current_keys(2).beefy].try_into().unwrap();
            pallet_beefy::Authorities::<Runtime>::set(authorities);
            pallet_beefy::NextAuthorities::<Runtime>::set(next_authorities);
            pallet_beefy::ValidatorSetId::<Runtime>::put(7);
            pallet_beefy::SetIdSession::<Runtime>::insert(7, 11);
            let before = (
                pallet_beefy::Authorities::<Runtime>::get(),
                pallet_beefy::NextAuthorities::<Runtime>::get(),
                pallet_beefy::ValidatorSetId::<Runtime>::get(),
                pallet_beefy::SetIdSession::<Runtime>::get(7),
            );

            MigrateSessionKeys::on_runtime_upgrade();

            assert_eq!(
                (
                    pallet_beefy::Authorities::<Runtime>::get(),
                    pallet_beefy::NextAuthorities::<Runtime>::get(),
                    pallet_beefy::ValidatorSetId::<Runtime>::get(),
                    pallet_beefy::SetIdSession::<Runtime>::get(7),
                ),
                before
            );
            assert_eq!(pallet_beefy::GenesisBlock::<Runtime>::get(), None);
            #[cfg(feature = "try-runtime")]
            frame_support::assert_ok!(crate::Beefy::do_try_state());
        });
    }

    #[test]
    fn session_keys_preserves_queue_without_registered_keys() {
        sp_io::TestExternalities::default().execute_with(|| {
            set_last_runtime_upgrade(11_000);
            let original = [(validator(2), old_keys(21)), (validator(1), old_keys(11))];
            seed_old_queued_keys(&original);
            #[cfg(feature = "try-runtime")]
            let state = MigrateSessionKeys::pre_upgrade().unwrap();
            MigrateSessionKeys::on_runtime_upgrade();
            let queued = pallet_session::QueuedKeys::<Runtime>::get();
            assert_eq!(queued.len(), original.len());
            for ((account, keys), (old_account, old)) in queued.iter().zip(&original) {
                assert_eq!(account, old_account);
                assert_original_keys(keys, old);
                assert_eq!(keys.beefy, placeholder_beefy_key(account));
            }
            assert_eq!(
                pallet_session::KeyOwner::<Runtime>::iter().collect::<Vec<_>>(),
                vec![]
            );
            assert_eq!(pallet_beefy::GenesisBlock::<Runtime>::get(), None);
            #[cfg(feature = "try-runtime")]
            MigrateSessionKeys::post_upgrade(state).unwrap();
        });
    }

    #[test]
    fn session_keys_empty_state_without_upgrade_record_stays_empty() {
        sp_io::TestExternalities::default().execute_with(|| {
            let before = sp_io::storage::root(StateVersion::V1);
            #[cfg(feature = "try-runtime")]
            let state = MigrateSessionKeys::pre_upgrade().unwrap();
            MigrateSessionKeys::on_runtime_upgrade();
            assert_eq!(sp_io::storage::root(StateVersion::V1), before);
            #[cfg(feature = "try-runtime")]
            MigrateSessionKeys::post_upgrade(state).unwrap();
        });
    }

    #[test]
    fn session_keys_missing_upgrade_record_never_consumes_current_keys() {
        sp_io::TestExternalities::default().execute_with(|| {
            let account = validator(1);
            let mut keys = current_keys(1);
            keys.beefy = BeefyId::from(sp_core::ecdsa::Pair::from_seed(&[10; 32]).public());
            pallet_session::NextKeys::<Runtime>::insert(&account, &keys);
            pallet_session::QueuedKeys::<Runtime>::put(vec![(account.clone(), keys.clone())]);
            for key_type in SessionKeys::key_ids() {
                pallet_session::KeyOwner::<Runtime>::insert(
                    (*key_type, keys.get_raw(*key_type).to_vec()),
                    &account,
                );
            }
            let before = sp_io::storage::root(StateVersion::V1);
            #[cfg(feature = "try-runtime")]
            assert!(MigrateSessionKeys::pre_upgrade().is_err());
            assert!(std::panic::catch_unwind(MigrateSessionKeys::on_runtime_upgrade).is_err());
            assert_eq!(sp_io::storage::root(StateVersion::V1), before);
        });
    }

    #[test]
    fn session_keys_corrupt_queue_cannot_partially_migrate_registered_keys() {
        sp_io::TestExternalities::default().execute_with(|| {
            set_last_runtime_upgrade(11_000);
            seed_old_next_keys(&[(validator(1), old_keys(1)), (validator(2), old_keys(2))]);
            sp_io::storage::set(
                &pallet_session::QueuedKeys::<Runtime>::hashed_key(),
                &[0xff],
            );
            let before = sp_io::storage::root(StateVersion::V1);
            assert!(
                std::panic::catch_unwind(MigrateSessionKeys::on_runtime_upgrade).is_err(),
                "invalid queue must reject preparation before its first write"
            );
            assert_eq!(sp_io::storage::root(StateVersion::V1), before);
        });
    }

    #[cfg(feature = "try-runtime")]
    #[test]
    fn session_keys_pre_upgrade_rejects_unsupported_raw_layouts() {
        for case in 0..13 {
            sp_io::TestExternalities::default().execute_with(|| {
                set_last_runtime_upgrade(11_000);
                let account = validator(1);
                let old = old_keys(1);
                seed_old_next_keys(&[(account.clone(), old.clone())]);
                seed_old_queued_keys(&[(account.clone(), old.clone())]);
                let next_key = pallet_session::NextKeys::<Runtime>::hashed_key_for(&account);
                let queue_key = pallet_session::QueuedKeys::<Runtime>::hashed_key();
                let owner_key = pallet_session::KeyOwner::<Runtime>::hashed_key_for((
                    sp_consensus_babe::KEY_TYPE,
                    old.get_raw(sp_consensus_babe::KEY_TYPE),
                ));
                match case {
                    0 => {
                        let mut bytes = old.encode();
                        bytes.push(0);
                        sp_io::storage::set(&next_key, &bytes);
                    }
                    1 => sp_io::storage::set(&next_key, &current_keys(1).encode()),
                    2 => sp_io::storage::set(&queue_key, &[0xff]),
                    3 => {
                        let mut bytes = vec![(account.clone(), old.clone())].encode();
                        bytes.push(0);
                        sp_io::storage::set(&queue_key, &bytes);
                    }
                    4 => sp_io::storage::set(
                        &queue_key,
                        &vec![(account.clone(), current_keys(1))].encode(),
                    ),
                    5 => {
                        let mut key = next_key.clone();
                        key.push(0);
                        sp_io::storage::set(&key, &old.encode());
                    }
                    6 => sp_io::storage::clear(&owner_key),
                    7 => sp_io::storage::set(&owner_key, &validator(2).encode()),
                    8 => {
                        let mut bytes = account.encode();
                        bytes.push(0);
                        sp_io::storage::set(&owner_key, &bytes);
                    }
                    9 => pallet_beefy::GenesisBlock::<Runtime>::put(Some(1)),
                    10 => {
                        let beefy_key = placeholder_beefy_key(&account);
                        pallet_session::KeyOwner::<Runtime>::insert(
                            (
                                sp_consensus_beefy::KEY_TYPE,
                                <BeefyId as AsRef<[u8]>>::as_ref(&beefy_key),
                            ),
                            validator(2),
                        );
                    }
                    11 => sp_io::storage::set(&next_key, &old.encode()[..127]),
                    12 => {
                        let mut key = owner_key.clone();
                        key[32] ^= 1;
                        sp_io::storage::set(&key, &account.encode());
                    }
                    _ => unreachable!(),
                }
                assert!(MigrateSessionKeys::pre_upgrade().is_err(), "case {case}");
            });
        }
    }

    #[cfg(feature = "try-runtime")]
    #[test]
    fn session_keys_pre_upgrade_rejects_unknown_upgrade_history() {
        for version in [None, Some(10_999), Some(11_001), Some(20_099), Some(20_100)] {
            sp_io::TestExternalities::default().execute_with(|| {
                if let Some(version) = version {
                    set_last_runtime_upgrade(version);
                }
                seed_old_next_keys(&[(validator(1), old_keys(1))]);
                assert!(MigrateSessionKeys::pre_upgrade().is_err(), "{version:?}");
            });
        }
        for case in 0..2 {
            sp_io::TestExternalities::default().execute_with(|| {
                set_last_runtime_upgrade(11_000);
                let key = frame_system::LastRuntimeUpgrade::<Runtime>::hashed_key();
                if case == 0 {
                    let mut bytes = sp_io::storage::get(&key).unwrap().to_vec();
                    bytes.push(0);
                    sp_io::storage::set(&key, &bytes);
                } else {
                    frame_system::LastRuntimeUpgrade::<Runtime>::put(
                        frame_system::LastRuntimeUpgradeInfo {
                            spec_version: 11_000.into(),
                            spec_name: sp_runtime::create_runtime_str!("other-runtime"),
                        },
                    );
                }
                assert!(MigrateSessionKeys::pre_upgrade().is_err(), "case {case}");
            });
        }
    }

    #[cfg(feature = "try-runtime")]
    #[test]
    fn session_keys_post_upgrade_detects_preservation_corruption() {
        for case in 0..9 {
            sp_io::TestExternalities::default().execute_with(|| {
                set_last_runtime_upgrade(11_000);
                let entries = [(validator(1), old_keys(1)), (validator(2), old_keys(2))];
                seed_old_next_keys(&entries);
                seed_old_queued_keys(&entries);
                let unrelated = (sp_consensus_babe::KEY_TYPE, vec![99; 32]);
                pallet_session::KeyOwner::<Runtime>::insert(&unrelated, validator(99));
                let mut state = MigrateSessionKeys::pre_upgrade().unwrap();
                MigrateSessionKeys::on_runtime_upgrade();
                let account = &entries[0].0;
                let next_key = pallet_session::NextKeys::<Runtime>::hashed_key_for(account);
                let queue_key = pallet_session::QueuedKeys::<Runtime>::hashed_key();
                match case {
                    0 => {
                        let mut keys = pallet_session::NextKeys::<Runtime>::get(account).unwrap();
                        pallet_session::KeyOwner::<Runtime>::remove((
                            sp_consensus_babe::KEY_TYPE,
                            keys.get_raw(sp_consensus_babe::KEY_TYPE),
                        ));
                        keys.babe = old_keys(3).babe;
                        pallet_session::KeyOwner::<Runtime>::insert(
                            (
                                sp_consensus_babe::KEY_TYPE,
                                keys.get_raw(sp_consensus_babe::KEY_TYPE),
                            ),
                            account,
                        );
                        pallet_session::NextKeys::<Runtime>::insert(account, keys);
                    }
                    1 => {
                        let mut queued = pallet_session::QueuedKeys::<Runtime>::get();
                        queued.swap(0, 1);
                        pallet_session::QueuedKeys::<Runtime>::put(queued);
                    }
                    2 => {
                        let mut queued = pallet_session::QueuedKeys::<Runtime>::get();
                        queued[0].1.grandpa = old_keys(3).grandpa;
                        pallet_session::QueuedKeys::<Runtime>::put(queued);
                    }
                    3 => pallet_session::KeyOwner::<Runtime>::remove(&unrelated),
                    4 => pallet_beefy::GenesisBlock::<Runtime>::put(Some(1)),
                    5 => {
                        let mut bytes = sp_io::storage::get(&next_key).unwrap().to_vec();
                        bytes.push(0);
                        sp_io::storage::set(&next_key, &bytes);
                    }
                    6 => {
                        let mut bytes = sp_io::storage::get(&queue_key).unwrap().to_vec();
                        bytes.push(0);
                        sp_io::storage::set(&queue_key, &bytes);
                    }
                    7 => {
                        let key = pallet_session::KeyOwner::<Runtime>::hashed_key_for(unrelated);
                        let mut bytes = sp_io::storage::get(&key).unwrap().to_vec();
                        bytes.push(0);
                        sp_io::storage::set(&key, &bytes);
                    }
                    8 => state.push(0),
                    _ => unreachable!(),
                }
                assert!(
                    MigrateSessionKeys::post_upgrade(state).is_err(),
                    "case {case}"
                );
            });
        }
    }
}
