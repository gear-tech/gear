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

use super::v2::GasNodeV2 as OldGasNode;
use crate::{AccountIdOf, Balance, Config, Pallet};
use common::{
    gas_provider::{GasNode, GasNodeId},
    storage::MapStorage,
};
use frame_support::{
    pallet_prelude::*,
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade, PalletInfo},
};
use gear_core::ids::{MessageId, ReservationId};
use sp_runtime::traits::Saturating;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

const BEFORE_MIGRATION_VERSION: StorageVersion = StorageVersion::new(2);
const AFTER_MIGRATION_VERSION: StorageVersion = StorageVersion::new(3);

type NodeId = GasNodeId<MessageId, ReservationId>;

mod old_storage {
    use super::*;

    pub struct GasNodesPrefix<T>(PhantomData<(T,)>);
    impl<T: Config> frame_support::traits::StorageInstance for GasNodesPrefix<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>().expect("No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.")
        }
        const STORAGE_PREFIX: &'static str = "GasNodes";
    }
    pub type NodeOf<T> = OldGasNode<AccountIdOf<T>, NodeId, Balance>;
    pub type GasNodes<T> = StorageMap<GasNodesPrefix<T>, Identity, NodeId, NodeOf<T>>;
}

mod new_storage {
    use super::*;

    pub struct GasNodesPrefix<T>(PhantomData<(T,)>);
    impl<T: Config> frame_support::traits::StorageInstance for GasNodesPrefix<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>().expect("No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.")
        }
        const STORAGE_PREFIX: &'static str = "GasNodes";
    }
    pub type NodeOf<T> = GasNode<AccountIdOf<T>, NodeId, Balance>;
    pub type GasNodes<T> = StorageMap<GasNodesPrefix<T>, Identity, NodeId, NodeOf<T>>;
}

use new_storage::{GasNodes as NewGasNodes, NodeOf as NewNode};
use old_storage::{GasNodes as OldGasNodes, NodeOf as OldNode};

common::wrap_storage_map!(
    storage: NewGasNodes,
    name: NewGasNodesWrap,
    key: NodeId,
    value: NewNode<T>
);

common::wrap_storage_map!(
    storage: OldGasNodes,
    name: OldGasNodesWrap,
    key: NodeId,
    value: OldNode<T>
);

fn find_root<T: Config>(node_id: NodeId) -> NodeId {
    let mut root = node_id;

    loop {
        let node = OldGasNodes::<T>::get(root)
            .expect("Old GasTree is corrupted: parent node does not exist");
        match node {
            OldGasNode::External { .. } | OldGasNode::Reserved { .. } | OldGasNode::Cut { .. } => {
                break;
            }
            OldGasNode::SpecifiedLocal { parent, .. }
            | OldGasNode::UnspecifiedLocal { parent, .. } => root = parent,
        }
    }

    root
}

fn convert_v2_to_v3<T: Config>(_node_id: NodeId, old_node: OldNode<T>) -> NewNode<T> {
    match old_node {
        OldGasNode::Cut { id, value, lock } => GasNode::Cut { id, value, lock },
        OldGasNode::External {
            id,
            value,
            lock,
            system_reserve,
            refs,
            consumed,
            deposit,
        } => GasNode::External {
            id,
            value,
            lock,
            system_reserve,
            refs,
            consumed,
            deposit,
        },
        OldGasNode::Reserved {
            id,
            value,
            lock,
            refs,
            consumed,
        } => GasNode::Reserved {
            id,
            value,
            lock,
            refs,
            consumed,
        },
        OldGasNode::SpecifiedLocal {
            parent,
            value,
            lock,
            system_reserve,
            refs,
            consumed,
        } => GasNode::SpecifiedLocal {
            root: find_root::<T>(parent),
            parent,
            value,
            lock,
            system_reserve,
            refs,
            consumed,
        },
        OldGasNode::UnspecifiedLocal {
            parent,
            lock,
            system_reserve,
        } => GasNode::UnspecifiedLocal {
            root: find_root::<T>(parent),
            parent,
            lock,
            system_reserve,
        },
    }
}

pub struct MigrateToV3<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateToV3<T> {
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

        if current == AFTER_MIGRATION_VERSION && onchain == BEFORE_MIGRATION_VERSION {
            let mut translated = 0_u64;
            NewGasNodes::<T>::translate::<OldGasNode<AccountIdOf<T>, NodeId, Balance>, _>(
                |node_id, old_node| {
                    translated.saturating_inc();
                    Some(convert_v2_to_v3::<T>(node_id, old_node))
                },
            );
            current.put::<Pallet<T>>();
            log::info!(
                "Upgraded {} gas nodes, storage to version {:?}",
                translated,
                current
            );
            T::DbWeight::get().reads_writes(translated + 1, translated + 1)
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
            "must have upgraded from version {:?} to {:?}.",
            BEFORE_MIGRATION_VERSION, AFTER_MIGRATION_VERSION,
        );

        log::info!("Storage successfully migrated.");
        Ok(())
    }
}

#[cfg(test)]
pub mod test_v3 {
    use super::*;
    use crate::{mock::*, Balance, Pallet};
    use common::Origin as _;
    use frame_support::{
        codec::Encode,
        storage::{storage_prefix, unhashed},
        traits::{OnRuntimeUpgrade, PalletInfoAccess},
        Identity, StorageHasher,
    };
    use gear_core::ids::MessageId;
    use sp_core::H256;
    use sp_runtime::traits::Zero;
    use sp_std::collections::btree_set::BTreeSet;

    fn storage_key_from_gas_node_id(node_id: &NodeId) -> Vec<u8> {
        let storage_prefix = storage_prefix(<Pallet<Test>>::name().as_bytes(), b"GasNodes");
        let key_hashed = node_id.using_encoded(Identity::hash);

        [storage_prefix.as_ref(), key_hashed.as_ref()].concat()
    }

    #[test]
    fn migration_works() {
        type OldNode = super::OldNode<Test>;

        let _ = env_logger::try_init();

        let default_external_node = || OldNode::External {
            id: Default::default(),
            value: Balance::zero(),
            lock: Zero::zero(),
            system_reserve: Default::default(),
            refs: Default::default(),
            consumed: false,
            deposit: false,
        };

        let can_be_root = |node: &OldNode| {
            matches!(
                node,
                OldGasNode::External { .. } | OldGasNode::Reserved { .. }
            )
        };

        new_test_ext().execute_with(|| {
            BEFORE_MIGRATION_VERSION.put::<crate::Pallet<Test>>();

            let nodes_amount = 100;
            let mut gas_node_ids: Vec<(NodeId, OldNode)> = vec![];
            let mut known_roots = BTreeSet::<NodeId>::new();
            for _ in 0..nodes_amount {
                let random_hash = H256::random();
                let random_number = random_hash.to_low_u64_be();

                let msg_id = MessageId::from_origin(random_hash);
                let node_id = NodeId::Node(msg_id);
                let key = storage_key_from_gas_node_id(&node_id);

                let parent = if !gas_node_ids.is_empty() {
                    let random_index = (random_number as usize) % gas_node_ids.len();
                    let (id, node) = gas_node_ids.get(random_index).unwrap();
                    match node {
                        OldNode::External { .. }
                        | OldNode::Reserved { .. }
                        | OldNode::SpecifiedLocal { .. } => Some((*id, can_be_root(node))),
                        _ => None,
                    }
                } else {
                    None
                };

                let node = match random_number % 5 {
                    0 => default_external_node(),
                    1 => OldNode::Reserved {
                        id: Default::default(),
                        value: Balance::zero(),
                        lock: Zero::zero(),
                        refs: Default::default(),
                        consumed: false,
                    },
                    2 => OldNode::Cut {
                        id: Default::default(),
                        value: Balance::zero(),
                        lock: Zero::zero(),
                    },
                    3 => {
                        if let Some((parent, is_root)) = parent {
                            if is_root {
                                known_roots.insert(parent);
                            }
                            OldNode::SpecifiedLocal {
                                parent,
                                value: Balance::zero(),
                                lock: Zero::zero(),
                                system_reserve: Default::default(),
                                refs: Default::default(),
                                consumed: false,
                            }
                        } else {
                            default_external_node()
                        }
                    }
                    _ => {
                        if let Some((parent, is_root)) = parent {
                            if is_root {
                                known_roots.insert(parent);
                            }
                            OldNode::UnspecifiedLocal {
                                parent,
                                lock: Zero::zero(),
                                system_reserve: Default::default(),
                            }
                        } else {
                            default_external_node()
                        }
                    }
                };

                log::info!("try to put node");
                unhashed::put(key.as_slice(), &node);
                gas_node_ids.push((node_id, node));
            }

            let weight = MigrateToV3::<Test>::on_runtime_upgrade();
            assert_ne!(weight.ref_time(), 0);

            let mut count = 0;
            for (_, node) in NewGasNodes::<Test>::iter() {
                count += 1;
                match node {
                    GasNode::SpecifiedLocal { root, parent, .. }
                    | GasNode::UnspecifiedLocal { root, parent, .. } => {
                        assert!(known_roots.contains(&root));
                        let found_root = find_root::<Test>(parent);
                        assert_eq!(root, found_root);
                    }
                    _ => {}
                }
            }

            assert_eq!(count, nodes_amount)
        });
    }
}
