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

mod offchain;

pub type Authorship<T> = pallet_authorship::Pallet<T>;

#[frame_support::pallet]
pub mod pallet {
    use super::offchain::PayeeInfo;
    use super::*;
    use common::{
        value_tree::ValueView, Dispatch, GasToFeeConverter, Message, Origin, PaymentProvider,
        Program, GAS_VALUE_PREFIX,
    };
    use frame_support::{
        dispatch::{DispatchError, DispatchResultWithPostInfo},
        pallet_prelude::*,
        traits::{Currency, Get, ReservableCurrency},
    };
    use frame_system::{offchain::SendTransactionTypes, pallet_prelude::*, RawOrigin};
    use sp_core::offchain::Duration;
    use sp_runtime::{
        offchain::{
            storage::StorageValueRef,
            storage_lock::{StorageLock, Time},
        },
        traits::{SaturatedConversion, Saturating},
        Perbill,
    };
    use sp_std::prelude::*;

    #[pallet::config]
    pub trait Config:
        frame_system::Config + pallet_authorship::Config + SendTransactionTypes<Call<Self>>
    {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        /// Gas and value transfer currency
        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

        /// Gas to Currency converter
        type GasConverter: GasToFeeConverter<Balance = BalanceOf<Self>>;

        /// Type providing interface for making payment in currency units
        type PaymentProvider: PaymentProvider<Self::AccountId, Balance = BalanceOf<Self>>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;

        /// The desired interval between offchain worker invocations.
        #[pallet::constant]
        type WaitListTraversalInterval: Get<Self::BlockNumber>;

        /// Time lock expiration duration for an offchain worker
        #[pallet::constant]
        type ExpirationDuration: Get<u64>;

        /// The maximum number of waitlisted messages to be processed on-chain in one go.
        #[pallet::constant]
        type MaxBatchSize: Get<u32>;

        /// The amount of gas necessary for a trap reply message to be processed.
        #[pallet::constant]
        type TrapReplyExistentialGasLimit: Get<u64>;

        /// The fraction of the collected wait list rent an external submitter will get as a reward
        #[pallet::constant]
        type ExternalSubmitterRewardFraction: Get<Perbill>;

        /// The cost for a message to spend one block in the wait list
        #[pallet::constant]
        type WaitListFeePerBlock: Get<u64>;
    }

    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        WaitListRentCollected(u32),
    }

    // Gear pallet error.
    #[pallet::error]
    pub enum Error<T> {
        /// Value not found for a key in storage.
        FailedToGetValueFromStorage,
    }

    /// Accepting the unsigned `collect_waitlist_rent` extrinsic either if it originated on the
    /// the local node or if it has already been included in a block.
    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;
        fn validate_unsigned(source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            match call {
                Call::collect_waitlist_rent { payees_list } => {
                    // Only accept transactions from a trusted source
                    if !matches!(
                        source,
                        TransactionSource::Local | TransactionSource::InBlock
                    ) {
                        return InvalidTransaction::Call.into();
                    }

                    // Check the payload size (a precaution against a malicious validator)
                    if payees_list.len() > T::MaxBatchSize::get() as usize {
                        return InvalidTransaction::ExhaustsResources.into();
                    }

                    // TODO: apply other necessary validity checks
                    // https://github.com/gear-tech/gear/issues/506

                    let current_block = <frame_system::Pallet<T>>::block_number();
                    ValidTransaction::with_tag_prefix("gear")
                        .priority(TransactionPriority::max_value())
                        .and_provides(current_block)
                        .longevity(T::WaitListTraversalInterval::get().saturated_into::<u64>())
                        .propagate(true)
                        .build()
                }
                _ => InvalidTransaction::Call.into(),
            }
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// Initialization
        fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
            0_u64
        }

        /// Finalization
        fn on_finalize(_bn: BlockNumberFor<T>) {}

        /// Offchain worker
        ///
        /// Scans the wait list portion by portion and sends a transaction back on-chain
        /// to charge messages' authors for "renting" a slot in the list.
        /// Maintains a minimum interval between full scans, idling in between if necessary
        fn offchain_worker(now: BlockNumberFor<T>) {
            // Only do something if we are a validator
            if !sp_io::offchain::is_validator() {
                log::debug!(
                    target: "runtime::usage",
                    "Skipping offchain worker at {:?}: not a validator.",
                    now,
                );
                return;
            }

            // Ensure we maintain minimum interval between full wait list traversals
            let current_round_storage_ref =
                StorageValueRef::persistent(offchain::STORAGE_ROUND_STARTED_AT);
            let current_round_started_at =
                match current_round_storage_ref.get::<BlockNumberFor<T>>() {
                    Ok(maybe_round_started_at) => maybe_round_started_at.unwrap_or_default(),
                    _ => {
                        log::debug!(
                            target: "runtime::usage",
                            "Failed to get a value from storage at block {:?}",
                            now,
                        );
                        return;
                    }
                };
            let (_, last_key) = match offchain::get_last_key_from_offchain_storage() {
                Ok(x) => x,
                _ => {
                    log::debug!(
                        target: "runtime::usage",
                        "Failed to get a value from storage at block {:?}",
                        now,
                    );
                    return;
                }
            };

            if now.saturating_sub(current_round_started_at) < T::WaitListTraversalInterval::get()
                && &last_key[..] == common::STORAGE_WAITLIST_PREFIX
            {
                // We have either finished the previous round or never started one, and the number of
                // elapsed blocks since last traversal is less than the expected minimum interval
                log::debug!(
                    target: "runtime::usage",
                    "Block {:?} offchain worker. Not starting next wait list traversal until block {:?}",
                    now,
                    current_round_started_at.saturating_add(T::WaitListTraversalInterval::get()),
                );
                return;
            }

            if &last_key[..] == common::STORAGE_WAITLIST_PREFIX {
                // Starting a new round
                current_round_storage_ref.set(&now);
            }

            // Acquire the lock protecting shared offchain workers' storage
            let mut lock = StorageLock::<'_, Time>::with_deadline(
                offchain::STORAGE_OCW_LOCK,
                Duration::from_millis(T::ExpirationDuration::get()),
            );
            let _guard = lock.lock();

            let res = Self::waitlist_usage(now);
            if let Err(e) = res {
                log::error!(
                    target: "runtime::usage",
                    "Error in offchain worker at {:?}: {:?}", now, e,
                )
            }
        }
    }

    impl<T: Config> Pallet<T> {
        fn do_rent_collection(
            payees_list: Vec<PayeeInfo>,
            external_account: Option<&T::AccountId>,
        ) -> u32 {
            let current_block = <frame_system::Pallet<T>>::block_number();
            let mut total_collected = 0;
            payees_list
                .into_iter()
                .filter_map(
                    |PayeeInfo {
                         program_id,
                         message_id,
                     }| {
                        common::remove_waiting_message(program_id, message_id)
                    },
                )
                .for_each(|(dispatch, bn)| {
                    let Dispatch { message, kind } = dispatch;
                    let program_id = message.dest;

                    let mut gas_tree = ValueView::get(GAS_VALUE_PREFIX, message.id)
                        .expect("A message in wait list must have an associated value tree");
                    let duration = current_block.saturated_into::<u32>().saturating_sub(bn);
                    let full_fee = T::WaitListFeePerBlock::get().saturating_mul(duration.into());

                    // Taking the amount locked in the respective value tree as the ground truth
                    // of the amount of gas a message has at its disposal to account for and correct
                    // potential discrepancy between this value and the message `gas_limit` field.
                    let actual_gas_limit = gas_tree.value();
                    let free_gas_limit =
                        actual_gas_limit.saturating_sub(T::TrapReplyExistentialGasLimit::get());

                    let new_free_gas_limit = free_gas_limit.saturating_sub(full_fee);

                    let actual_fee = free_gas_limit.saturating_sub(new_free_gas_limit);

                    gas_tree.spend(actual_fee);

                    // Make actual payment
                    match external_account {
                        Some(who) => {
                            let total_reward = T::GasConverter::gas_to_fee(actual_fee);
                            let user_reward =
                                T::ExternalSubmitterRewardFraction::get() * total_reward;
                            let validator_reward = total_reward.saturating_sub(user_reward);
                            if let Err(e) = T::PaymentProvider::withhold_reserved(
                                gas_tree.origin(),
                                who,
                                user_reward,
                            ) {
                                log::warn!("Failed to repatriate reserved amount: {:?}", e);
                            }
                            if let Some(author) = Authorship::<T>::author() {
                                if let Err(e) = T::PaymentProvider::withhold_reserved(
                                    gas_tree.origin(),
                                    &author,
                                    validator_reward,
                                ) {
                                    log::warn!("Failed to repatriate reserved amount: {:?}", e);
                                }
                            }
                        }
                        _ => {
                            if let Some(author) = Authorship::<T>::author() {
                                if let Err(e) = T::PaymentProvider::withhold_reserved(
                                    gas_tree.origin(),
                                    &author,
                                    T::GasConverter::gas_to_fee(actual_fee),
                                ) {
                                    log::warn!("Failed to repatriate reserved amount: {:?}", e);
                                }
                            }
                        }
                    };

                    if new_free_gas_limit == 0 {
                        match common::get_program(program_id) {
                            Some(Program::Active(mut program)) => {
                                // Generate trap reply

                                // Account for the fact that original gas balance of the message could have
                                // already been lower than the required "existential" gas limit
                                let trap_gas =
                                    actual_gas_limit.min(T::TrapReplyExistentialGasLimit::get());

                                program.nonce += 1;

                                let trap_message_id = core_processor::next_message_id(
                                    program_id.as_ref().into(),
                                    program.nonce,
                                ).into_origin();
                                let trap_message = Message {
                                    id: trap_message_id,
                                    source: program_id,
                                    dest: message.source,
                                    payload: vec![],
                                    gas_limit: trap_gas,
                                    value: 0,
                                    reply: Some((message.id, core_processor::ERR_EXIT_CODE)),
                                };

                                let dispatch = Dispatch::new_reply(trap_message);

                                // Enqueue the trap reply message
                                let _ = gas_tree.split_off(trap_message_id, trap_gas);
                                common::queue_dispatch(dispatch);

                                // Save back the program with incremented nonce
                                common::set_program(program_id, program, Default::default());
                            }
                            _ => {
                                unreachable!(
                                    "program with {:?} id was terminated and messages to it were remove from WL",
                                    program_id
                                )
                            }
                        }
                        // "There is always an associated program for a message in wait list",
                    } else {
                        common::insert_waiting_message(
                            program_id,
                            message.id,
                            Dispatch {
                                message,
                                kind
                            },
                            current_block.saturated_into(),
                        );
                    }

                    total_collected += 1;
                });
            total_collected
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Collect rent payment for keeping messages in the wait list.
        ///
        /// This extrinsic can be both signed and unsigned:
        /// - the former one can only be submitted locally by the block author,
        /// - the latter can come from any legitimate external user.
        #[pallet::weight(<T as Config>::WeightInfo::collect_waitlist_rent(payees_list.len() as u32))]
        pub fn collect_waitlist_rent(
            origin: OriginFor<T>,
            payees_list: Vec<PayeeInfo>,
        ) -> DispatchResultWithPostInfo {
            let who = match origin.into() {
                Ok(RawOrigin::Signed(t)) => Ok(Some(t)),
                Ok(RawOrigin::None) => Ok(None),
                _ => Err(DispatchError::BadOrigin),
            }?;

            let total_collected = Self::do_rent_collection(payees_list, who.as_ref());

            if total_collected > 0 {
                Self::deposit_event(Event::WaitListRentCollected(total_collected));
            }

            Ok(().into())
        }
    }
}
