// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

use crate::{
    Config, MemoryPageStorage2, Pallet, PausedProgramStorage, ProgramStorage, ResumeSessions,
};
use common::Program;
use frame_support::{
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
    weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::program::MemoryInfix;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use {
    frame_support::{
        codec::{Decode, Encode},
        traits::StorageVersion,
    },
    sp_std::vec::Vec,
};

const MEMORY_INFIX: MemoryInfix = 0;

pub struct MigrateToV3<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateToV3<T> {
    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        assert_eq!(
            StorageVersion::get::<Pallet<T>>(),
            2,
            "Can only upgrade from version 2"
        );

        let count = v2::ProgramStorage::<T>::iter().fold(0u64, |count, (program_id, program)| {
            match program {
                v2::Program::Terminated(_) | v2::Program::Exited(_) => {
                    assert!(v2::MemoryPageStorage::<T>::iter_key_prefix(program_id)
                        .next()
                        .is_none());
                }
                v2::Program::Active(_) => (),
            }

            count + 1
        });

        Ok(count.encode())
    }

    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        log::info!(
            "🚚 Running migration with current storage version {current:?} / onchain {onchain:?}"
        );

        // 1 read for on chain storage version
        let mut weight = T::DbWeight::get().reads(1);

        if current == 3 && onchain == 2 {
            ProgramStorage::<T>::translate(
                |program_id, program: v2::Program<BlockNumberFor<T>>| {
                    weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

                    Some(match program {
                        v2::Program::Active(p) => {
                            for (page, data) in v2::MemoryPageStorage::<T>::drain_prefix(program_id)
                            {
                                weight =
                                    weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

                                MemoryPageStorage2::<T>::insert(
                                    (program_id, MEMORY_INFIX, page),
                                    data,
                                );
                            }

                            Program::Active(common::ActiveProgram {
                                allocations: p.allocations,
                                pages_with_data: p.pages_with_data,
                                gas_reservation_map: p.gas_reservation_map,
                                code_hash: p.code_hash,
                                code_exports: p.code_exports,
                                static_pages: p.static_pages,
                                state: p.state,
                                expiration_block: p.expiration_block,
                                memory_infix: MEMORY_INFIX,
                            })
                        }
                        v2::Program::Exited(id) => Program::Exited(id),
                        v2::Program::Terminated(id) => Program::Terminated(id),
                    })
                },
            );

            let _ = ResumeSessions::<T>::clear(u32::MAX, None);
            let _ = v2::SessionMemoryPages::<T>::clear(u32::MAX, None);
            let _ = PausedProgramStorage::<T>::clear(u32::MAX, None);

            current.put::<Pallet<T>>();

            log::info!("Successfully migrated storage");
        } else {
            log::info!("❌ Migration did not execute. This probably should be removed");
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
        assert_eq!(StorageVersion::get::<Pallet<T>>(), 3, "Must upgrade");

        // Check that everything decoded fine.
        let count = ProgramStorage::<T>::iter_keys().fold(0u64, |i, k| {
            let Ok(program) = ProgramStorage::<T>::try_get(k) else {
                unreachable!("Cannot decode v3 Program");
            };

            if let Program::Active(p) = program {
                assert_eq!(p.memory_infix, MEMORY_INFIX);

                for page in p.pages_with_data.iter() {
                    assert!(MemoryPageStorage2::<T>::contains_key((
                        k,
                        p.memory_infix,
                        page
                    )));
                }
            }

            i + 1
        });

        let old_count: u64 =
            Decode::decode(&mut &state[..]).expect("pre_upgrade provides a valid state; qed");
        assert_eq!(count, old_count);

        assert!(v2::MemoryPageStorage::<T>::iter().next().is_none());
        assert!(v2::SessionMemoryPages::<T>::iter().next().is_none());

        assert!(ResumeSessions::<T>::iter().next().is_none());
        assert!(PausedProgramStorage::<T>::iter().next().is_none());

        Ok(())
    }
}

mod v2 {
    use crate::{Config, Pallet};
    use common::ProgramState;
    use frame_support::{
        codec::{self, Decode, Encode},
        scale_info::{self, TypeInfo},
        storage::types::{StorageDoubleMap, StorageMap},
        traits::{PalletInfo, StorageInstance},
        Identity,
    };
    use frame_system::pallet_prelude::BlockNumberFor;
    use gear_core::{
        ids::ProgramId,
        memory::PageBuf,
        message::DispatchKind,
        pages::{GearPage, WasmPage},
        reservation::GasReservationMap,
    };
    use primitive_types::H256;
    use sp_runtime::traits::Saturating;
    use sp_std::{collections::btree_set::BTreeSet, marker::PhantomData, prelude::*};

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
    }

    #[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
    #[codec(crate = codec)]
    #[scale_info(crate = scale_info)]
    pub enum Program<BlockNumber: Copy + Saturating> {
        Active(ActiveProgram<BlockNumber>),
        Exited(ProgramId),
        Terminated(ProgramId),
    }

    pub struct MemoryPagesPrefix<T>(PhantomData<T>);

    impl<T: Config> StorageInstance for MemoryPagesPrefix<T> {
        const STORAGE_PREFIX: &'static str = "MemoryPageStorage";

        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>()
                .expect("No name found for the pallet in the runtime!")
        }
    }

    pub type MemoryPageStorage<T> =
        StorageDoubleMap<MemoryPagesPrefix<T>, Identity, ProgramId, Identity, GearPage, PageBuf>;

    pub struct ProgramStoragePrefix<T>(PhantomData<T>);

    impl<T: Config> StorageInstance for ProgramStoragePrefix<T> {
        const STORAGE_PREFIX: &'static str = "ProgramStorage";

        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>()
                .expect("No name found for the pallet in the runtime!")
        }
    }

    pub type ProgramStorage<T> =
        StorageMap<ProgramStoragePrefix<T>, Identity, ProgramId, Program<BlockNumberFor<T>>>;

    pub struct SessionMemoryPagesPrefix<T>(PhantomData<T>);

    impl<T: Config> StorageInstance for SessionMemoryPagesPrefix<T> {
        const STORAGE_PREFIX: &'static str = "SessionMemoryPages";

        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>()
                .expect("No name found for the pallet in the runtime!")
        }
    }

    pub type SessionMemoryPages<T> =
        StorageMap<SessionMemoryPagesPrefix<T>, Identity, u128, Vec<(GearPage, PageBuf)>>;
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod test {
    use super::*;
    use crate::{mock::*, pallet, PausedProgramStorage};
    use common::{PausedProgramStorage as _, ProgramState};
    use frame_system::pallet_prelude::BlockNumberFor;
    use gear_core::{ids::ProgramId, memory::PageBuf, pages::GearPage};
    use primitive_types::H256;

    #[test]
    fn migration_to_v3_works() {
        new_test_ext().execute_with(|| {
            StorageVersion::new(2).put::<GearProgram>();

            // add active program
            let program_id = ProgramId::from(1u64);
            let page = GearPage::from(0);
            v2::MemoryPageStorage::<Test>::insert(program_id, page, {
                let mut page = PageBuf::new_zeroed();
                page[0] = 1;

                page
            });
            let program = v2::Program::<BlockNumberFor<Test>>::Active(v2::ActiveProgram {
                allocations: Default::default(),
                pages_with_data: [page].into(),
                gas_reservation_map: Default::default(),
                code_hash: Default::default(),
                code_exports: Default::default(),
                static_pages: 13.into(),
                state: ProgramState::Initialized,
                expiration_block: 100,
            });
            v2::ProgramStorage::<Test>::insert(program_id, program);

            // add exited program
            let program = v2::Program::<BlockNumberFor<Test>>::Exited(program_id);
            let program_id = ProgramId::from(2u64);
            v2::ProgramStorage::<Test>::insert(program_id, program);

            // add terminated program
            let program = v2::Program::<BlockNumberFor<Test>>::Terminated(program_id);
            let program_id = ProgramId::from(3u64);
            v2::ProgramStorage::<Test>::insert(program_id, program);

            // add a paused program
            let program_id = ProgramId::from(4u64);
            PausedProgramStorage::<Test>::insert(program_id, (0, H256::from([1u8; 32])));

            // add a resume session
            let session_id = pallet::Pallet::<Test>::resume_session_init(
                USER_2,
                30,
                program_id,
                Default::default(),
                Default::default(),
            )
            .unwrap();
            pallet::Pallet::<Test>::resume_session_push(
                session_id,
                USER_2,
                vec![
                    (GearPage::from(0), PageBuf::new_zeroed()),
                    (GearPage::from(2), PageBuf::new_zeroed()),
                ],
            )
            .unwrap();

            let state = MigrateToV3::<Test>::pre_upgrade().unwrap();
            let _w = MigrateToV3::<Test>::on_runtime_upgrade();
            MigrateToV3::<Test>::post_upgrade(state).unwrap();

            assert_eq!(StorageVersion::get::<GearProgram>(), 3);
        })
    }
}
