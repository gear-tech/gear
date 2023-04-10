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
    use common::{self, storage::*, CodeStorage, Origin, Program, ProgramStorage};
    use core::fmt;
    use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
    use frame_system::pallet_prelude::*;
    use gear_core::{
        ids::{CodeId, ProgramId},
        memory::{GearPage, PageBuf, PageU32Size, WasmPage},
        message::{StoredDispatch, StoredMessage},
    };
    use primitive_types::H256;
    use scale_info::TypeInfo;
    use sp_std::{collections::btree_map::BTreeMap, convert::TryInto, prelude::*};

    pub(crate) type QueueOf<T> = <<T as Config>::Messenger as Messenger>::Queue;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>
            + TryInto<Event<Self>>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;

        /// Storage with codes for programs.
        type CodeStorage: CodeStorage;

        type Messenger: Messenger<QueuedDispatch = StoredDispatch>;

        type ProgramStorage: ProgramStorage + IterableMap<(ProgramId, (Program, Self::BlockNumber))>;
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        DebugMode(bool),
        /// A snapshot of the debug data: programs and message queue ('debug mode' only)
        DebugDataSnapshot(DebugData),
    }

    // GearSupport pallet error.
    #[pallet::error]
    pub enum Error<T> {}

    /// Program debug info.
    #[derive(Encode, Decode, Clone, Default, PartialEq, Eq, TypeInfo)]
    pub struct ProgramInfo {
        pub static_pages: WasmPage,
        pub persistent_pages: BTreeMap<GearPage, PageBuf>,
        pub code_hash: H256,
    }

    impl fmt::Debug for ProgramInfo {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("ProgramInfo")
                .field("static_pages", &self.static_pages)
                .field(
                    "persistent_pages",
                    &self
                        .persistent_pages
                        .iter()
                        .map(|(page, data)|
                        // Prints only bytes which is not zero
                        (
                            *page,
                            data.iter()
                                .enumerate()
                                .filter(|(_, val)| **val != 0)
                                .map(|(idx, val)| (idx, *val))
                                .collect::<BTreeMap<_, _>>(),
                        ))
                        .collect::<BTreeMap<_, _>>(),
                )
                .field("code_hash", &self.code_hash)
                .finish()
        }
    }

    #[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
    pub enum ProgramState {
        Active(ProgramInfo),
        Terminated,
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

    #[pallet::storage]
    #[pallet::getter(fn remap_program_id)]
    pub type RemapId<T> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn programs_map)]
    pub type ProgramsMap<T> = StorageValue<_, BTreeMap<H256, H256>, ValueQuery>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// Initialization
        fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
            Weight::zero()
        }

        /// Finalization
        fn on_finalize(_bn: BlockNumberFor<T>) {}
    }

    #[derive(Decode, Encode)]
    struct Node {
        value: StoredDispatch,
        next: Option<H256>,
    }

    fn remap_with(dispatch: StoredDispatch, progs: &BTreeMap<H256, H256>) -> StoredDispatch {
        let (kind, msg, context) = dispatch.into_parts();
        let mut source = msg.source().into_origin();
        let mut destination = msg.destination().into_origin();

        for (k, v) in progs.iter() {
            let k = *k;
            let v = *v;

            if k == destination {
                destination = v;
            }

            if v == source {
                source = k;
            }
        }

        let message = StoredMessage::new(
            msg.id(),
            ProgramId::from_origin(source),
            ProgramId::from_origin(destination),
            (*msg.payload()).to_vec().try_into().unwrap(),
            msg.value(),
            msg.details(),
        );

        StoredDispatch::new(kind, message, context)
    }

    impl<T: Config> pallet_gear::DebugInfo for Pallet<T> {
        fn do_snapshot() {
            let dispatch_queue = QueueOf::<T>::iter()
                .map(|v| v.unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e)))
                .collect();

            let programs = T::ProgramStorage::iter()
                .map(|(id, (prog, _bn))| {
                    let active = match prog {
                        Program::Active(active) => active,
                        _ => {
                            return ProgramDetails {
                                id,
                                state: ProgramState::Terminated,
                            }
                        }
                    };
                    let code_id = CodeId::from_origin(active.code_hash);
                    let static_pages = match T::CodeStorage::get_code(code_id) {
                        Some(code) => code.static_pages(),
                        None => WasmPage::zero(),
                    };
                    let persistent_pages = T::ProgramStorage::get_program_data_for_pages(
                        id,
                        active.pages_with_data.iter(),
                    )
                    .unwrap();

                    ProgramDetails {
                        id,
                        state: {
                            ProgramState::Active(ProgramInfo {
                                static_pages,
                                persistent_pages,
                                code_hash: active.code_hash,
                            })
                        },
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

        fn is_remap_id_enabled() -> bool {
            Self::remap_program_id()
        }

        fn remap_id() {
            let programs_map = ProgramsMap::<T>::get();

            QueueOf::<T>::mutate_values(|d| remap_with(d, &programs_map));
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
