// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
pub use weights::WeightInfo;

pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::{self, storage::*, Origin, Program, ProgramStorage};
    use core::fmt;
    use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
    use frame_system::pallet_prelude::*;
    use gear_core::{
        ids::{CodeId, ProgramId},
        memory::{GearPage, PageBuf},
        message::StoredDispatch,
    };
    use scale_info::TypeInfo;
    use sp_std::{collections::btree_map::BTreeMap, convert::TryInto, prelude::*};

    pub(crate) type QueueOf<T> = <<T as Config>::Messenger as Messenger>::Queue;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>
            + TryInto<Event<Self>>;

        /// Storage with messages.
        type Messenger: Messenger<QueuedDispatch = StoredDispatch>;

        /// Storage with programs data.
        type ProgramStorage: ProgramStorage + IterableMap<(ProgramId, (Program, Self::BlockNumber))>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Event showing state of debug mode on its change.
        DebugMode(bool),
        /// A snapshot of the debug data with debug mode turned on.
        DebugDataSnapshot(DebugData),
    }

    /// Program debug info.
    #[derive(Encode, Decode, Clone, Default, PartialEq, Eq, TypeInfo)]
    pub struct ProgramInfo {
        pub code_id: CodeId,
        pub persistent_pages: BTreeMap<GearPage, PageBuf>,
    }

    impl fmt::Debug for ProgramInfo {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("ProgramInfo")
                .field("code_id", &self.code_id)
                .field(
                    "persistent_pages",
                    &self
                        .persistent_pages
                        .iter()
                        .map(|(page, data)|
                        // Prints only bytes which is not zero
                        (
                            page,
                            data.iter()
                                .enumerate()
                                .filter(|(_, &val)| val != 0)
                                .map(|(idx, val)| (idx, val))
                                .collect::<BTreeMap<_, _>>(),
                        ))
                        .collect::<BTreeMap<_, _>>(),
                )
                .finish()
        }
    }

    #[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
    pub enum ProgramState {
        Active(ProgramInfo),
        Inactive,
    }

    #[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
    pub struct ProgramDetails {
        pub id: ProgramId,
        pub state: ProgramState,
    }

    #[derive(Debug, Encode, Decode, Clone, Default, PartialEq, Eq, TypeInfo)]
    pub struct DebugData {
        pub dispatch_queue: Vec<StoredDispatch>,
        pub programs: Vec<ProgramDetails>,
    }

    #[pallet::storage]
    #[pallet::getter(fn debug_mode)]
    pub type DebugMode<T> = StorageValue<_, bool, ValueQuery>;

    impl<T: Config> pallet_gear::DebugInfo for Pallet<T> {
        fn do_snapshot() {
            let dispatch_queue = QueueOf::<T>::iter()
                .map(|v| v.unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e)))
                .collect();

            let programs = T::ProgramStorage::iter()
                .map(|(id, (prog, _bn))| {
                    let Program::Active(program) = prog else {
                        return ProgramDetails {
                            id,
                            state: ProgramState::Inactive,
                        }
                    };

                    let code_id = CodeId::from_origin(program.code_hash);
                    let persistent_pages = T::ProgramStorage::get_program_data_for_pages(
                        id,
                        program.pages_with_data.iter(),
                    )
                    .unwrap_or_else(|e| unreachable!("Program storage corrupted! {:?}", e));

                    ProgramDetails {
                        id,
                        state: ProgramState::Active(ProgramInfo {
                            code_id,
                            persistent_pages,
                        }),
                    }
                })
                .collect();

            Self::deposit_event(Event::DebugDataSnapshot(DebugData {
                dispatch_queue,
                programs,
            }));
        }

        fn is_enabled() -> bool {
            Self::debug_mode()
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Turn the debug mode on and off.
        ///
        /// The origin must be the root.
        ///
        /// Parameters:
        /// - `debug_mode_on`: if true, debug mode will be turned on, turned off otherwise.
        ///
        /// Emits the following events:
        /// - `DebugMode(debug_mode_on).
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::enable_debug_mode())]
        pub fn enable_debug_mode(
            origin: OriginFor<T>,
            debug_mode_on: bool,
        ) -> DispatchResultWithPostInfo {
            ensure_root(origin)?;
            DebugMode::<T>::put(debug_mode_on);

            Self::deposit_event(Event::DebugMode(debug_mode_on));

            // This extrinsic is not chargeable
            Ok(Pays::No.into())
        }
    }
}
