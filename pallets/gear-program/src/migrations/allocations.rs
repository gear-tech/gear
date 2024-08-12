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

use crate::{AllocationsStorage, Config, Pallet, ProgramStorage};
use frame_support::{
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade, StorageVersion},
    weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::program::{ActiveProgram, Program};
use sp_runtime::SaturatedConversion;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use {
    frame_support::ensure,
    sp_runtime::{
        codec::{Decode, Encode},
        TryRuntimeError,
    },
    sp_std::vec::Vec,
};

const MIGRATE_FROM_VERSION: u16 = 9;
const MIGRATE_TO_VERSION: u16 = 10;
const ALLOWED_CURRENT_STORAGE_VERSION: u16 = 10;

pub struct MigrateAllocations<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateAllocations<T> {
    fn on_runtime_upgrade() -> Weight {
        let onchain = Pallet::<T>::on_chain_storage_version();

        // 1 read for on chain storage version
        let mut weight = T::DbWeight::get().reads(1);

        if onchain == MIGRATE_FROM_VERSION {
            let current = Pallet::<T>::current_storage_version();
            if current != ALLOWED_CURRENT_STORAGE_VERSION {
                log::error!("❌ Migration is not allowed for current storage version {current:?}.");
                return weight;
            }

            let update_to = StorageVersion::new(MIGRATE_TO_VERSION);
            log::info!("🚚 Running migration from {onchain:?} to {update_to:?}, current storage version is {current:?}.");

            ProgramStorage::<T>::translate(|id, program: v9::Program<BlockNumberFor<T>>| {
                weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

                Some(match program {
                    v9::Program::Active(p) => {
                        let allocations_tree_len =
                            p.allocations.intervals_amount().saturated_into();
                        AllocationsStorage::<T>::insert(id, p.allocations);

                        // NOTE: p.pages_with_data is removed from the program

                        Program::Active(ActiveProgram {
                            allocations_tree_len,
                            memory_infix: p.memory_infix,
                            gas_reservation_map: p.gas_reservation_map,
                            code_hash: p.code_hash,
                            code_exports: p.code_exports,
                            static_pages: p.static_pages.into(),
                            state: p.state,
                            expiration_block: p.expiration_block,
                        })
                    }
                    v9::Program::Exited(id) => Program::Exited(id),
                    v9::Program::Terminated(id) => Program::Terminated(id),
                })
            });

            update_to.put::<Pallet<T>>();

            log::info!("✅ Successfully migrates storage");
        } else {
            log::info!("🟠 Migration requires onchain version {MIGRATE_FROM_VERSION}, so was skipped for {onchain:?}");
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        let res = if onchain == MIGRATE_FROM_VERSION {
            ensure!(
                current == ALLOWED_CURRENT_STORAGE_VERSION,
                "Current storage version is not allowed for migration, check migration code in order to allow it."
            );

            Some(v9::ProgramStorage::<T>::iter().count() as u64)
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
            let count = ProgramStorage::<T>::iter().count() as u64;
            ensure!(
                old_count == count,
                "incorrect count of programs after migration: old {} != new {}",
            );
            ensure!(
                Pallet::<T>::on_chain_storage_version() == MIGRATE_TO_VERSION,
                "incorrect storage version after migration"
            );
        }

        Ok(())
    }
}

mod v9 {
    use gear_core::{
        ids::ProgramId,
        message::DispatchKind,
        pages::{numerated::tree::IntervalsTree, GearPage, WasmPage},
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
        pub allocations: IntervalsTree<WasmPage>,
        pub pages_with_data: IntervalsTree<GearPage>,
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
        Exited(ProgramId),
        Terminated(ProgramId),
    }

    #[cfg(feature = "try-runtime")]
    use {
        crate::{Config, Pallet},
        frame_support::{
            storage::types::StorageMap,
            traits::{PalletInfo, StorageInstance},
            Identity,
        },
        sp_std::marker::PhantomData,
    };

    #[cfg(feature = "try-runtime")]
    pub struct ProgramStoragePrefix<T>(PhantomData<T>);

    #[cfg(feature = "try-runtime")]
    impl<T: Config> StorageInstance for ProgramStoragePrefix<T> {
        const STORAGE_PREFIX: &'static str = "ProgramStorage";

        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>()
                .expect("No name found for the pallet in the runtime!")
        }
    }

    #[cfg(feature = "try-runtime")]
    pub type ProgramStorage<T> = StorageMap<
        ProgramStoragePrefix<T>,
        Identity,
        ProgramId,
        Program<frame_system::pallet_prelude::BlockNumberFor<T>>,
    >;
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod test {
    use super::*;
    use crate::mock::*;
    use common::GearPage;
    use frame_support::traits::StorageVersion;
    use frame_system::pallet_prelude::BlockNumberFor;
    use gear_core::{ids::ProgramId, pages::WasmPage, program::ProgramState};
    use sp_runtime::traits::Zero;

    #[test]
    fn migration_works() {
        let _ = env_logger::try_init();

        new_test_ext().execute_with(|| {
            StorageVersion::new(MIGRATE_FROM_VERSION).put::<GearProgram>();

            // add active program
            let active_program_id = ProgramId::from(1u64);
            let program = v9::Program::<BlockNumberFor<Test>>::Active(v9::ActiveProgram {
                allocations: [1u16, 2, 3, 4, 5, 101, 102]
                    .into_iter()
                    .map(WasmPage::from)
                    .collect(),
                pages_with_data: [4u16, 5, 6, 7, 8, 400, 401]
                    .into_iter()
                    .map(GearPage::from)
                    .collect(),
                gas_reservation_map: Default::default(),
                code_hash: Default::default(),
                code_exports: Default::default(),
                static_pages: 1.into(),
                state: ProgramState::Initialized,
                expiration_block: 100,
                memory_infix: Default::default(),
            });
            v9::ProgramStorage::<Test>::insert(active_program_id, program);

            // add exited program
            let program = v9::Program::<BlockNumberFor<Test>>::Exited(active_program_id);
            let program_id = ProgramId::from(2u64);
            v9::ProgramStorage::<Test>::insert(program_id, program);

            // add terminated program
            let program = v9::Program::<BlockNumberFor<Test>>::Terminated(program_id);
            let program_id = ProgramId::from(3u64);
            v9::ProgramStorage::<Test>::insert(program_id, program);

            let state = MigrateAllocations::<Test>::pre_upgrade().unwrap();
            let w = MigrateAllocations::<Test>::on_runtime_upgrade();
            assert!(!w.is_zero());
            MigrateAllocations::<Test>::post_upgrade(state).unwrap();

            let allocations = AllocationsStorage::<Test>::get(active_program_id).unwrap();
            assert_eq!(
                allocations.to_vec(),
                [
                    WasmPage::from(1)..=WasmPage::from(5),
                    WasmPage::from(101)..=WasmPage::from(102)
                ]
            );

            let Some(Program::Active(program)) = ProgramStorage::<Test>::get(active_program_id)
            else {
                panic!("Program must be active");
            };

            assert_eq!(program.allocations_tree_len, 2);

            assert_eq!(
                ProgramStorage::<Test>::get(ProgramId::from(2u64)).unwrap(),
                Program::Exited(active_program_id)
            );
            assert_eq!(
                ProgramStorage::<Test>::get(ProgramId::from(3u64)).unwrap(),
                Program::Terminated(ProgramId::from(2u64))
            );

            assert_eq!(StorageVersion::get::<GearProgram>(), MIGRATE_TO_VERSION);
        })
    }
}
