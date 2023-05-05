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
    traits::{Get, OnRuntimeUpgrade, StorageVersion},
    weights::Weight,
};
use gear_core::ids::{CodeId, ProgramId};
use parity_scale_codec::{Decode, Encode};
use sp_runtime::Saturating;
use sp_std::marker::PhantomData;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

// almost 2 month for networks with 1-second block production
pub const FREE_PERIOD: u32 = 5_000_000;
static_assertions::const_assert!(FREE_PERIOD > 0);

const VERSION_1: StorageVersion = StorageVersion::new(1);

pub struct MigrateV1ToV2<T>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateV1ToV2<T> {
    fn on_runtime_upgrade() -> Weight {
        let version = StorageVersion::get::<Pallet<T>>();
        let mut weight: Weight = Weight::zero();

        if version == VERSION_1 {
            ProgramStorage::<T>::translate(
                |program_id, (program, _bn): (version_1::Program, <T as frame_system::Config>::BlockNumber)| {
                    let block_number = T::CurrentBlockNumber::get();
                    let expiration_block = block_number.saturating_add(FREE_PERIOD.into());
                    let task = ScheduledTask::PauseProgram(program_id);
                    TaskPoolOf::<T>::add(expiration_block, task)
                        .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));

                    // 1st read: old program data
                    // 1st write: new program data
                    // 2nd write: add task
                    weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 2));

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

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        use common::ProgramState;
        use version_1::{
            try_runtime::{set_program, State},
            ActiveProgram, Program,
        };

        let active_program = Program::Active(ActiveProgram {
            allocations: [1.into(), 2.into(), 3.into()].into(),
            pages_with_data: [1.into(), 2.into(), 3.into()].into(),
            gas_reservation_map: Default::default(),
            code_hash: Default::default(),
            code_exports: Default::default(),
            static_pages: Default::default(),
            state: ProgramState::Initialized,
        });
        let active = set_program::<T>(active_program.clone(), b"active");

        let exited_value_destination = ProgramId::generate(CodeId::from(u64::MAX), b"exited");
        let exited = set_program::<T>(Program::Exited(exited_value_destination), b"exited");

        let terminated_inheritor = ProgramId::generate(CodeId::from(u64::MAX), b"terminated");
        let terminated = set_program::<T>(Program::Terminated(terminated_inheritor), b"terminated");

        Ok(State {
            active,
            active_program,
            exited,
            exited_value_destination,
            terminated,
            terminated_inheritor,
        }
        .encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
        use version_1::{try_runtime::State, Program as OldProgram};

        let State {
            active,
            active_program: old_active_program,
            exited,
            exited_value_destination,
            terminated,
            terminated_inheritor,
        } = State::decode(&mut &state[..]).unwrap();

        let new_active_program = ProgramStorage::<T>::get(active).unwrap();
        match (new_active_program, old_active_program) {
            (Program::Active(new), OldProgram::Active(old)) => {
                assert_eq!(new.allocations, old.allocations);
                assert_eq!(new.pages_with_data, old.pages_with_data);
                assert_eq!(new.gas_reservation_map, old.gas_reservation_map);
                assert_eq!(new.code_hash, old.code_hash);
                assert_eq!(new.code_exports, old.code_exports);
                assert_eq!(new.static_pages, old.static_pages);
                assert_eq!(new.state, old.state);
                assert_eq!(
                    new.expiration_block,
                    T::CurrentBlockNumber::get() + FREE_PERIOD.into()
                );
            }
            _ => panic!("New and old program expected to be active"),
        }

        let exited = ProgramStorage::<T>::get(exited).unwrap();
        assert_eq!(exited, Program::Exited(exited_value_destination));

        let terminated = ProgramStorage::<T>::get(terminated).unwrap();
        assert_eq!(terminated, Program::Terminated(terminated_inheritor));

        Ok(())
    }
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

    pub mod try_runtime {
        use super::*;
        use crate::{Config, ProgramStorage};
        use frame_support::storage::generator::StorageMap;
        use frame_system::pallet_prelude::BlockNumberFor;
        use gear_core::ids::CodeId;

        #[derive(Debug, Encode, Decode)]
        pub struct State {
            pub active: ProgramId,
            pub active_program: Program,
            pub exited: ProgramId,
            pub exited_value_destination: ProgramId,
            pub terminated: ProgramId,
            pub terminated_inheritor: ProgramId,
        }

        pub fn set_program<T: Config>(program: Program, salt: &[u8]) -> ProgramId {
            let id = ProgramId::generate(CodeId::from(u64::MAX), salt);
            let key = ProgramStorage::<T>::storage_map_final_key(id);
            let bn = BlockNumberFor::<T>::from(u32::MAX);
            sp_io::storage::set(&key, &(program, bn).encode());
            id
        }
    }
}
