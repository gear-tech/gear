// This file is part of Gear.

// Copyright (C) 2023-2024 Gear Technologies Inc.
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

use crate::{Config, Pallet, ProgramStorage};
use frame_support::{
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade, StorageVersion},
    weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::program::{ActiveProgram, Program};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use {
    frame_support::ensure,
    sp_runtime::{
        TryRuntimeError,
        codec::{Decode, Encode},
    },
    sp_std::vec::Vec,
};

const MIGRATE_FROM_VERSION: u16 = 11;
const MIGRATE_TO_VERSION: u16 = 12;
const ALLOWED_CURRENT_STORAGE_VERSION: u16 = 13;

pub struct MigrateProgramCodeHashToCodeId<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateProgramCodeHashToCodeId<T> {
    fn on_runtime_upgrade() -> Weight {
        // 1 read for onchain storage version
        let mut weight = T::DbWeight::get().reads(1);
        let mut counter = 0;

        let onchain = Pallet::<T>::on_chain_storage_version();

        if onchain == MIGRATE_FROM_VERSION {
            let current = Pallet::<T>::in_code_storage_version();
            if current != ALLOWED_CURRENT_STORAGE_VERSION {
                log::error!("❌ Migration is not allowed for current storage version {current:?}.");
                return weight;
            }

            let update_to = StorageVersion::new(MIGRATE_TO_VERSION);
            log::info!(
                "🚚 Running migration from {onchain:?} to {update_to:?}, current storage version is {current:?}."
            );

            ProgramStorage::<T>::translate(|_, program: v11::Program<BlockNumberFor<T>>| {
                weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

                counter += 1;

                Some(match program {
                    v11::Program::Active(p) => {
                        // NOTE: p.code_exports and static_pages is removed from the program
                        Program::Active(ActiveProgram {
                            allocations_tree_len: p.allocations_tree_len,
                            memory_infix: p.memory_infix,
                            gas_reservation_map: p.gas_reservation_map,
                            code_id: p.code_hash.into(),
                            state: p.state,
                            expiration_block: p.expiration_block,
                        })
                    }
                    v11::Program::Exited(id) => Program::Exited(id),
                    v11::Program::Terminated(id) => Program::Terminated(id),
                })
            });

            // Put new storage version
            weight = weight.saturating_add(T::DbWeight::get().writes(1));

            update_to.put::<Pallet<T>>();

            log::info!("✅ Successfully migrated storage. {counter} codes have been migrated");
        } else {
            log::info!(
                "🟠 Migration requires onchain version {MIGRATE_FROM_VERSION}, so was skipped for {onchain:?}"
            );
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        let current = Pallet::<T>::in_code_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        let res = if onchain == MIGRATE_FROM_VERSION {
            ensure!(
                current == ALLOWED_CURRENT_STORAGE_VERSION,
                "Current storage version is not allowed for migration, check migration code in order to allow it."
            );

            Some(v11::ProgramStorage::<T>::iter().count() as u64)
        } else {
            None
        };

        Ok(res.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
        if let Some(old_count) = Option::<u64>::decode(&mut state.as_ref())
            .map_err(|_| "`pre_upgrade` provided an invalid state")?
        {
            let count = ProgramStorage::<T>::iter_keys().count() as u64;
            ensure!(old_count == count, "incorrect count of elements");
        }

        Ok(())
    }
}

mod v11 {
    use gear_core::{
        ids::ActorId,
        message::DispatchKind,
        pages::WasmPage,
        program::{MemoryInfix, ProgramState},
        reservation::GasReservationMap,
    };
    use primitive_types::H256;
    use sp_runtime::{
        codec::{self, Decode, Encode},
        scale_info::{self, TypeInfo},
        traits::Saturating,
    };
    use sp_std::{collections::btree_set::BTreeSet, prelude::*};

    #[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
    #[codec(crate = codec)]
    #[scale_info(crate = scale_info)]
    pub struct ActiveProgram<BlockNumber: Copy + Saturating> {
        pub allocations_tree_len: u32,
        pub memory_infix: MemoryInfix,
        pub gas_reservation_map: GasReservationMap,
        pub code_hash: H256,
        pub code_exports: BTreeSet<DispatchKind>,
        pub static_pages: WasmPage,
        pub state: ProgramState,
        pub expiration_block: BlockNumber,
    }

    #[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
    #[codec(crate = codec)]
    #[scale_info(crate = scale_info)]
    pub enum Program<BlockNumber: Copy + Saturating> {
        Active(ActiveProgram<BlockNumber>),
        Exited(ActorId),
        Terminated(ActorId),
    }

    #[cfg(feature = "try-runtime")]
    use {
        crate::{Config, Pallet},
        frame_support::Identity,
    };

    #[cfg(feature = "try-runtime")]
    #[frame_support::storage_alias]
    pub type ProgramStorage<T: Config> = StorageMap<
        Pallet<T>,
        Identity,
        ActorId,
        Program<frame_system::pallet_prelude::BlockNumberFor<T>>,
    >;
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod test {
    use super::*;
    use crate::mock::*;
    use frame_support::traits::StorageVersion;
    use gear_core::{
        ids::{ActorId, CodeId},
        program::ProgramState,
    };
    use primitive_types::H256;
    use sp_runtime::traits::Zero;

    #[test]
    fn v12_program_code_id_migration_works() {
        let _ = tracing_subscriber::fmt::try_init();

        new_test_ext().execute_with(|| {
            StorageVersion::new(MIGRATE_FROM_VERSION).put::<GearProgram>();

            // add active program
            let active_program_id = ActorId::from(1u64);
            let program = v11::Program::<BlockNumberFor<Test>>::Active(v11::ActiveProgram {
                allocations_tree_len: 2,
                gas_reservation_map: Default::default(),
                code_hash: H256::from([1; 32]),
                code_exports: Default::default(),
                static_pages: 1.into(),
                state: ProgramState::Initialized,
                expiration_block: 100,
                memory_infix: Default::default(),
            });
            v11::ProgramStorage::<Test>::insert(active_program_id, program.clone());

            // add exited program
            let program = v11::Program::<BlockNumberFor<Test>>::Exited(active_program_id);
            let program_id = ActorId::from(2u64);
            v11::ProgramStorage::<Test>::insert(program_id, program);

            // add terminated program
            let program = v11::Program::<BlockNumberFor<Test>>::Terminated(program_id);
            let program_id = ActorId::from(3u64);
            v11::ProgramStorage::<Test>::insert(program_id, program);

            let state = MigrateProgramCodeHashToCodeId::<Test>::pre_upgrade().unwrap();
            let w = MigrateProgramCodeHashToCodeId::<Test>::on_runtime_upgrade();
            assert!(!w.is_zero());
            MigrateProgramCodeHashToCodeId::<Test>::post_upgrade(state).unwrap();

            let Some(Program::Active(program)) = ProgramStorage::<Test>::get(active_program_id)
            else {
                panic!("Program must be active");
            };

            assert_eq!(program.allocations_tree_len, 2);
            assert_eq!(program.memory_infix, Default::default());
            assert_eq!(program.gas_reservation_map, Default::default());
            assert_eq!(program.code_id, CodeId::from(H256::from([1; 32])));
            assert_eq!(program.state, ProgramState::Initialized);
            assert_eq!(program.expiration_block, 100);

            assert_eq!(
                ProgramStorage::<Test>::get(ActorId::from(2u64)).unwrap(),
                Program::Exited(active_program_id)
            );
            assert_eq!(
                ProgramStorage::<Test>::get(ActorId::from(3u64)).unwrap(),
                Program::Terminated(ActorId::from(2u64))
            );

            assert_eq!(StorageVersion::get::<GearProgram>(), MIGRATE_TO_VERSION);
        })
    }
}
