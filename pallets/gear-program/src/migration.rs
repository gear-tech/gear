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

use crate::{Config, Pallet, ProgramStorage, TaskPoolOf};
use common::{scheduler::*, Program};
use frame_support::{
    traits::{Get, StorageVersion},
    weights::Weight,
};
use sp_runtime::Saturating;

// almost 2 month for networks with 1-second block production
pub const FREE_PERIOD: u32 = 5_000_000;
static_assertions::const_assert!(FREE_PERIOD > 0);

const VERSION_1: StorageVersion = StorageVersion::new(1);

/// Wrapper for all migrations of this pallet, based on `StorageVersion`.
pub fn migrate<T: Config>() -> Weight {
    let version = StorageVersion::get::<Pallet<T>>();
    let weight: Weight = Weight::zero();

    if version == VERSION_1 {
        ProgramStorage::<T>::translate(
            |program_id, (program, _bn): (version_1::Program, <T as frame_system::Config>::BlockNumber)| {
                let block_number = T::CurrentBlockNumber::get();
                let expiration_block = block_number.saturating_add(FREE_PERIOD.into());
                let task = ScheduledTask::PauseProgram(program_id);
                TaskPoolOf::<T>::add(expiration_block, task)
                    .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));

                Some(match program {
                    version_1::Program::Active(p) => Program::Active(common::ActiveProgram {
                        allocations: p.allocations,
                        pages_with_data: p.pages_with_data,
                        gas_reservation_map: p.gas_reservation_map,
                        code_hash: p.code_hash,
                        code_exports: p.code_exports,
                        static_pages: p.static_pages,
                        state: p.state,
                        expiration_block,
                    }),
                    version_1::Program::Exited(id) => Program::Exited(id),
                    version_1::Program::Terminated(id) => Program::Terminated(id),
                })
            },
        );

        super::pallet::PROGRAM_STORAGE_VERSION.put::<Pallet<T>>();
    }

    weight
}

mod version_1 {
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
