// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{AccountIdOf, Balance, Config, Pallet};
use common::{
    gas_provider::{ChildrenRefs, GasNodeId, NodeLock},
    storage::MapStorage,
    LockId,
};
use frame_support::{
    pallet_prelude::*,
    storage::{storage_prefix, KeyPrefixIterator},
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade, PalletInfo},
};
use gear_core::ids::{MessageId, ProgramId, ReservationId};
use sp_runtime::traits::{Saturating, Zero};
use sp_std::collections::btree_set::BTreeSet;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub const PALLET_GEAR_MESSENGER_NAME: &str = "GearMessenger";
pub const WAITLIST_STORAGE_NAME: &str = "Waitlist";
pub const MAILBOX_STORAGE_NAME: &str = "Mailbox";
pub const DISPATCH_STASH_STORAGE_NAME: &str = "DispatchStash";

#[derive(Decode, Encode)]
pub enum OldGasNode<ExternalId, Id, Balance: Zero> {
    External {
        id: ExternalId,
        value: Balance,
        lock: Balance,
        system_reserve: Balance,
        refs: ChildrenRefs,
        consumed: bool,
    },
    Cut {
        id: ExternalId,
        value: Balance,
        lock: Balance,
    },
    Reserved {
        id: ExternalId,
        value: Balance,
        lock: Balance,
        refs: ChildrenRefs,
        consumed: bool,
    },
    SpecifiedLocal {
        parent: Id,
        value: Balance,
        lock: Balance,
        system_reserve: Balance,
        refs: ChildrenRefs,
        consumed: bool,
    },
    UnspecifiedLocal {
        parent: Id,
        lock: Balance,
        system_reserve: Balance,
    },
}

#[derive(Decode, Encode)]
pub enum GasNode<ExternalId, Id, Balance> {
    External {
        id: ExternalId,
        value: Balance,
        lock: NodeLock<Balance>,
        system_reserve: Balance,
        refs: ChildrenRefs,
        consumed: bool,
    },
    Cut {
        id: ExternalId,
        value: Balance,
        lock: NodeLock<Balance>,
    },
    Reserved {
        id: ExternalId,
        value: Balance,
        lock: NodeLock<Balance>,
        refs: ChildrenRefs,
        consumed: bool,
    },
    SpecifiedLocal {
        parent: Id,
        value: Balance,
        lock: NodeLock<Balance>,
        system_reserve: Balance,
        refs: ChildrenRefs,
        consumed: bool,
    },
    UnspecifiedLocal {
        parent: Id,
        lock: NodeLock<Balance>,
        system_reserve: Balance,
    },
}

#[derive(Decode, Default)]
pub struct WaitlistKey((ProgramId, MessageId));
impl From<WaitlistKey> for MessageId {
    fn from(val: WaitlistKey) -> Self {
        val.0 .1
    }
}

#[derive(Decode, Default)]
pub struct MailboxKey<T: Config>((T::AccountId, MessageId));
impl<T: Config> From<MailboxKey<T>> for MessageId {
    fn from(val: MailboxKey<T>) -> Self {
        val.0 .1
    }
}

pub type DispatchStashKey = MessageId;
pub(crate) type NodeId = GasNodeId<MessageId, ReservationId>;

pub struct GasNodesPrefix<T>(PhantomData<(T,)>);

impl<T: Config> frame_support::traits::StorageInstance for GasNodesPrefix<T> {
    fn pallet_prefix() -> &'static str {
        <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>().expect("No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.")
    }
    const STORAGE_PREFIX: &'static str = "GasNodes";
}

pub type Key = GasNodeId<MessageId, ReservationId>;
pub type NodeOf<T> = GasNode<AccountIdOf<T>, Key, Balance>;

// Private storage for missed blocks collection.
pub type GasNodes<T> = StorageMap<GasNodesPrefix<T>, Identity, Key, NodeOf<T>>;

// Public wrap of the missed blocks collection.
common::wrap_storage_map!(
    storage: GasNodes,
    name: GasNodesWrap,
    key: Key,
    value: NodeOf<T>
);

pub struct MigrateToV1<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T> {
    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        let version = <Pallet<T>>::on_chain_storage_version();

        Ok(version.encode())
    }

    fn on_runtime_upgrade() -> Weight {
        let current = StorageVersion::new(1);
        let onchain = Pallet::<T>::on_chain_storage_version();

        log::info!(
            "🚚 Running migration with current storage version {:?} / onchain {:?}",
            current,
            onchain
        );

        let mut db_reads = 1_u64; // already accessed onchain storage
        if current == 1 && onchain == 0 {
            let waitlist_storage_prefix = storage_prefix(
                PALLET_GEAR_MESSENGER_NAME.as_bytes(),
                WAITLIST_STORAGE_NAME.as_bytes(),
            );
            let waitlist_keys = KeyPrefixIterator::new(
                waitlist_storage_prefix.to_vec(),
                waitlist_storage_prefix.to_vec(),
                |mut key| Ok(WaitlistKey::decode(&mut key)?.into()),
            )
            .collect::<BTreeSet<MessageId>>();

            let mailbox_storage_prefix = storage_prefix(
                PALLET_GEAR_MESSENGER_NAME.as_bytes(),
                MAILBOX_STORAGE_NAME.as_bytes(),
            );
            let mailbox_keys = KeyPrefixIterator::new(
                mailbox_storage_prefix.to_vec(),
                mailbox_storage_prefix.to_vec(),
                |mut key| Ok(MailboxKey::<T>::decode(&mut key)?.into()),
            )
            .collect::<BTreeSet<MessageId>>();

            let dispatch_stash_storage_prefix = storage_prefix(
                PALLET_GEAR_MESSENGER_NAME.as_bytes(),
                DISPATCH_STASH_STORAGE_NAME.as_bytes(),
            );
            let dispatch_stash_keys = KeyPrefixIterator::new(
                dispatch_stash_storage_prefix.to_vec(),
                dispatch_stash_storage_prefix.to_vec(),
                |mut key| DispatchStashKey::decode(&mut key),
            )
            .collect::<BTreeSet<_>>();

            db_reads = db_reads
                .saturating_add(waitlist_keys.len() as u64)
                .saturating_add(mailbox_keys.len() as u64)
                .saturating_add(dispatch_stash_keys.len() as u64);

            // A function that tries to derive the actual lock type through storages scan
            // Caveat: for simplicity sake we do not scan programs reservation maps:
            // - assuming a lock to be a `Reservation` if the key is not found in other storages.
            let appropriate_lock_id = |node_id: &NodeId| -> LockId {
                match node_id {
                    NodeId::Node(msg_id) => {
                        if mailbox_keys.contains(msg_id) {
                            LockId::Mailbox
                        } else if waitlist_keys.contains(msg_id) {
                            LockId::Waitlist
                        } else if dispatch_stash_keys.contains(msg_id) {
                            LockId::DispatchStash
                        } else {
                            // Likely unreachable branch
                            LockId::Reservation
                        }
                    }
                    _ => LockId::Reservation,
                }
            };

            let mut translated = 0_u64;
            GasNodes::<T>::translate::<OldGasNode<AccountIdOf<T>, NodeId, Balance>, _>(
                |key, old_value| {
                    let mut new_lock = NodeLock::<Balance>::zero();
                    let new_value = match old_value {
                        OldGasNode::External {
                            id,
                            value,
                            lock,
                            system_reserve,
                            refs,
                            consumed,
                        } => {
                            if !lock.is_zero() {
                                new_lock[appropriate_lock_id(&key)] = lock;
                            }
                            GasNode::External {
                                id,
                                value,
                                lock: new_lock,
                                system_reserve,
                                refs,
                                consumed,
                            }
                        }
                        OldGasNode::Cut { id, value, lock } => {
                            if !lock.is_zero() {
                                new_lock[appropriate_lock_id(&key)] = lock;
                            }
                            GasNode::Cut {
                                id,
                                value,
                                lock: new_lock,
                            }
                        }
                        OldGasNode::Reserved {
                            id,
                            value,
                            lock,
                            refs,
                            consumed,
                        } => {
                            if !lock.is_zero() {
                                new_lock[appropriate_lock_id(&key)] = lock;
                            }
                            GasNode::Reserved {
                                id,
                                value,
                                lock: new_lock,
                                refs,
                                consumed,
                            }
                        }
                        OldGasNode::SpecifiedLocal {
                            parent,
                            value,
                            lock,
                            system_reserve,
                            refs,
                            consumed,
                        } => {
                            if !lock.is_zero() {
                                new_lock[appropriate_lock_id(&key)] = lock;
                            }
                            GasNode::SpecifiedLocal {
                                parent,
                                value,
                                lock: new_lock,
                                system_reserve,
                                refs,
                                consumed,
                            }
                        }
                        OldGasNode::UnspecifiedLocal {
                            parent,
                            lock,
                            system_reserve,
                        } => {
                            if !lock.is_zero() {
                                new_lock[appropriate_lock_id(&key)] = lock;
                            }
                            GasNode::UnspecifiedLocal {
                                parent,
                                lock: new_lock,
                                system_reserve,
                            }
                        }
                    };
                    translated.saturating_inc();
                    Some(new_value)
                },
            );
            current.put::<Pallet<T>>();
            log::info!(
                "Upgraded {} gas nodes, storage to version {:?}",
                translated,
                current
            );
            T::DbWeight::get().reads_writes(translated + db_reads, translated + 1)
        } else {
            log::info!("❌ Migration did not execute. This probably should be removed");
            T::DbWeight::get().reads(db_reads)
        }
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
        let old_version: StorageVersion =
            Decode::decode(&mut state.as_ref()).map_err(|_| "Cannot decode version")?;
        let onchain_version = Pallet::<T>::on_chain_storage_version();
        assert_ne!(
            onchain_version, old_version,
            "must have upgraded from version 0 to 1."
        );

        log::info!("Storage successfully migrated.");
        Ok(())
    }
}
