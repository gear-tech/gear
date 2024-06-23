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

//! Pallet Gear Eth Bridge.

#![cfg_attr(not(feature = "std"), no_std)]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![warn(missing_docs)]

pub use builtin::Actor;
pub use internal::{EthMessage, Proof};
pub use pallet::*;

// TODO (breathx): impl `mock` and `tests` modules.
mod builtin;
mod internal;

#[allow(missing_docs)]
#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::Origin;
    use frame_support::{
        pallet_prelude::*,
        traits::{OneSessionHandler, StorageVersion},
        StorageHasher,
    };
    use frame_system::{
        ensure_root, ensure_signed,
        pallet_prelude::{BlockNumberFor, OriginFor},
    };
    use gprimitives::{ActorId, H160, H256, U256};
    use sp_runtime::{
        traits::{Keccak256, One, Saturating, Zero},
        BoundToRuntimeAppPublic, RuntimeAppPublic,
    };
    use sp_std::vec::Vec;

    type QueueCapacityOf<T> = <T as Config>::QueueCapacity;
    type SessionsPerEraOf<T> = <T as Config>::SessionsPerEra;

    /// Pallet Gear Eth Bridge's storage version.
    pub const ETH_BRIDGE_STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    /// Pallet Gear Eth Bridge's config.
    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_staking::Config {
        /// Type representing aggregated runtime event.
        type RuntimeEvent: From<Event<Self>>
            + TryInto<Event<Self>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Constant defining maximal payload size in bytes of message for bridging.
        #[pallet::constant]
        type MaxPayloadSize: Get<u32>;

        /// Constant defining maximal amount of messages that are able to be
        /// bridged within the single staking era.
        #[pallet::constant]
        type QueueCapacity: Get<u32>;

        /// Constant defining amount of sessions in manager for keys rotation.
        /// Similar to `pallet_staking::SessionsPerEra`.
        #[pallet::constant]
        type SessionsPerEra: Get<u32>;
    }

    /// Pallet Gear Eth Bridge's event.
    #[pallet::event]
    #[pallet::generate_deposit(fn deposit_event)]
    pub enum Event<T> {
        /// Bridge got cleared on initialization of the second block in a new era.
        BridgeCleared,

        /// Optimistically, single-time called event defining that pallet
        /// got initialized and started processing session changes,
        /// as well as putting initial zeroed queue merkle root.
        BridgeInitialized,

        /// Bridge was paused and temporary doesn't process any incoming requests.
        BridgePaused,

        /// Bridge was unpaused and from now on processes any incoming requests.
        BridgeUnpaused,

        /// Grandpa validator's keys set was hashed and set in storage at
        /// first block of the last session in the era.
        GrandpaSetHashChanged(H256),

        /// A new message was queued for bridging.
        MessageQueued { message: EthMessage, hash: H256 },

        /// Merkle root of the queue changed: new messages queued within the block.
        QueueMerkleRootChanged(H256),
    }

    /// Pallet Gear Eth Bridge's error.
    #[pallet::error]
    pub enum Error<T> {
        /// The error happens when bridge got called before
        /// proper initialization after deployment.
        BridgeIsNotYetInitialized,

        /// The error happens when bridge got called when paused.
        BridgeIsPaused,

        /// The error happens when bridging message sent with too big payload.
        MaxPayloadSizeExceeded,

        /// The error happens when bridging queue capacity exceeded,
        /// so message couldn't be sent.
        QueueCapacityExceeded,
    }

    /// Lifecycle storage.
    ///
    /// Defines if pallet got initialized and focused on common session changes.
    #[pallet::storage]
    type Initialized<T> = StorageValue<_, bool, ValueQuery>;

    /// Lifecycle storage.
    ///
    /// Defines if pallet is accepting any mutable requests. Governance-ruled.
    #[pallet::storage]
    type Paused<T> = StorageValue<_, bool, ValueQuery>;

    /// Primary storage.
    ///
    /// Keeps hash of queued validator keys for the next era.
    ///
    /// **Invariant**: Key exists in storage since first block of some era,
    /// until initialization of the second block of the next era.
    #[pallet::storage]
    type GrandpaSetHash<T> = StorageValue<_, H256>;

    /// Primary storage.
    ///
    /// Keeps merkle root of the bridge's queued messages.
    ///
    /// **Invariant**: Key exists since pallet initialization. If queue is empty,
    /// zeroed hash set in storage.
    #[pallet::storage]
    type QueueMerkleRoot<T> = StorageValue<_, H256>;

    /// Primary storage.
    ///
    /// Keeps bridge's queued messages keccak hashes.
    #[pallet::storage]
    type Queue<T> = StorageValue<_, BoundedVec<H256, QueueCapacityOf<T>>, ValueQuery>;

    /// Operational storage.
    ///
    /// Declares timer of the session changes (`on_new_session` calls),
    /// when `queued_validators` must be stored within the pallet.
    ///
    /// **Invariant**: reducing each time on new session, it equals 0 only
    /// since storing grandpa keys hash until next session change,
    /// when it becomes `SessionPerEra - 1`.
    #[pallet::storage]
    type SessionsTimer<T> = StorageValue<_, u32, ValueQuery>;

    /// Operational storage.
    ///
    /// Defines should queue, queue merkle root and grandpa keys hash be cleared.
    ///
    /// **Invariant**: exist in storage and equals `true` once per era in the
    /// very first block, to perform removal on next block's initialization.
    #[pallet::storage]
    type ClearScheduled<T> = StorageValue<_, bool, ValueQuery>;

    /// Operational storage.
    ///
    /// Keeps next message's nonce for bridging. Must be increased on each use.
    #[pallet::storage]
    pub(crate) type MessageNonce<T> = StorageValue<_, U256, ValueQuery>;

    /// Operational storage.
    ///
    /// Defines if queue was changed within the block, it's necessary to
    /// update queue merkle root by the end of the block.
    #[pallet::storage]
    type QueueChanged<T> = StorageValue<_, bool, ValueQuery>;

    /// Pallet Gear Eth Bridge's itself.
    #[pallet::pallet]
    #[pallet::storage_version(ETH_BRIDGE_STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // TODO (breathx): write benchmarks.
    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
        #[pallet::call_index(0)]
        #[pallet::weight(T::DbWeight::get().reads_writes(2, 1))]
        pub fn pause(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            // Ensuring called by root.
            ensure_root(origin)?;

            // Ensuring that pallet is initialized.
            ensure!(
                Initialized::<T>::get(),
                Error::<T>::BridgeIsNotYetInitialized
            );

            // Checking if pallet is paused already, otherwise pausing it.
            if !Paused::<T>::get() {
                // Updating storage value.
                Paused::<T>::put(true);

                // Depositing event about bridge being paused.
                Self::deposit_event(Event::<T>::BridgePaused);
            }

            // Returning successful result without weight refund.
            Ok(().into())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::DbWeight::get().reads_writes(2, 1))]
        pub fn unpause(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            // Ensuring called by root.
            ensure_root(origin)?;

            // Ensuring that pallet is initialized.
            ensure!(
                Initialized::<T>::get(),
                Error::<T>::BridgeIsNotYetInitialized
            );

            // Checking if pallet is paused, removing key, so unpausing it.
            if Paused::<T>::take() {
                // Depositing event about bridge being unpaused.
                Self::deposit_event(Event::<T>::BridgeUnpaused);
            }

            // Returning successful result without weight refund.
            Ok(().into())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(Weight::zero())]
        pub fn send_eth_message(
            origin: OriginFor<T>,
            destination: H160,
            payload: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            let source = ensure_signed(origin)?.cast();

            Self::queue_message(source, destination, payload)?;

            Ok(().into())
        }
    }

    impl<T: Config> Pallet<T> {
        pub(crate) fn queue_message(
            source: ActorId,
            destination: H160,
            payload: Vec<u8>,
        ) -> Result<(U256, H256), Error<T>> {
            // Ensuring that pallet is initialized.
            ensure!(
                Initialized::<T>::get(),
                Error::<T>::BridgeIsNotYetInitialized
            );

            // Ensuring that pallet isn't paused.
            ensure!(!Paused::<T>::get(), Error::<T>::BridgeIsPaused);

            // Creating new message from given data.
            //
            // Inside goes query and bump of nonce,
            // as well as checking payload size.
            let message = EthMessage::try_new(source, destination, payload)?;

            // Appending hash of the message into the queue
            // if it's capacity wasn't exceeded.
            let hash = Queue::<T>::mutate(|v| {
                (v.len() < QueueCapacityOf::<T>::get() as usize)
                    .then(|| {
                        let hash = message.hash();

                        // Always `Ok`: check performed above as in inner implementation.
                        v.try_push(hash).map(|()| hash).ok()
                    })
                    .flatten()
                    .ok_or(Error::<T>::QueueCapacityExceeded)
            })
            .map_err(|e| {
                // In case of error, reverting increase of `MessageNonce` performed
                // in message creation to keep builtin interactions transactional.
                MessageNonce::<T>::mutate_exists(|nonce| {
                    *nonce = nonce.and_then(|inner| {
                        inner.checked_sub(U256::one()).filter(|new| !new.is_zero())
                    });
                });

                e
            })?;

            // Marking queue as changed, so root will be updated later.
            QueueChanged::<T>::put(true);

            // Extracting nonce to return.
            let nonce = message.nonce();

            // Depositing event about message being queued for bridging.
            Self::deposit_event(Event::<T>::MessageQueued { message, hash });

            Ok((nonce, hash))
        }

        /// Returns merkle inclusion proof of the message hash in the queue.
        pub fn merkle_proof(hash: H256) -> Option<Proof> {
            // Querying actual queue.
            let queue = Queue::<T>::get();

            // Lookup for hash index within the queue.
            let idx = queue.iter().position(|&v| v == hash)?;

            // Generating proof.
            let proof = binary_merkle_tree::merkle_proof::<Keccak256, _, _>(queue, idx);

            // Returning appropriate type.
            Some(proof.into())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
            // Resulting weight of the hook.
            //
            // Initially consists of one read of `ClearScheduled` storage.
            let mut weight = T::DbWeight::get().reads(1);

            // Checking if clear operation was scheduled by session handler.
            if ClearScheduled::<T>::take() {
                // Removing grandpa set hash from storage.
                GrandpaSetHash::<T>::kill();

                // Removing queued messages from storage.
                Queue::<T>::kill();

                // Setting zero queue root, keeping invariant of this key existence.
                QueueMerkleRoot::<T>::put(H256::zero());

                // Increasing resulting weight by 3 writes of above keys removal.
                weight = weight.saturating_add(T::DbWeight::get().writes(3));
            }

            // Returning weight.
            weight
        }

        fn on_finalize(_bn: BlockNumberFor<T>) {
            // If queue wasn't changed, than nothing to do here.
            if !QueueChanged::<T>::get() {
                return;
            }

            // Querying actual queue.
            let queue = Queue::<T>::get();

            // Checking invariant.
            //
            // If queue was changed within the block, it couldn't be empty.
            debug_assert!(!queue.is_empty());

            // Calculating new queue merkle root.
            let root = binary_merkle_tree::merkle_root::<Keccak256, _>(queue);

            // Updating queue merkle root in storage.
            QueueMerkleRoot::<T>::put(root);

            // Depositing event about queue root being updated.
            Self::deposit_event(Event::<T>::QueueMerkleRootChanged(root));
        }
    }

    impl<T: Config> BoundToRuntimeAppPublic for Pallet<T> {
        type Public = sp_consensus_grandpa::AuthorityId;
    }

    impl<T: Config> OneSessionHandler<T::AccountId> for Pallet<T> {
        type Key = <Self as BoundToRuntimeAppPublic>::Public;

        // TODO (breathx): support genesis session and avoid `Initialized` storage.
        fn on_genesis_session<'a, I: 'a>(_validators: I) {}

        // TODO (breathx): support `Stalled` changes of grandpa.
        fn on_new_session<'a, I: 'a>(changed: bool, _validators: I, queued_validators: I)
        where
            I: Iterator<Item = (&'a T::AccountId, Self::Key)>,
        {
            // If historically pallet hasn't yet faced `changed = true`,
            // any type of calculations aren't performed.
            if !Initialized::<T>::get() && !changed {
                return;
            }

            // First time facing `changed = true`, so from now on, pallet
            // is starting handling grandpa sets and queue.
            if !Initialized::<T>::get() && changed {
                // Setting pallet status initialized.
                Initialized::<T>::put(true);

                // Depositing event about getting initialized.
                Self::deposit_event(Event::<T>::BridgeInitialized);

                // Invariant.
                //
                // At any single point of pallet existence, when it's active
                // and queue is empty, queue merkle root must present
                // in storage and be zeroed.
                QueueMerkleRoot::<T>::put(H256::zero());
            }

            // Here starts common processing of properly initialized pallet.
            if changed {
                // Checking invariant.
                //
                // Reset scheduling must be resolved on the first block
                // after session changed.
                debug_assert!(!ClearScheduled::<T>::get());

                // Scheduling reset on next block's init.
                ClearScheduled::<T>::put(true);

                // Checking invariant.
                //
                // Timer is supposed to be `null` (default zero), if was just
                // initialized, otherwise zero set in storage.
                debug_assert!(SessionsTimer::<T>::get().is_zero());

                // Scheduling settlement of grandpa keys in `SessionsPerEra - 1` session changes.
                SessionsTimer::<T>::put(SessionsPerEraOf::<T>::get().saturating_sub(One::one()));
            } else {
                // Reducing timer. If became zero, it means we're at the last
                // session of the era and queued keys must be kept.
                let to_set_grandpa_keys = SessionsTimer::<T>::mutate(|timer| {
                    timer.saturating_dec();
                    timer.is_zero()
                });

                // Setting future keys hash, if needed.
                if to_set_grandpa_keys {
                    // Collecting all keys into `Vec<u8>`.
                    let keys_bytes = queued_validators
                        .flat_map(|(_, key)| key.to_raw_vec())
                        .collect::<Vec<_>>();

                    // Hashing keys bytes with `Blake2`.
                    let grandpa_set_hash = Blake2_256::hash(&keys_bytes).into();

                    // Setting new grandpa set hash into storage.
                    GrandpaSetHash::<T>::put(grandpa_set_hash);

                    // Depositing event about update in the set.
                    Self::deposit_event(Event::<T>::GrandpaSetHashChanged(grandpa_set_hash));
                }
            }
        }

        fn on_disabled(_validator_index: u32) {}
    }
}
