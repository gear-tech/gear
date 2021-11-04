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

pub const WAITLIST_FEE_PER_BLOCK: u64 = 100;

#[frame_support::pallet]
pub mod pallet {
    use super::offchain::PayeeInfo;
    use super::*;
    use common::{
        value_tree::ValueView, GasToFeeConverter, Message, PaymentProvider, GAS_VALUE_PREFIX,
    };
    use frame_support::{
        dispatch::{DispatchError, DispatchResultWithPostInfo},
        pallet_prelude::*,
        traits::{Currency, Get, ReservableCurrency},
    };
    use frame_system::{offchain::SendTransactionTypes, pallet_prelude::*, RawOrigin};
    use primitive_types::H256;
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

        /// Type providing interface to make payment in currency units
        type PaymentProvider: PaymentProvider<Self::AccountId, Balance = BalanceOf<Self>>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;

        /// The desired interval between offchain worker invocations.
        #[pallet::constant]
        type WaitListTraversalInterval: Get<Self::BlockNumber>;

        /// Time lock expiration duration for an offchain worker
        #[pallet::constant]
        type ExpirationDuration: Get<u64>;

        /// The desired interval between offchain worker invocations.
        #[pallet::constant]
        type MaxBatchSize: Get<u32>;

        /// The amount of gas necessary for a trap reply message to be processed.
        #[pallet::constant]
        type TrapReplyExistentialGasLimit: Get<u64>;

        /// The fraction of the collected wait list rent an external submitter will get as a reward
        #[pallet::constant]
        type ExternalSubmitterRewardFraction: Get<Perbill>;
    }

    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        WaitListRentCollected,
    }

    // Gear pallet error.
    #[pallet::error]
    pub enum Error<T> {
        /// Value not found for a key in storage.
        FailedToGetValueFromStorage,
    }

    /// Methods for the `ValidateUnsigned` implementation:
    /// Restricts calls to `collect_waitlist_rent_unsigned` to local calls
    /// (i.e. extrinsics generated on this node) or those already in a block
    /// therefore ruling out any origin other than block authors.
    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;
        fn validate_unsigned(source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            match call {
                Call::collect_waitlist_rent { payees_list: _ } => {
                    // Only accept transactions from a trusted source: local OCW or those already in block.
                    if !matches!(
                        source,
                        TransactionSource::Local | TransactionSource::InBlock
                    ) {
                        return InvalidTransaction::Call.into();
                    }

                    // TODO: ensure necessary validity checks hold

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
        /// Makes necessary actions to maintain data needed for charging programs
        /// for keeping messages in the wait list
        fn offchain_worker(now: BlockNumberFor<T>) {
            // Only do something if we are a potential validator.
            if !sp_io::offchain::is_validator() {
                log::debug!(
                    target: "gear-support",
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
                            target: "gear-support",
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
                        target: "gear-support",
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
                    target: "gear-support",
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
                    target: "gear-support",
                    "Error in offchain worker at {:?}: {:?}", now, e,
                )
            }
        }
    }

    impl<T: Config> Pallet<T> {
        fn do_rent_collection(
            payees_list: Vec<PayeeInfo>,
            external_account: Option<&T::AccountId>,
        ) {
            let current_block = <frame_system::Pallet<T>>::block_number();
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
                .for_each(|(msg, bn)| {
                    let program_id = msg.dest;

                    let mut gas_tree = ValueView::get(GAS_VALUE_PREFIX, msg.id)
                        .expect("A message in wait list must have an associated value tree");
                    let duration = current_block.saturated_into::<u32>().saturating_sub(bn);
                    let full_fee: u64 = (duration as u64).saturating_mul(WAITLIST_FEE_PER_BLOCK);

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
                            let _ = T::PaymentProvider::withhold_reserved(
                                gas_tree.origin(),
                                who,
                                user_reward,
                            );
                            let _ = T::PaymentProvider::withhold_reserved(
                                gas_tree.origin(),
                                &Authorship::<T>::author(),
                                validator_reward,
                            );
                        }
                        _ => {
                            let _ = T::PaymentProvider::withhold_reserved(
                                gas_tree.origin(),
                                &Authorship::<T>::author(),
                                T::GasConverter::gas_to_fee(actual_fee),
                            );
                        }
                    };

                    if new_free_gas_limit == 0 {
                        // Generate trap reply

                        // Account for the fact that original gas balance of the message could have
                        // already been lower than the required "existential" gas limit
                        let trap_gas = actual_gas_limit.min(T::TrapReplyExistentialGasLimit::get());

                        let mut program = common::get_program(program_id).expect(
                            "There is always an associated program for a message in wait list",
                        );
                        program.nonce += 1;

                        let trap_message_id =
                            runner::generate_message_id(program_id.as_ref().into(), program.nonce);
                        let trap_message = Message {
                            id: H256::from_slice(trap_message_id.as_slice()),
                            source: program_id,
                            dest: msg.source,
                            payload: vec![],
                            gas_limit: trap_gas,
                            value: 0,
                            reply: Some((msg.id, runner::EXIT_CODE_PANIC)),
                        };

                        // Enqueue the trap reply message
                        gas_tree.split_off(trap_message.id, trap_gas);
                        common::queue_message(trap_message);

                        // Save back the program with incremented nonce
                        common::set_program(program_id, program, Default::default());
                    } else {
                        common::insert_waiting_message(
                            program_id,
                            msg.id,
                            msg,
                            current_block.saturated_into(),
                        );
                    }
                });
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

            log::debug!(
                target: "runtime::gear",
                "[collect_waitlist_rent_unsigned] payload: {:?}",
                payees_list,
            );

            let _charged = Self::do_rent_collection(payees_list, who.as_ref());

            Self::deposit_event(Event::WaitListRentCollected);

            Ok(().into())
        }
    }
}
