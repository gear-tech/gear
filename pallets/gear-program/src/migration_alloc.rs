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
use common::Program;
use frame_support::{
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
    weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use {
    sp_runtime::{
        codec::{Decode, Encode},
        TryRuntimeError,
    },
    sp_std::vec::Vec,
};

pub struct MigrateToV4<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateToV4<T> {
    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        Ok((v2::ProgramStorage::<T>::iter().count() as u64).encode())
    }

    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        log::info!(
            "🚚 Running migration with current storage version {current:?} / onchain {onchain:?}"
        );

        // 1 read for on chain storage version
        let mut weight = T::DbWeight::get().reads(1);

        if current == 4 && onchain == 3 {
            ProgramStorage::<T>::translate(|_, program: v2::Program<BlockNumberFor<T>>| {
                weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

                Some(match program {
                    v2::Program::Active(p) => Program::Active(common::ActiveProgram {
                        allocations: p.allocations.into_iter().collect(),
                        pages_with_data: p.pages_with_data.into_iter().collect(),
                        gas_reservation_map: p.gas_reservation_map,
                        code_hash: p.code_hash,
                        code_exports: p.code_exports,
                        static_pages: p.static_pages.into(),
                        state: p.state,
                        expiration_block: p.expiration_block,
                        memory_infix: p.memory_infix,
                    }),
                    v2::Program::Exited(id) => Program::Exited(id),
                    v2::Program::Terminated(id) => Program::Terminated(id),
                })
            });

            current.put::<Pallet<T>>();

            log::info!("Successfully migrated storage");
        } else {
            log::info!("❌ Migration did not execute. This probably should be removed");
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
        // Check that everything decoded fine.
        let count = ProgramStorage::<T>::iter_keys().fold(0u64, |i, _| i + 1);
        let old_count: u64 =
            Decode::decode(&mut &state[..]).expect("pre_upgrade provides a valid state; qed");
        assert_eq!(count, old_count);

        Ok(())
    }
}

mod v2 {
    use common::ProgramState;
    use gear_core::{
        ids::ProgramId,
        message::DispatchKind,
        pages::{GearPage, WasmPage},
        program::MemoryInfix,
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
        pub allocations: BTreeSet<WasmPage>,
        pub pages_with_data: BTreeSet<GearPage>,
        pub gas_reservation_map: GasReservationMap,
        pub code_hash: H256,
        pub code_exports: BTreeSet<DispatchKind>,
        pub static_pages: WasmPage,
        pub state: ProgramState,
        pub expiration_block: BlockNumber,
        pub memory_infix: MemoryInfix,
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
    use common::ProgramState;
    use frame_support::traits::StorageVersion;
    use frame_system::pallet_prelude::BlockNumberFor;
    use gear_core::{
        ids::ProgramId,
        pages::{GearPage, WasmPage},
    };
    use sp_runtime::traits::Zero;

    #[test]
    fn migration_to_v3_works() {
        new_test_ext().execute_with(|| {
            StorageVersion::new(2).put::<GearProgram>();

            // add active program
            let active_program_id = ProgramId::from(1u64);
            let program = v2::Program::<BlockNumberFor<Test>>::Active(v2::ActiveProgram {
                allocations: [1u16, 2, 3, 4, 5, 101, 102]
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                pages_with_data: [4u16, 5, 6, 7, 8, 400, 401]
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                gas_reservation_map: Default::default(),
                code_hash: Default::default(),
                code_exports: Default::default(),
                static_pages: 1.into(),
                state: ProgramState::Initialized,
                expiration_block: 100,
                memory_infix: Default::default(),
            });
            v2::ProgramStorage::<Test>::insert(active_program_id, program);

            // add exited program
            let program = v2::Program::<BlockNumberFor<Test>>::Exited(active_program_id);
            let program_id = ProgramId::from(2u64);
            v2::ProgramStorage::<Test>::insert(program_id, program);

            // add terminated program
            let program = v2::Program::<BlockNumberFor<Test>>::Terminated(program_id);
            let program_id = ProgramId::from(3u64);
            v2::ProgramStorage::<Test>::insert(program_id, program);

            let state = MigrateToV4::<Test>::pre_upgrade().unwrap();
            let w = MigrateToV4::<Test>::on_runtime_upgrade();
            assert!(!w.is_zero());
            MigrateToV4::<Test>::post_upgrade(state).unwrap();

            if let Program::Active(p) = ProgramStorage::<Test>::get(active_program_id).unwrap() {
                assert_eq!(
                    p.allocations.to_vec(),
                    [
                        WasmPage::from(1)..=WasmPage::from(5),
                        WasmPage::from(101)..=WasmPage::from(102)
                    ]
                );
                assert_eq!(
                    p.pages_with_data.to_vec(),
                    [
                        GearPage::from(4)..=GearPage::from(8),
                        GearPage::from(400)..=GearPage::from(401)
                    ]
                );
            } else {
                panic!("Program must be active");
            }

            assert_eq!(
                ProgramStorage::<Test>::get(ProgramId::from(2u64)).unwrap(),
                Program::Exited(active_program_id)
            );
            assert_eq!(
                ProgramStorage::<Test>::get(ProgramId::from(3u64)).unwrap(),
                Program::Terminated(ProgramId::from(2u64))
            );

            assert_eq!(StorageVersion::get::<GearProgram>(), 4);
        })
    }
}
