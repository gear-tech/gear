// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

//! # Gear Bridge Pallet

#![cfg_attr(not(feature = "std"), no_std)]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]

mod builtin;
mod internal;

pub use crate::internal::Proof;
pub use builtin::Actor;
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use crate::internal::{EthMessage, EthMessageData, Proof};
    use common::Origin;
    use frame_support::{
        pallet_prelude::*,
        traits::{OneSessionHandler, StorageVersion},
        StorageHasher,
    };
    use frame_system::pallet_prelude::*;
    use gear_core::message::{Payload, PayloadSizeError};
    use primitive_types::{H160, H256, U256};
    use sp_runtime::{traits::One, BoundToRuntimeAppPublic, Saturating};
    use sp_std::vec::Vec;

    pub(crate) use binary_merkle_tree as merkle_tree;

    pub type KeccakHasher = sp_runtime::traits::Keccak256;

    pub use frame_support::weights::Weight;

    pub const BRIDGE_STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    // TODO (breathx): rm session and staking bound.
    #[pallet::config]
    pub trait Config:
        frame_system::Config + pallet_session::Config + pallet_staking::Config
    {
        type RuntimeEvent: From<Event<Self>>
            + TryInto<Event<Self>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        #[pallet::constant]
        type QueueLimit: Get<u32>;
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T> {
        MessageQueued { nonce: U256, hash: H256 },
        Reset,
        RootUpdated(H256),
        SetPaused(bool),
        ValidatorsSetUpdated(H256),
    }

    // TODO (breathx): NonZeroValue for builtin actor
    #[pallet::error]
    pub enum Error<T> {
        BridgePaused,
        QueueLimitExceeded,
    }

    #[pallet::pallet]
    #[pallet::storage_version(BRIDGE_STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub(crate) type NextNonce<T> = StorageValue<_, U256, ValueQuery>;

    // TODO (breathx): consider what pause should stop: incoming requests vs whole workflow
    #[pallet::storage]
    pub(crate) type Paused<T> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    pub(crate) type Queue<T> =
        StorageValue<_, BoundedVec<H256, <T as Config>::QueueLimit>, ValueQuery>;

    #[pallet::storage]
    pub(crate) type QueueChanged<T> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    pub(crate) type QueueRoot<T> = StorageValue<_, H256>;

    #[pallet::storage]
    pub(crate) type ResetOnInitOf<T> = StorageValue<_, BlockNumberFor<T>>;

    #[pallet::storage]
    pub(crate) type UpdatedRecently<T> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    pub(crate) type ValidatorsSet<T> = StorageValue<_, H256>;

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::zero())]
        pub fn set_paused(origin: OriginFor<T>, paused: bool) -> DispatchResultWithPostInfo {
            ensure_root(origin)?;

            if Paused::<T>::get() != paused {
                Paused::<T>::put(paused);
                Self::deposit_event(Event::<T>::SetPaused(paused));
            }

            Ok(().into())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(Weight::zero())]
        pub fn send(
            origin: OriginFor<T>,
            destination: H160,
            payload: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            // TODO (breathx): avoid here double payload wrapping for builtin.
            let payload = payload
                .try_into()
                .map_err(|e: PayloadSizeError| DispatchError::Other(e.into()))?;

            let _ = Self::send_impl(who.cast(), destination, payload)?;

            Ok(().into())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    where
        T::AccountId: Origin,
    {
        // TODO (breathx): add max weight of root calculation
        fn on_initialize(n: BlockNumberFor<T>) -> Weight {
            let mut weight = Weight::zero();

            QueueChanged::<T>::kill();
            weight = weight.saturating_add(T::DbWeight::get().writes(1));

            if ResetOnInitOf::<T>::get() == Some(n) {
                ResetOnInitOf::<T>::kill();
                weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

                Queue::<T>::kill();
                QueueRoot::<T>::put(H256::zero());
                ValidatorsSet::<T>::kill();
                weight = weight.saturating_add(T::DbWeight::get().writes(3));

                Self::deposit_event(Event::<T>::Reset);
            } else {
                weight = weight.saturating_add(T::DbWeight::get().reads(1));
            };

            weight
        }

        fn on_finalize(_bn: BlockNumberFor<T>) {
            if !QueueChanged::<T>::get() {
                return;
            }

            let queue = Queue::<T>::get();

            if queue.is_empty() {
                log::error!("Queue supposed to be non-empty");
                return;
            };

            let root = merkle_tree::merkle_root::<KeccakHasher, _>(queue);

            QueueRoot::<T>::put(root);

            Self::deposit_event(Event::<T>::RootUpdated(root));
        }
    }

    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
        pub(crate) fn send_impl(
            source: H256,
            destination: H160,
            payload: Payload,
        ) -> Result<(U256, H256), Error<T>> {
            ensure!(!Paused::<T>::get(), Error::<T>::BridgePaused);

            let nonce = Self::fetch_inc_nonce();
            let data = EthMessageData::new(destination, payload);

            let message = EthMessage::from_data(nonce, source, data);

            Self::queue(&message)
        }

        fn fetch_inc_nonce() -> U256 {
            NextNonce::<T>::mutate(|v| {
                let nonce = *v;
                *v = nonce.saturating_add(U256::one());
                nonce
            })
        }

        fn queue(message: &EthMessage) -> Result<(U256, H256), Error<T>> {
            let hash = Queue::<T>::mutate(|v| {
                (v.len() < T::QueueLimit::get() as usize)
                    .then(|| {
                        let hash = message.hash();

                        // Always `Ok`: check performed above as in inner implementation.
                        v.try_push(hash).map(|()| hash).ok()
                    })
                    .flatten()
                    .ok_or(Error::<T>::QueueLimitExceeded)
            })?;

            QueueChanged::<T>::put(true);

            let nonce = message.nonce();

            Self::deposit_event(Event::<T>::MessageQueued { nonce, hash });

            Ok((nonce, hash))
        }

        pub fn merkle_proof(hash: H256) -> Option<Proof> {
            let queue = Queue::<T>::get();

            let idx = queue.iter().position(|&v| v == hash)?;

            let proof = merkle_tree::merkle_proof::<KeccakHasher, _, _>(queue, idx);

            Some(proof.into())
        }
    }

    impl<T: Config> Pallet<T> {
        fn update_validators_set<'a, I: 'a>(validators: I)
        where
            I: Iterator<Item = (&'a T::AccountId, sp_consensus_grandpa::AuthorityId)>,
        {
            let concat_grandpa_keys: Vec<_> = validators
                .flat_map(|(_, key)| key.clone().into_inner().0)
                .collect();

            let validators_set_hash = Blake2_256::hash(&concat_grandpa_keys).into();

            ValidatorsSet::<T>::put(validators_set_hash);

            Self::deposit_event(Event::<T>::ValidatorsSetUpdated(validators_set_hash));
        }
    }

    impl<T: Config> BoundToRuntimeAppPublic for Pallet<T> {
        type Public = sp_consensus_grandpa::AuthorityId;
    }

    impl<T: Config> OneSessionHandler<T::AccountId> for Pallet<T> {
        type Key = sp_consensus_grandpa::AuthorityId;

        fn on_genesis_session<'a, I: 'a>(_validators: I) {}

        fn on_new_session<'a, I: 'a>(changed: bool, _validators: I, queued_validators: I)
        where
            I: Iterator<Item = (&'a T::AccountId, Self::Key)>,
        {
            if changed {
                Self::update_validators_set(queued_validators);
                UpdatedRecently::<T>::put(true);
            } else if UpdatedRecently::<T>::take() {
                // ATTENTION: this algo needs to call init function before pallet session!
                ResetOnInitOf::<T>::put(
                    frame_system::Pallet::<T>::block_number().saturating_add(One::one()),
                )
            }
        }
        fn on_disabled(_validator_index: u32) {}
    }
}
