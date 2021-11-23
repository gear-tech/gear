// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

pub trait DebugInfo {
    fn do_snapshot();
    fn is_enabled() -> bool;
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::{self, Message};
    use frame_support::{
        dispatch::DispatchResultWithPostInfo, pallet_prelude::*, storage::PrefixIterator,
    };
    use frame_system::pallet_prelude::*;
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

    #[derive(Debug, Encode, Decode, Clone, PartialEq, TypeInfo)]
    pub struct ProgramDetails {
        pub id: H256,
        pub static_pages: u32,
        pub persistent_pages: BTreeMap<u32, Vec<u8>>,
        pub code_hash: H256,
        pub nonce: u64,
    }

    #[derive(Debug, Encode, Decode, Clone, PartialEq, TypeInfo)]
    pub struct DebugData {
        pub message_queue: Vec<Message>,
        pub programs: Vec<ProgramDetails>,
    }

    #[pallet::storage]
    #[pallet::getter(fn debug_mode)]
    pub type DebugMode<T> = StorageValue<_, bool, ValueQuery>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// Initialization
        fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
            0_u64
        }

        /// Finalization
        fn on_finalize(_bn: BlockNumberFor<T>) {}
    }

    impl<T: Config> DebugInfo for Pallet<T> {
        fn do_snapshot() {
            #[derive(Decode)]
            struct Node {
                value: Message,
                next: Option<H256>,
            }

            let mq_head_key = [common::STORAGE_MESSAGE_PREFIX, b"head"].concat();
            let mut message_queue = vec![];

            if let Some(head) = sp_io::storage::get(&mq_head_key) {
                let mut next_id = H256::from_slice(&head[..]);
                loop {
                    let next_node_key =
                        [common::STORAGE_MESSAGE_PREFIX, next_id.as_bytes()].concat();
                    if let Some(bytes) = sp_io::storage::get(&next_node_key) {
                        let current_node = Node::decode(&mut &bytes[..]).unwrap();
                        message_queue.push(current_node.value);
                        match current_node.next {
                            Some(h) => next_id = h,
                            None => break,
                        }
                    }
                }
            }

            let programs = PrefixIterator::<ProgramDetails>::new(
                common::STORAGE_PROGRAM_PREFIX.to_vec(),
                common::STORAGE_PROGRAM_PREFIX.to_vec(),
                |key, mut value| {
                    assert_eq!(key.len(), 32);
                    let program_id = H256::from_slice(key);
                    let program = common::Program::decode(&mut value)?;
                    Ok(ProgramDetails {
                        id: program_id,
                        static_pages: program.static_pages,
                        persistent_pages: common::get_program_pages(
                            program_id,
                            program.persistent_pages,
                        ),
                        code_hash: program.code_hash,
                        nonce: program.nonce,
                    })
                },
            )
            .collect();

            Self::deposit_event(Event::DebugDataSnapshot(DebugData {
                message_queue,
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
