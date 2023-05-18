// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
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

use crate::{Config, Pallet, ProgramStorage, TaskPoolOf};
use common::{scheduler::*, Program};
use frame_support::{
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
    weights::Weight,
};
use sp_runtime::Saturating;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use {
    frame_support::{
        codec::{Decode, Encode},
        traits::StorageVersion,
    },
    sp_std::vec::Vec,
};

// almost 2 month for networks with 1-second block production
pub const FREE_PERIOD: u32 = 5_000_000;
static_assertions::const_assert!(FREE_PERIOD > 0);

pub struct MigrateToV2<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateToV2<T> {
    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        assert_eq!(
            StorageVersion::get::<Pallet<T>>(),
            1,
            "Can only upgrade from version 1"
        );

        let count = ProgramStorage::<T>::iter_keys().count() as u64;

        Ok(count.encode())
    }

    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        log::info!(
            "🚚 Running migration with current storage version {:?} / onchain {:?}",
            current,
            onchain
        );

        // 1 read for on chain storage version
        let mut weight = T::DbWeight::get().reads(1);

        if current == 2 && onchain == 1 {
            ProgramStorage::<T>::translate(
                |program_id, (program, _bn): (v1::Program, <T as frame_system::Config>::BlockNumber)| {
                    weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

                    let block_number = T::CurrentBlockNumber::get();
                    let expiration_block = block_number.saturating_add(FREE_PERIOD.into());
                    let task = ScheduledTask::PauseProgram(program_id);
                    TaskPoolOf::<T>::add(expiration_block, task)
                        .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));

                    Some(match program {
                        v1::Program::Active(p) => Program::Active(common::ActiveProgram {
                            allocations: p.allocations,
                            pages_with_data: p.pages_with_data,
                            gas_reservation_map: p.gas_reservation_map,
                            code_hash: p.code_hash,
                            code_exports: p.code_exports,
                            static_pages: p.static_pages,
                            state: p.state,
                            expiration_block,
                        }),
                        v1::Program::Exited(id) => Program::Exited(id),
                        v1::Program::Terminated(id) => Program::Terminated(id),
                    })
                },
            );

            current.put::<Pallet<T>>();

            log::info!("Successfully migrated storage from v1 to v2");
        } else {
            log::info!("❌ Migration did not execute. This probably should be removed");
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
        assert_eq!(StorageVersion::get::<Pallet<T>>(), 2, "Must upgrade");

        // Check that everything decoded fine.
        let count = ProgramStorage::<T>::iter_keys().fold(0u64, |i, k| {
            let Ok(program) = ProgramStorage::<T>::try_get(k) else {
                unreachable!("Cannot decode v2 Program");
            };

            if let Program::Active(p) = program {
                assert!(p.expiration_block >= FREE_PERIOD.into());
            }

            i + 1
        });

        let old_count: u64 =
            Decode::decode(&mut &state[..]).expect("pre_upgrade provides a valid state; qed");
        assert_eq!(count, old_count);

        Ok(())
    }
}

mod v1 {
    use common::ProgramState;
    use frame_support::{
        codec::{self, Decode, Encode},
        scale_info::{self, TypeInfo},
    };
    use gear_core::{
        ids::ProgramId,
        memory::{GearPage, WasmPage},
        message::DispatchKind,
        reservation::GasReservationMap,
    };
    use primitive_types::H256;
    use sp_std::{collections::btree_set::BTreeSet, prelude::*};

    #[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
    #[codec(crate = codec)]
    #[scale_info(crate = scale_info)]
    pub struct ActiveProgram {
        pub allocations: BTreeSet<WasmPage>,
        pub pages_with_data: BTreeSet<GearPage>,
        pub gas_reservation_map: GasReservationMap,
        pub code_hash: H256,
        pub code_exports: BTreeSet<DispatchKind>,
        pub static_pages: WasmPage,
        pub state: ProgramState,
    }

    #[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
    #[codec(crate = codec)]
    #[scale_info(crate = scale_info)]
    pub enum Program {
        Active(ActiveProgram),
        Exited(ProgramId),
        Terminated(ProgramId),
    }
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod test {
    use super::*;
    use crate::mock::*;
    use common::ProgramState;
    use frame_support::{migration, Hashable, StoragePrefixedMap};
    use frame_system::pallet_prelude::BlockNumberFor;
    use gear_core::ids::ProgramId;

    #[test]
    fn migration_to_v2_works() {
        new_test_ext().execute_with(|| {
            StorageVersion::new(1).put::<GearProgram>();

            let module = ProgramStorage::<Test>::module_prefix();
            let item = ProgramStorage::<Test>::storage_prefix();

            // add active program
            let program_id = ProgramId::from(1u64);
            let block_number: BlockNumberFor<Test> = 0u32.into();
            let program = v1::Program::Active(v1::ActiveProgram {
                allocations: Default::default(),
                pages_with_data: Default::default(),
                gas_reservation_map: Default::default(),
                code_hash: Default::default(),
                code_exports: Default::default(),
                static_pages: 13.into(),
                state: ProgramState::Initialized,
            });
            migration::put_storage_value(
                module,
                item,
                &program_id.identity(),
                (program, block_number),
            );

            // add exited program
            let program = v1::Program::Exited(program_id);
            let program_id = ProgramId::from(2u64);
            migration::put_storage_value(
                module,
                item,
                &program_id.identity(),
                (program, block_number),
            );

            // add terminated program
            let program = v1::Program::Terminated(program_id);
            let program_id = ProgramId::from(3u64);
            migration::put_storage_value(
                module,
                item,
                &program_id.identity(),
                (program, block_number),
            );

            let state = MigrateToV2::<Test>::pre_upgrade().unwrap();
            let _w = MigrateToV2::<Test>::on_runtime_upgrade();
            MigrateToV2::<Test>::post_upgrade(state).unwrap();

            assert_eq!(StorageVersion::get::<GearProgram>(), 2);
        })
    }
}
