use crate::{Config, Error, Key, NodeOf, Pallet, Weight};
use common::GasMultiplier;
use core::marker::PhantomData;
use frame_support::{
    dispatch::GetStorageVersion,
    traits::{Get, OnRuntimeUpgrade},
};
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

const MULTIPLIER: GasMultiplier<u128, u64> = GasMultiplier::ValuePerGas(1_000);

pub struct MigrateToV3<T>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateToV3<T> {
    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        use parity_scale_codec::Encode as _;

        let version = <Pallet<T>>::on_chain_storage_version();

        Ok(version.encode())
    }

    fn on_runtime_upgrade() -> Weight {
        let onchain = Pallet::<T>::on_chain_storage_version();
        let current = Pallet::<T>::current_storage_version();

        if current != 3 || onchain != 2 {
            log::info!("❌ Migrations of `pallet-gear-gas` to V3 are outdated");

            return T::DbWeight::get().reads(1);
        }

        log::info!("🚚 Running migrations to version {current:?} from version {onchain:?}");

        let mut writes = 0u64;

        crate::GasNodes::<T>::translate::<v2::GasNode<T>, _>(|key, value| {
            writes += 1;
            translate::<T>(key, value)
                .map_err(|e| {
                    log::error!("Error translating {key:?} node: {e:?})");
                    e
                })
                .ok()
        });

        log::info!("Upgraded {writes:?} gas nodes");

        current.put::<Pallet<T>>();

        T::DbWeight::get().reads_writes(1, writes + 1)
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
        use frame_support::traits::StorageVersion;
        use parity_scale_codec::Decode;

        let previous: StorageVersion =
            Decode::decode(&mut state.as_ref()).map_err(|_| "Cannot decode version")?;

        if previous == 2 {
            let onchain = Pallet::<T>::on_chain_storage_version();

            assert_ne!(previous, onchain, "Must have upgraded from version 2 to 3");

            log::info!("Storage `pallet-gear-gas` successfully migrated to V3");
        } else {
            log::info!("Storage `pallet-gear-gas` was already migrated to V3");
        }

        Ok(())
    }
}

fn translate<T: Config>(node_key: Key, node: v2::GasNode<T>) -> Result<NodeOf<T>, Error<T>> {
    log::info!("Translating {node_key:?} node");

    let new_node = match node {
        v2::GasNode::<T>::Cut { id, value, lock } => NodeOf::<T>::Cut {
            id,
            multiplier: MULTIPLIER,
            value,
            lock,
        },
        v2::GasNode::<T>::External {
            id,
            value,
            lock,
            system_reserve,
            refs,
            consumed,
            deposit,
        } => NodeOf::<T>::External {
            id,
            multiplier: MULTIPLIER,
            value,
            lock,
            system_reserve,
            refs,
            consumed,
            deposit,
        },
        v2::GasNode::<T>::Reserved {
            id,
            value,
            lock,
            refs,
            consumed,
        } => NodeOf::<T>::Reserved {
            id,
            multiplier: MULTIPLIER,
            value,
            lock,
            refs,
            consumed,
        },
        v2::GasNode::<T>::SpecifiedLocal {
            parent,
            value,
            lock,
            system_reserve,
            refs,
            consumed,
        } => NodeOf::<T>::SpecifiedLocal {
            parent,
            root: v2::root(node_key, node)?,
            value,
            lock,
            system_reserve,
            refs,
            consumed,
        },
        v2::GasNode::<T>::UnspecifiedLocal {
            parent,
            lock,
            system_reserve,
        } => NodeOf::<T>::UnspecifiedLocal {
            parent,
            root: v2::root(node_key, node)?,
            lock,
            system_reserve,
        },
    };

    Ok(new_node)
}

mod v2 {
    use crate::{AccountIdOf, Balance, Config, Error, Key, Pallet};
    use common::gas_provider::{ChildrenRefs, GasNodeId, NodeLock};
    use core::marker::PhantomData;
    use frame_support::{
        storage::types::StorageMap,
        traits::{PalletInfo, StorageInstance},
        Identity,
    };
    use gear_core::ids::{MessageId, ReservationId};
    use parity_scale_codec::{Decode, Encode};

    pub type GasNode<T> = GasNodeImpl<AccountIdOf<T>, GasNodeId<MessageId, ReservationId>, Balance>;

    pub struct GasNodesPrefix<T>(PhantomData<T>);

    impl<T: Config> StorageInstance for GasNodesPrefix<T> {
        const STORAGE_PREFIX: &'static str = "GasNodes";

        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>()
                .expect("No name found for the pallet in the runtime!")
        }
    }

    pub type GasNodes<T> = StorageMap<GasNodesPrefix<T>, Identity, Key, GasNode<T>>;

    #[derive(Encode, Decode, Debug)]
    pub enum GasNodeImpl<ExternalId, Id, Balance> {
        External {
            id: ExternalId,
            value: Balance,
            lock: NodeLock<Balance>,
            system_reserve: Balance,
            refs: ChildrenRefs,
            consumed: bool,
            deposit: bool,
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

    impl<ExternalId, Id: Copy, Balance> GasNodeImpl<ExternalId, Id, Balance> {
        pub fn parent(&self) -> Option<Id> {
            match self {
                Self::External { .. } | Self::Cut { .. } | Self::Reserved { .. } => None,
                Self::SpecifiedLocal { parent, .. } | Self::UnspecifiedLocal { parent, .. } => {
                    Some(*parent)
                }
            }
        }
    }

    pub fn root<T: Config>(mut node_key: Key, mut node: GasNode<T>) -> Result<Key, Error<T>> {
        log::trace!("Looking for root of {node_key:?} ({node:?}");

        while let Some(parent) = node.parent() {
            node_key = parent;
            node = GasNodes::<T>::get(node_key).ok_or(Error::<T>::ParentIsLost)?;
        }

        log::trace!("Root found: {node_key:?}");

        Ok(node_key)
    }
}
