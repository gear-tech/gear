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

use crate::{AccountIdOf, Balance, Config, GasNodes, Pallet};
use common::gas_provider::{ChildrenRefs, GasNode, GasNodeId, NodeLock};
use frame_support::{
    pallet_prelude::*,
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
};
use gear_core::ids::{MessageId, ReservationId};
use sp_runtime::traits::Saturating;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub(crate) type NodeId = GasNodeId<MessageId, ReservationId>;

#[derive(Decode, Encode)]
pub enum OldGasNode<ExternalId, Id, Balance> {
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

pub struct MigrateToV2<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> OnRuntimeUpgrade for MigrateToV2<T> {
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

        if current == 2 && onchain == 1 {
            let mut total = 0_u64;
            let mut translated = 0_u64;
            GasNodes::<T>::translate::<OldGasNode<AccountIdOf<T>, NodeId, Balance>, _>(
                |_key, old_value| {
                    total.saturating_inc();

                    if let OldGasNode::External {
                        id,
                        value,
                        lock,
                        system_reserve,
                        refs,
                        consumed,
                    } = old_value
                    {
                        translated.saturating_inc();
                        Some(GasNode::External {
                            id,
                            value,
                            lock,
                            system_reserve,
                            refs,
                            consumed,
                            provision: false,
                        })
                    } else {
                        None
                    }
                },
            );
            current.put::<Pallet<T>>();
            log::info!(
                "Upgraded {} gas nodes, storage to version {:?}",
                translated,
                current
            );
            T::DbWeight::get().reads_writes(total + 1, translated + 1)
        } else {
            log::info!("❌ Migration did not execute. This probably should be removed");
            T::DbWeight::get().reads(1)
        }
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
        let old_version: StorageVersion =
            Decode::decode(&mut state.as_ref()).map_err(|_| "Cannot decode version")?;
        let onchain_version = Pallet::<T>::on_chain_storage_version();
        assert_ne!(
            onchain_version, old_version,
            "must have upgraded from version 1 to 2."
        );

        log::info!("Storage successfully migrated.");
        Ok(())
    }
}

#[cfg(feature = "try-runtime")]
#[cfg(test)]
pub mod test_v2 {
    use super::*;
    use crate::{mock::*, AccountIdOf, Balance, GasNodes, Pallet};
    use common::{gas_provider::GasNode, Origin as _};
    use frame_support::{
        codec::Encode,
        storage::{storage_prefix, unhashed},
        traits::{OnRuntimeUpgrade, PalletInfoAccess},
        Identity, StorageHasher,
    };
    use gear_core::ids::MessageId;
    use sp_core::H256;
    use sp_runtime::traits::Zero;

    fn gas_nodes_storage_map_final_key(key: &NodeId) -> Vec<u8> {
        let storage_prefix = storage_prefix(<Pallet<Test>>::name().as_bytes(), b"GasNodes");
        let key_hashed = key.using_encoded(Identity::hash);

        let mut final_key = Vec::with_capacity(storage_prefix.len() + key_hashed.len());

        final_key.extend_from_slice(&storage_prefix);
        final_key.extend_from_slice(key_hashed.as_ref());

        final_key
    }

    #[test]
    fn migration_to_v2_works() {
        let _ = env_logger::try_init();
        new_test_ext().execute_with(|| {
            StorageVersion::new(1).put::<crate::Pallet<Test>>();

            let message_ids = (0..25)
                .map(|_| MessageId::from_origin(H256::random()))
                .collect::<Vec<_>>();

            // Populate gas nodes with old gas node type
            message_ids.into_iter().for_each(|msg_id| {
                let node_id = NodeId::Node(msg_id);
                let key = gas_nodes_storage_map_final_key(&node_id);
                let mut factor_bytes = [0_u8; 8];
                factor_bytes.copy_from_slice(&msg_id.as_ref()[0..8]);
                let random_factor = u64::from_le_bytes(factor_bytes);
                // Decide the node type
                let node = match random_factor % 3 {
                    0 => OldGasNode::<AccountIdOf<Test>, NodeId, Balance>::External {
                        id: 1_u64,
                        value: Balance::zero(),
                        lock: Zero::zero(),
                        system_reserve: Default::default(),
                        refs: Default::default(),
                        consumed: false,
                    },
                    1 => OldGasNode::<AccountIdOf<Test>, NodeId, Balance>::SpecifiedLocal {
                        parent: NodeId::Node(MessageId::from_origin(H256::random())),
                        value: Balance::zero(),
                        lock: Zero::zero(),
                        system_reserve: Default::default(),
                        refs: Default::default(),
                        consumed: false,
                    },
                    _ => OldGasNode::<AccountIdOf<Test>, NodeId, Balance>::UnspecifiedLocal {
                        parent: NodeId::Node(MessageId::from_origin(H256::random())),
                        lock: Zero::zero(),
                        system_reserve: Default::default(),
                    },
                };

                unhashed::put(&key[..], &node);
            });

            // run migration from v1 to v2.
            let weight = MigrateToV2::<Test>::on_runtime_upgrade();
            assert_ne!(weight.ref_time(), 0);

            for (_key, node) in GasNodes::<Test>::iter() {
                if let GasNode::External { provision, .. } = node {
                    assert!(!provision, "Incorrect migration");
                }
            }
        });
    }
}
