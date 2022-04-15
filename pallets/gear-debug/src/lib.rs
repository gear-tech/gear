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

#[macro_use]
extern crate alloc;

pub use pallet::*;
pub use weights::WeightInfo;

// #[cfg(feature = "runtime-benchmarks")]
// mod benchmarking;
pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::{self, Origin, Program};
    use core::fmt;
    use frame_support::{
        dispatch::DispatchResultWithPostInfo, pallet_prelude::*, storage::PrefixIterator,
    };
    use frame_system::pallet_prelude::*;
    use gear_core::{
        ids::ProgramId,
        memory::{PageNumber, WasmPageNumber},
        message::{StoredDispatch, StoredMessage},
    };
    use primitive_types::H256;
    use scale_info::TypeInfo;
    use sp_std::{collections::btree_map::BTreeMap, convert::TryInto, prelude::*};

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>>
            + IsType<<Self as frame_system::Config>::Event>
            + TryInto<Event<Self>>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::generate_store(pub(super) trait Store)]
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

    #[derive(Encode, Decode, Clone, Default, PartialEq, TypeInfo)]
    pub struct ProgramInfo {
        pub static_pages: WasmPageNumber,
        pub persistent_pages: BTreeMap<PageNumber, Vec<u8>>,
        pub code_hash: H256,
    }

    impl fmt::Debug for ProgramInfo {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("ProgramInfo")
                .field("static_pages", &self.static_pages)
                .field(
                    "persistent_pages",
                    &self.persistent_pages.keys().cloned().collect::<Vec<_>>(),
                )
                .field("code_hash", &self.code_hash)
                .finish()
        }
    }

    #[derive(Encode, Decode, Clone, PartialEq, TypeInfo, Debug)]
    pub enum ProgramState {
        Active(ProgramInfo),
        Terminated,
    }

    #[derive(Encode, Decode, Clone, PartialEq, TypeInfo, Debug)]
    pub struct ProgramDetails {
        pub id: H256,
        pub state: ProgramState,
    }

    #[derive(Debug, Encode, Decode, Clone, Default, PartialEq, TypeInfo)]
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
            0_u64
        }

        /// Finalization
        fn on_finalize(_bn: BlockNumberFor<T>) {}
    }

    #[derive(Decode, Encode)]
    struct Node {
        value: StoredDispatch,
        next: Option<H256>,
    }

    impl<T: Config> pallet_gear::DebugInfo for Pallet<T> {
        fn do_snapshot() {
            let mq_head_key = [common::STORAGE_MESSAGE_PREFIX, b"head"].concat();
            let mut dispatch_queue = vec![];

            if let Some(head) = sp_io::storage::get(&mq_head_key) {
                let mut next_id = H256::from_slice(&head[..]);
                loop {
                    let next_node_key =
                        [common::STORAGE_MESSAGE_PREFIX, next_id.as_bytes()].concat();
                    if let Some(bytes) = sp_io::storage::get(&next_node_key) {
                        let current_node = Node::decode(&mut &bytes[..]).unwrap();
                        dispatch_queue.push(current_node.value);
                        match current_node.next {
                            Some(h) => next_id = h,
                            None => break,
                        }
                    }
                }
            }

            let programs = PrefixIterator::<(H256, Program)>::new(
                common::STORAGE_PROGRAM_PREFIX.to_vec(),
                common::STORAGE_PROGRAM_PREFIX.to_vec(),
                |key, mut value| {
                    assert_eq!(key.len(), 32);
                    let program_id = H256::from_slice(key);
                    let program = Program::decode(&mut value)?;
                    Ok((program_id, program))
                },
            )
            .map(|(id, p)| ProgramDetails {
                id,
                state: if let Program::Active(active) = p {
                    ProgramState::Active(ProgramInfo {
                        static_pages: active.static_pages,
                        persistent_pages: common::get_program_pages(id, active.persistent_pages)
                            .expect("active program exists, therefore pages do"),
                        code_hash: active.code_hash,
                    })
                } else {
                    ProgramState::Terminated
                },
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
            let mq_head_key = [common::STORAGE_MESSAGE_PREFIX, b"head"].concat();

            if let Some(head) = sp_io::storage::get(&mq_head_key) {
                let mut next_id = H256::from_slice(&head[..]);
                loop {
                    let next_node_key =
                        [common::STORAGE_MESSAGE_PREFIX, next_id.as_bytes()].concat();
                    if let Some(bytes) = sp_io::storage::get(&next_node_key) {
                        let mut current_node = Node::decode(&mut &bytes[..]).unwrap();
                        for (k, v) in programs_map.iter() {
                            if *k == current_node.value.destination().into_origin() {
                                current_node.value = StoredDispatch::new(
                                    current_node.value.kind(),
                                    StoredMessage::new(
                                        current_node.value.id(),
                                        current_node.value.source(),
                                        ProgramId::from_origin(*v),
                                        (*current_node.value.payload()).to_vec(),
                                        current_node.value.value(),
                                        current_node.value.reply(),
                                    ),
                                    current_node.value.context().clone(),
                                );

                                sp_io::storage::set(&next_node_key, &current_node.encode());
                            }

                            if *v == current_node.value.source().into_origin() {
                                current_node.value = StoredDispatch::new(
                                    current_node.value.kind(),
                                    StoredMessage::new(
                                        current_node.value.id(),
                                        ProgramId::from_origin(*k),
                                        current_node.value.destination(),
                                        (*current_node.value.payload()).to_vec(),
                                        current_node.value.value(),
                                        current_node.value.reply(),
                                    ),
                                    current_node.value.context().clone(),
                                );

                                sp_io::storage::set(&next_node_key, &current_node.encode());
                            }
                        }

                        match current_node.next {
                            Some(h) => next_id = h,
                            None => break,
                        }
                    }
                }
            }
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
