// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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
// TODO: remove on rust update.
#![allow(unknown_lints)]
#![allow(clippy::manual_inspect)]
#![allow(clippy::useless_conversion)]

pub use builtin::Actor;
pub use pallet::*;
pub use pallet_gear_eth_bridge_primitives::{EthMessage, Proof};
pub use weights::WeightInfo;

use internal::EthMessageExt;

pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod builtin;
mod internal;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::Origin;
    use frame_support::{
        PalletId, StorageHasher,
        pallet_prelude::*,
        traits::{
            ConstBool, Currency, ExistenceRequirement, OneSessionHandler, StorageInstance,
            StorageVersion,
        },
    };
    use frame_system::{
        ensure_signed,
        pallet_prelude::{BlockNumberFor, OriginFor},
    };
    use gprimitives::{ActorId, H160, H256, U256};
    use sp_runtime::{
        BoundToRuntimeAppPublic, RuntimeAppPublic,
        traits::{Keccak256, One, Saturating, Zero},
    };
    use sp_std::vec::Vec;

    type QueueCapacityOf<T> = <T as Config>::QueueCapacity;
    type SessionsPerEraOf<T> = <T as Config>::SessionsPerEra;
    type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
    type BalanceOf<T> = <CurrencyOf<T> as Currency<AccountIdOf<T>>>::Balance;
    pub(crate) type CurrencyOf<T> = <T as pallet_gear_bank::Config>::Currency;

    /// Pallet Gear Eth Bridge's storage version.
    pub const ETH_BRIDGE_STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    /// Pallet Gear Eth Bridge's config.
    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_gear_bank::Config {
        /// Type representing aggregated runtime event.
        type RuntimeEvent: From<Event<Self>>
            + TryInto<Event<Self>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The bridge' pallet id, used for deriving its sovereign account ID.
        #[pallet::constant]
        type PalletId: Get<PalletId>;

        /// Account ID of the bridge builtin.
        #[pallet::constant]
        type BuiltinAddress: Get<Self::AccountId>;

        /// Privileged origin for bridge management operations.
        type ControlOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Privileged origin for administrative operations.
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// The AccountId of the bridge admin.
        #[pallet::constant]
        type BridgeAdmin: Get<Self::AccountId>;

        /// The AccountId of the bridge pauser.
        #[pallet::constant]
        type BridgePauser: Get<Self::AccountId>;

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

        /// Weight cost incurred by pallet calls.
        type WeightInfo: WeightInfo;
    }

    /// Pallet Gear Eth Bridge's event.
    #[pallet::event]
    #[pallet::generate_deposit(fn deposit_event)]
    pub enum Event<T> {
        /// Grandpa validator's keys set was hashed and set in storage at
        /// first block of the last session in the era.
        AuthoritySetHashChanged(H256),

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

        /// A new message was queued for bridging.
        MessageQueued {
            /// Enqueued message.
            message: EthMessage,
            /// Hash of the enqueued message.
            hash: H256,
        },

        /// Merkle root of the queue changed: new messages queued within the block.
        QueueMerkleRootChanged(H256),
    }

    /// Pallet Gear Eth Bridge's error.
    #[pallet::error]
    #[cfg_attr(test, derive(Clone))]
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

        /// The error happens when bridging thorough builtin and message value
        /// is inapplicable to operation or insufficient.
        IncorrectValueApplied,
    }

    /// Lifecycle storage.
    ///
    /// Defines if pallet got initialized and focused on common session changes.
    #[pallet::storage]
    pub(crate) type Initialized<T> = StorageValue<_, bool, ValueQuery>;

    /// Lifecycle storage.
    ///
    /// Defines if pallet is accepting any mutable requests. Governance-ruled.
    #[pallet::storage]
    pub(crate) type Paused<T> = StorageValue<_, bool, ValueQuery, ConstBool<true>>;

    /// Primary storage.
    ///
    /// Keeps hash of queued validator keys for the next era.
    ///
    /// **Invariant**: Key exists in storage since first block of some era's last
    /// session, until initialization of the second block of the next era.
    #[pallet::storage]
    pub(crate) type AuthoritySetHash<T> = StorageValue<_, H256>;

    /// Primary storage.
    ///
    /// Keeps merkle root of the bridge's queued messages.
    ///
    /// **Invariant**: Key exists since pallet initialization. If queue is empty,
    /// zeroed hash set in storage.
    #[pallet::storage]
    pub(crate) type QueueMerkleRoot<T> = StorageValue<_, H256>;

    /// Primary storage.
    ///
    /// Keeps bridge's queued messages keccak hashes.
    #[pallet::storage]
    #[pallet::unbounded]
    pub(crate) type Queue<T> = StorageValue<_, Vec<H256>, ValueQuery>;

    /// Operational storage.
    ///
    /// Declares timer of the session changes (`on_new_session` calls),
    /// when `queued_validators` must be stored within the pallet.
    ///
    /// **Invariant**: reducing each time on new session, it equals 0 only
    /// since storing grandpa keys hash until next session change,
    /// when it becomes `SessionPerEra - 1`.
    #[pallet::storage]
    pub(crate) type SessionsTimer<T> = StorageValue<_, u32, ValueQuery>;

    /// Operational storage.
    ///
    /// Defines in how many on_initialize hooks queue, queue merkle root and
    /// grandpa keys hash should be cleared.
    ///
    /// **Invariant**: set to 2 on_init hooks when new session with authorities
    /// set change, then decreasing to zero on each new block hook. When equals
    /// to zero, reset is performed.
    #[pallet::storage]
    pub(crate) type ClearTimer<T> = StorageValue<_, u32>;

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
    pub(crate) type QueueChanged<T> = StorageValue<_, bool, ValueQuery>;

    /// Operational storage.
    ///
    /// Defines the amount of fee to be paid for the transport of messages.
    #[pallet::storage]
    pub type TransportFee<T> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Pallet Gear Eth Bridge's itself.
    #[pallet::pallet]
    #[pallet::storage_version(ETH_BRIDGE_STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
        /// Root extrinsic that pauses pallet.
        /// When paused, no new messages could be queued.
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::pause())]
        pub fn pause(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            // Ensuring called by `ControlOrigin` or root.
            T::ControlOrigin::ensure_origin_or_root(origin)?;

            // Ensuring that pallet is initialized.
            ensure!(
                Initialized::<T>::get(),
                Error::<T>::BridgeIsNotYetInitialized
            );

            // Taking value (so pausing it) with checking if it was unpaused.
            if !Paused::<T>::take() {
                // Depositing event about bridge being paused.
                Self::deposit_event(Event::<T>::BridgePaused);
            }

            // Returning successful result without weight refund.
            Ok(().into())
        }

        /// Root extrinsic that unpauses pallet.
        /// When paused, no new messages could be queued.
        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::unpause())]
        pub fn unpause(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            // Ensuring called by `ControlOrigin` or root.
            T::ControlOrigin::ensure_origin_or_root(origin)?;

            // Ensuring that pallet is initialized.
            ensure!(
                Initialized::<T>::get(),
                Error::<T>::BridgeIsNotYetInitialized
            );

            // Checking if pallet is paused.
            if Paused::<T>::get() {
                // Unpausing pallet.
                Paused::<T>::put(false);

                // Depositing event about bridge being unpaused.
                Self::deposit_event(Event::<T>::BridgeUnpaused);
            }

            // Returning successful result without weight refund.
            Ok(().into())
        }

        /// Extrinsic that inserts message in a bridging queue,
        /// updating queue merkle root at the end of the block.
        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::send_eth_message())]
        pub fn send_eth_message(
            origin: OriginFor<T>,
            destination: H160,
            payload: Vec<u8>,
        ) -> DispatchResultWithPostInfo
        where
            T::AccountId: Origin,
        {
            let source: ActorId = ensure_signed(origin.clone())?.cast();
            let is_governance_origin = T::ControlOrigin::ensure_origin(origin).is_ok();

            // Transfer fee or skip it if it's zero or governance origin.
            let fee = TransportFee::<T>::get();
            if !(fee.is_zero() || is_governance_origin) {
                let builtin_id = T::BuiltinAddress::get();
                CurrencyOf::<T>::transfer(
                    &source.cast(),
                    &builtin_id,
                    fee,
                    ExistenceRequirement::AllowDeath,
                )?;
            }

            Self::queue_message(source, destination, payload, is_governance_origin)?;

            Ok(().into())
        }

        /// Root extrinsic that sets fee for the transport of messages.
        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::set_fee())]
        pub fn set_fee(origin: OriginFor<T>, fee: BalanceOf<T>) -> DispatchResultWithPostInfo {
            // Ensuring called by `AdminOrigin` or root.
            T::AdminOrigin::ensure_origin_or_root(origin)?;

            // Setting the fee.
            TransportFee::<T>::put(fee);

            // Returning successful result without weight refund.
            Ok(().into())
        }
    }

    impl<T: Config> Pallet<T> {
        pub(crate) fn queue_message(
            source: ActorId,
            destination: H160,
            payload: Vec<u8>,
            is_governance_origin: bool,
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

            // Appending hash of the message into the queue,
            // checks whether the queue capacity is exceeded,
            // or skips the check when the origin is governance.
            let hash = Queue::<T>::mutate(|v| {
                (is_governance_origin || v.len() < QueueCapacityOf::<T>::get() as usize)
                    .then(|| {
                        let hash = message.hash();
                        v.push(hash);
                        hash
                    })
                    .ok_or(Error::<T>::QueueCapacityExceeded)
            })
            .inspect_err(|_| {
                // In case of error, reverting increase of `MessageNonce` performed
                // in message creation to keep builtin interactions transactional.
                MessageNonce::<T>::mutate_exists(|nonce| {
                    *nonce = nonce.and_then(|inner| {
                        inner.checked_sub(U256::one()).filter(|new| !new.is_zero())
                    });
                });
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
            // Initially consists of one read of `ClearTimer` storage.
            let mut weight = T::DbWeight::get().reads(1);

            // Querying timer and checking its value if some.
            if let Some(timer) = ClearTimer::<T>::get() {
                // Asserting invariant that in case of key existence, it's non-zero.
                debug_assert!(!timer.is_zero());

                // Decreasing timer.
                let new_timer = timer.saturating_sub(1);

                if new_timer.is_zero() {
                    // Removing timer for the next session hook.
                    ClearTimer::<T>::kill();

                    // Removing grandpa set hash from storage.
                    AuthoritySetHash::<T>::kill();

                    // Removing queued messages from storage.
                    Queue::<T>::kill();

                    // Setting zero queue root, keeping invariant of this key existence.
                    QueueMerkleRoot::<T>::put(H256::zero());

                    // Depositing event about clearing the bridge.
                    Self::deposit_event(Event::<T>::BridgeCleared);

                    // Increasing resulting weight by 3 writes of above keys removal.
                    weight = weight.saturating_add(T::DbWeight::get().writes(4));
                } else {
                    // Put back non-zero timer to schedule clearing.
                    ClearTimer::<T>::put(new_timer);

                    // Increasing resulting weight by 1 writes of above keys insertion.
                    weight = weight.saturating_add(T::DbWeight::get().writes(1));
                }
            }

            // Returning weight.
            weight
        }

        fn on_finalize(_bn: BlockNumberFor<T>) {
            // If queue wasn't changed, than nothing to do here.
            if !QueueChanged::<T>::take() {
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

        fn on_genesis_session<'a, I: 'a>(_validators: I) {}

        // TODO: consider support of `Stalled` changes of grandpa (#4113).
        fn on_new_session<'a, I>(changed: bool, _validators: I, queued_validators: I)
        where
            I: 'a + Iterator<Item = (&'a T::AccountId, Self::Key)>,
        {
            // If historically pallet hasn't yet faced `changed = true`,
            // any type of calculations aren't performed.
            if !Initialized::<T>::get() && !changed {
                return;
            }

            // Here starts common processing of properly initialized pallet.
            if changed {
                // Checking invariant.
                //
                // Reset scheduling must be resolved on the first block
                // after session changed.
                debug_assert!(ClearTimer::<T>::get().is_none());

                // First time facing `changed = true`, so from now on, pallet
                // is starting handling grandpa sets and queue.
                if !Initialized::<T>::get() {
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
                } else {
                    // Scheduling reset on next block's init.
                    //
                    // Firstly, it will decrease in the same block, because call of
                    // `on_new_session` hook will be performed earlier in the same
                    // block, because `pallet_session` triggers it in its `on_init`
                    // and has smaller pallet id.
                    ClearTimer::<T>::put(2);
                }

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
                    AuthoritySetHash::<T>::put(grandpa_set_hash);

                    // Depositing event about update in the set.
                    Self::deposit_event(Event::<T>::AuthoritySetHashChanged(grandpa_set_hash));
                }
            }
        }

        fn on_disabled(_validator_index: u32) {}
    }

    /// Prefix alias of the `pallet_gear_eth_bridge::AuthoritySetHash` storage.
    pub struct AuthoritySetHashPrefix<T>(PhantomData<T>);

    impl<T: Config> StorageInstance for AuthoritySetHashPrefix<T> {
        const STORAGE_PREFIX: &'static str =
            <_GeneratedPrefixForStorageAuthoritySetHash<T> as StorageInstance>::STORAGE_PREFIX;

        fn pallet_prefix() -> &'static str {
            <_GeneratedPrefixForStorageAuthoritySetHash<T> as StorageInstance>::pallet_prefix()
        }
    }

    /// Prefix alias of the `pallet_gear_eth_bridge::QueueMerkleRoot` storage.
    pub struct QueueMerkleRootPrefix<T>(PhantomData<T>);

    impl<T: Config> StorageInstance for QueueMerkleRootPrefix<T> {
        const STORAGE_PREFIX: &'static str =
            <_GeneratedPrefixForStorageQueueMerkleRoot<T> as StorageInstance>::STORAGE_PREFIX;

        fn pallet_prefix() -> &'static str {
            <_GeneratedPrefixForStorageQueueMerkleRoot<T> as StorageInstance>::pallet_prefix()
        }
    }
}
