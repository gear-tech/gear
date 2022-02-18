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

use common::DAGBasedLedger;
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
    use common::{Dispatch, GasPrice, Message, Origin, PaymentProvider, Program};
    use frame_support::{
        dispatch::{DispatchError, DispatchResultWithPostInfo},
        pallet_prelude::*,
        traits::{Currency, Get},
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
        frame_system::Config
        + pallet_authorship::Config
        + pallet_gear::Config
        + SendTransactionTypes<Call<Self>>
    {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

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

    type BalanceOf<T> = <<T as pallet_gear::Config>::Currency as Currency<
        <T as frame_system::Config>::AccountId,
    >>::Balance;

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        WaitListRentCollected(u32),
    }

    // Usage pallet error.
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
                        common::remove_waiting_message(program_id, message_id).and_then(|(dispatch, bn)| {
                            let duration = current_block.saturated_into::<u32>().saturating_sub(bn);
                            let chargeable_amount =
                                T::WaitListFeePerBlock::get().saturating_mul(duration.into());

                            match <T as pallet_gear::Config>::GasHandler::get(dispatch.message.id) {
                                Some((msg_gas_balance, origin)) => {
                                    let usable_gas = msg_gas_balance
                                        .saturating_sub(T::TrapReplyExistentialGasLimit::get());

                                    let new_free_gas = usable_gas.saturating_sub(chargeable_amount);

                                    let actual_fee = usable_gas.saturating_sub(new_free_gas);
                                    Some((actual_fee, origin, dispatch, msg_gas_balance))
                                },
                                _ => {
                                    log::warn!(
                                        "Message in wait list doesn't have associated gas - can't charge rent"
                                    );
                                    None
                                }
                            }
                        })
                    },
                )
                .for_each(|(fee, origin, mut dispatch, msg_gas_balance)| {
                    let msg_id = dispatch.message.id;
                    if let Err(e) = <T as pallet_gear::Config>::GasHandler::spend(msg_id, fee) {
                        log::error!(
                            "Error spending {:?} gas from {:?}: {:?}",
                            fee, msg_id, e
                        );
                        return;
                    };
                    let total_reward = T::GasPrice::gas_price(fee);

                    // Counter-balance the created imbalance with a value transfer
                    match external_account {
                        Some(who) => {
                            let user_reward =
                                T::ExternalSubmitterRewardFraction::get() * total_reward;
                            let validator_reward = total_reward.saturating_sub(user_reward);
                            if let Err(e) = T::PaymentProvider::withhold_reserved(
                                origin,
                                who,
                                user_reward,
                            ) {
                                log::warn!("Failed to repatriate reserved amount: {:?}", e);
                            }
                            if let Some(author) = Authorship::<T>::author() {
                                if let Err(e) = T::PaymentProvider::withhold_reserved(
                                    origin,
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
                                    origin,
                                    &author,
                                    total_reward,
                                ) {
                                    log::warn!("Failed to repatriate reserved amount: {:?}", e);
                                }
                            }
                        }
                    };

                    let program_id = dispatch.message.dest;
                    let new_msg_gas_balance = msg_gas_balance.saturating_sub(fee);
                    if new_msg_gas_balance <= T::TrapReplyExistentialGasLimit::get() {
                        match common::get_program(program_id) {
                            Some(Program::Active(mut program)) => {
                                // Generate trap reply

                                program.nonce += 1;

                                let trap_message_id = core_processor::next_message_id(
                                    program_id.as_ref().into(),
                                    program.nonce,
                                ).into_origin();
                                let trap_message = Message {
                                    id: trap_message_id,
                                    source: program_id,
                                    dest: dispatch.message.source,
                                    payload: vec![],
                                    gas_limit: new_msg_gas_balance,
                                    value: 0,
                                    reply: Some((msg_id, core_processor::ERR_EXIT_CODE)),
                                };

                                let reply_dispatch = Dispatch::new_reply(trap_message);

                                // Enqueue the trap reply message
                                let _ = <T as pallet_gear::Config>::GasHandler::split(
                                    msg_id,
                                    trap_message_id,
                                    new_msg_gas_balance
                                );
                                common::queue_dispatch(reply_dispatch);

                                // Save back the program with incremented nonce
                                common::set_program(program_id, program, Default::default());
                            }
                            _ => {
                                /// Wait init messages can't reach that, because if program init failed,
                                /// then all waiting messages are moved to queue deleted.
                                /// TODO #507 on each program delete/terminate action remove messages from WL
                                log::error!(
                                    "Program {:?} was killed, but message it generated is in WL",
                                    program_id
                                )
                            }
                        }
                    } else {
                        // Message still got enough gas limit and may keep waiting.
                        // Updating gas limit value and re-inserting the message into wait list.
                        dispatch.message.gas_limit = new_msg_gas_balance;
                        common::insert_waiting_message(
                            program_id,
                            msg_id,
                            dispatch,
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
