// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use crate::{AccountIdOf, Balance, Config, GasNodes, Pallet};

pub mod v1 {
    use super::*;
    use common::{
        gas_provider::{ChildrenRefs, GasNode, GasNodeId, NodeLock},
        LockId,
    };
    use frame_support::{
        pallet_prelude::*,
        storage::{storage_prefix, KeyPrefixIterator},
        traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
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

    pub struct MigrateToV1<T>(sp_std::marker::PhantomData<T>);
    impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T> {
        #[cfg(feature = "try-runtime")]
        fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
            let version = <Pallet<T>>::on_chain_storage_version();

            Ok(version.encode())
        }

        fn on_runtime_upgrade() -> Weight {
            let current = Pallet::<T>::current_storage_version();
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
}

#[cfg(test)]
pub mod test {
    use super::{v1::*, *};
    use crate::mock::*;
    use common::{gas_provider::GasNode, storage::*, Origin as _};
    use frame_support::{
        assert_ok,
        codec::Encode,
        storage::{storage_prefix, unhashed},
        traits::{OnRuntimeUpgrade, PalletInfoAccess},
        Identity, StorageHasher,
    };
    use gear_core::{
        ids::{MessageId, ProgramId, ReservationId},
        message::{DispatchKind, StoredDispatch, StoredMessage},
    };
    use sp_core::H256;
    use sp_runtime::traits::Zero;

    pub(crate) type MailboxOf<T> = <pallet_gear_messenger::Pallet<T> as Messenger>::Mailbox;
    pub(crate) type WaitlistOf<T> = <pallet_gear_messenger::Pallet<T> as Messenger>::Waitlist;
    pub(crate) type DispatchStashOf<T> =
        <pallet_gear_messenger::Pallet<T> as Messenger>::DispatchStash;

    fn gas_nodes_storage_map_final_key(key: &NodeId) -> Vec<u8> {
        let storage_prefix = storage_prefix(<Pallet<Test>>::name().as_bytes(), b"GasNodes");
        let key_hashed = key.using_encoded(Identity::hash);

        let mut final_key = Vec::with_capacity(storage_prefix.len() + key_hashed.len());

        final_key.extend_from_slice(&storage_prefix);
        final_key.extend_from_slice(key_hashed.as_ref());

        final_key
    }

    #[test]
    fn migration_to_v1_works() {
        let _ = env_logger::try_init();
        new_test_ext().execute_with(|| {
            let mut mailbox_ids = vec![];
            for _i in 0_u32..25 {
                mailbox_ids.push(MessageId::from_origin(H256::random()));
            }
            let mut waitlist_ids = vec![];
            for _i in 0_u32..25 {
                waitlist_ids.push(MessageId::from_origin(H256::random()));
            }
            let mut dispatch_stash_ids = vec![];
            for _i in 0_u32..25 {
                dispatch_stash_ids.push(MessageId::from_origin(H256::random()));
            }
            let mut reservation_ids = vec![];
            for _i in 0_u32..25 {
                reservation_ids.push(ReservationId::from(H256::random().as_ref()));
            }

            // Populate mailbox
            mailbox_ids.iter().for_each(|msg_id| {
                assert_ok!(MailboxOf::<Test>::insert(
                    StoredMessage::new(
                        *msg_id,
                        ProgramId::from_origin(H256::random()),
                        ProgramId::from_origin(2_u64.into_origin()), // to Bob
                        Default::default(),
                        Default::default(),
                        None,
                    ),
                    5_u64
                ));
            });
            // Populate waitlist
            waitlist_ids.iter().for_each(|msg_id| {
                assert_ok!(WaitlistOf::<Test>::insert(
                    StoredDispatch::new(
                        DispatchKind::Handle,
                        StoredMessage::new(
                            *msg_id,
                            ProgramId::from_origin(H256::random()),
                            ProgramId::from_origin(H256::random()),
                            Default::default(),
                            Default::default(),
                            None,
                        ),
                        None
                    ),
                    5_u64
                ));
            });
            // Populate delayed messages stash
            dispatch_stash_ids.iter().for_each(|msg_id| {
                DispatchStashOf::<Test>::insert(
                    *msg_id,
                    (
                        StoredDispatch::new(
                            DispatchKind::Handle,
                            StoredMessage::new(
                                *msg_id,
                                ProgramId::from_origin(H256::random()),
                                ProgramId::from_origin(H256::random()),
                                Default::default(),
                                Default::default(),
                                None,
                            ),
                            None,
                        ),
                        Interval {
                            start: 1_u64,
                            finish: 10_u64,
                        },
                    ),
                );
            });

            // Populate gas nodes storage with old gas node type
            let mut total_locked = Balance::zero();
            mailbox_ids.iter().for_each(|msg_id| {
                let node_id = NodeId::Node(*msg_id);
                let key = gas_nodes_storage_map_final_key(&node_id);
                // Mailboxed gas nodes have type `Cut`
                unhashed::put(
                    &key[..],
                    &OldGasNode::<AccountIdOf<Test>, NodeId, Balance>::Cut {
                        id: 1_u64,
                        value: Balance::zero(),
                        lock: 1000,
                    },
                );
                total_locked = total_locked.saturating_add(1000);
            });
            reservation_ids.iter().for_each(|reservation_id| {
                let node_id = NodeId::Reservation(*reservation_id);
                let key = gas_nodes_storage_map_final_key(&node_id);
                // Mailboxed gas nodes have type `Cut`
                unhashed::put(
                    &key[..],
                    &OldGasNode::<AccountIdOf<Test>, NodeId, Balance>::Reserved {
                        id: 1_u64,
                        value: Balance::zero(),
                        lock: 2000,
                        refs: Default::default(),
                        consumed: false,
                    },
                );
                total_locked = total_locked.saturating_add(2000);
            });
            waitlist_ids
                .iter()
                .chain(dispatch_stash_ids.iter())
                .for_each(|msg_id| {
                    let node_id = NodeId::Node(*msg_id);
                    let key = gas_nodes_storage_map_final_key(&node_id);
                    let mut factor_bytes = [0_u8; 8];
                    factor_bytes.copy_from_slice(&msg_id.as_ref()[0..8]);
                    let random_factor = u64::from_le_bytes(factor_bytes);
                    // Decide the node type
                    let node = match random_factor % 3 {
                        0 => OldGasNode::<AccountIdOf<Test>, NodeId, Balance>::External {
                            id: 1_u64,
                            value: Balance::zero(),
                            lock: 3000,
                            system_reserve: Default::default(),
                            refs: Default::default(),
                            consumed: false,
                        },
                        1 => OldGasNode::<AccountIdOf<Test>, NodeId, Balance>::SpecifiedLocal {
                            parent: NodeId::Node(MessageId::from_origin(H256::random())),
                            value: Balance::zero(),
                            lock: 3000,
                            system_reserve: Default::default(),
                            refs: Default::default(),
                            consumed: false,
                        },
                        _ => OldGasNode::<AccountIdOf<Test>, NodeId, Balance>::UnspecifiedLocal {
                            parent: NodeId::Node(MessageId::from_origin(H256::random())),
                            lock: 3000,
                            system_reserve: Default::default(),
                        },
                    };
                    unhashed::put(&key[..], &node);
                    total_locked = total_locked.saturating_add(3000);
                });

            // run migration from v0 to v1.
            let weight = MigrateToV1::<Test>::on_runtime_upgrade();
            assert_ne!(weight.ref_time(), 0);

            let new_total_locked =
                GasNodes::<Test>::iter().fold(Balance::zero(), |acc, (_k, v)| {
                    let locked = match v {
                        GasNode::External { lock, .. }
                        | GasNode::Cut { lock, .. }
                        | GasNode::Reserved { lock, .. }
                        | GasNode::SpecifiedLocal { lock, .. }
                        | GasNode::UnspecifiedLocal { lock, .. } => lock.total_locked(),
                    };
                    acc.saturating_add(locked)
                });
            assert_eq!(total_locked, new_total_locked);
        });
    }
}
