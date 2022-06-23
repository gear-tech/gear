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

use frame_support::{traits::StorageVersion, weights::Weight};

pub mod migration;
pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod offchain;

pub type Authorship<T> = pallet_authorship::Pallet<T>;

/// The current storage version.
const USAGE_STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
    use super::{offchain::PayeeInfo, *};
    use common::{storage::*, GasPrice, Origin, PaymentProvider, ValueTree};
    use core_processor::common::ExecutionErrorReason;
    use frame_support::{
        dispatch::{DispatchError, DispatchResultWithPostInfo},
        pallet_prelude::*,
        traits::{Currency, Get, Imbalance, ReservableCurrency},
    };
    use frame_system::{offchain::SendTransactionTypes, pallet_prelude::*, RawOrigin};
    use gear_core::{
        ids::{MessageId, ProgramId},
        message::{ReplyMessage, ReplyPacket, StoredDispatch, StoredMessage},
    };
    use sp_core::offchain::Duration;
    use sp_runtime::{
        offchain::{
            storage::StorageValueRef,
            storage_lock::{StorageLock, Time},
        },
        traits::{SaturatedConversion, Saturating},
        Perbill,
    };
    use sp_std::{convert::TryInto, prelude::*};
    use pallet_gear::GasHandlerOf;

    pub(crate) type QueueOf<T> = <<T as Config>::Messenger as Messenger>::Queue;
    pub(crate) type MailboxOf<T> = <<T as Config>::Messenger as Messenger>::Mailbox;
    pub(crate) type WaitlistOf<T> = <<T as Config>::Messenger as Messenger>::Waitlist;

    #[pallet::config]
    pub trait Config:
        frame_system::Config
        + pallet_authorship::Config
        + pallet_gear::Config
        + SendTransactionTypes<Call<Self>>
    where
        <Self as frame_system::Config>::AccountId: Origin,
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

        type Messenger: Messenger<
            BlockNumber = Self::BlockNumber,
            QueuedDispatch = StoredDispatch,
            MailboxedMessage = StoredMessage,
            WaitlistFirstKey = ProgramId,
            WaitlistSecondKey = MessageId,
            WaitlistedMessage = StoredDispatch,
        >;
    }

    type BalanceOf<T> = <<T as pallet_gear::Config>::Currency as Currency<
        <T as frame_system::Config>::AccountId,
    >>::Balance;

    #[pallet::pallet]
    #[pallet::storage_version(USAGE_STORAGE_VERSION)]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config>
    where
        T::AccountId: Origin,
    {
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
    impl<T: Config> ValidateUnsigned for Pallet<T>
    where
        T::AccountId: Origin,
    {
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
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    where
        T::AccountId: Origin,
    {
        /// Offchain worker
        ///
        /// Scans the wait list portion by portion and sends a transaction back on-chain
        /// to charge messages' authors for "renting" a slot in the list.
        /// Maintains a minimum interval between full scans, idling in between if necessary
        fn offchain_worker(now: BlockNumberFor<T>) {
            // Only do something if we are a validator
            if !sp_io::offchain::is_validator() {
                log::debug!("Skipping offchain worker at {:?}: not a validator.", now,);
                return;
            }

            // Ensure we maintain minimum interval between full wait list traversals
            let current_round_storage_ref =
                StorageValueRef::persistent(offchain::STORAGE_ROUND_STARTED_AT);
            let current_round_started_at =
                match current_round_storage_ref.get::<BlockNumberFor<T>>() {
                    Ok(maybe_round_started_at) => maybe_round_started_at.unwrap_or_default(),
                    _ => {
                        log::debug!("Failed to get a value from storage at block {:?}", now,);
                        return;
                    }
                };
            let (_, last_key) = match offchain::get_last_key_from_offchain_storage() {
                Ok(x) => x,
                _ => {
                    log::debug!("Failed to get a value from storage at block {:?}", now,);
                    return;
                }
            };

            if now.saturating_sub(current_round_started_at) < T::WaitListTraversalInterval::get()
                && last_key.is_none()
            {
                // We have either finished the previous round or never started one, and the number of
                // elapsed blocks since last traversal is less than the expected minimum interval
                log::debug!(
                    "Block {:?} offchain worker. Not starting next wait list traversal until block {:?}",
                    now,
                    current_round_started_at.saturating_add(T::WaitListTraversalInterval::get()),
                );
                return;
            }

            if last_key.is_none() {
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
                log::debug!(
                    target: "essential",
                    "Error in offchain worker at {:?}: {:?}", now, e,
                )
            }
        }
    }

    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
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
                         WaitlistOf::<T>::remove(ProgramId::from_origin(program_id), MessageId::from_origin(message_id)).ok().and_then(|(dispatch, bn)| {
                            let duration: u32 = current_block.saturating_sub(bn).saturated_into::<u32>();
                            let chargeable_amount =
                                <T as pallet_gear::Config>::WaitListFeePerBlock::get().saturating_mul(duration.into());

                            match GasHandlerOf::<T>::get_limit(dispatch.id().into_origin()) {
                                Ok(maybe_limit) => {
                                    match maybe_limit {
                                        Some(msg_gas_balance) => {
                                            let usable_gas = msg_gas_balance
                                                .saturating_sub(T::TrapReplyExistentialGasLimit::get());

                                            let new_free_gas = usable_gas.saturating_sub(chargeable_amount);

                                            let actual_fee = usable_gas.saturating_sub(new_free_gas);
                                            Some((actual_fee, dispatch, msg_gas_balance))
                                        },
                                        _ => {
                                            log::debug!(
                                                target: "essential",
                                                "Message in wait list doesn't have associated gas - can't charge rent",
                                            );
                                            None
                                        }
                                    }
                                },
                                Err(_err) => {
                                    // This can only be due to invalid gas tree
                                    // TODO: handle appropriately
                                    unreachable!("Can never happen unless gas tree corrupted");
                                }
                            }
                        })
                    },
                )
                .for_each(|(fee, dispatch, msg_gas_balance)| {
                    let msg_id = dispatch.id();
                    if let Err(e) = GasHandlerOf::<T>::spend(msg_id.into_origin(), fee) {
                        log::debug!(
                            target: "essential",
                            "Error spending {:?} gas from {:?}: {:?}",
                            fee, msg_id, e
                        );
                        return;
                    };
                    let total_reward = T::GasPrice::gas_price(fee);
                    let origin = match GasHandlerOf::<T>::get_origin(msg_id.into_origin()) {
                        Ok(maybe_origin) => {
                            // NOTE: intentional expect.
                            // Given the gas tree is valid, the node with this id is guaranteed to have an origin
                            maybe_origin
                                .expect("Gas node is guaranteed to exist for the key due to earlier checks")
                        },
                        Err(_e) => {
                            // Can only be due to invalid gas tree
                            unreachable!("Can never happen unless gas tree corrupted");
                        }
                    };

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
                                log::debug!(
                                    target: "essential",
                                    "Failed to repatriate reserved amount: {:?}",
                                    e,
                                );
                            }
                            if let Some(author) = Authorship::<T>::author() {
                                if let Err(e) = T::PaymentProvider::withhold_reserved(
                                    origin,
                                    &author,
                                    validator_reward,
                                ) {
                                    log::debug!(
                                        target: "essential",
                                        "Failed to repatriate reserved amount: {:?}",
                                        e,
                                    );
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
                                    log::debug!(
                                        target: "essential",
                                        "Failed to repatriate reserved amount: {:?}",
                                        e,
                                    );
                                }
                            }
                        }
                    };

                    let program_id = dispatch.destination();
                    let new_msg_gas_balance = msg_gas_balance.saturating_sub(fee);
                    if new_msg_gas_balance <= T::TrapReplyExistentialGasLimit::get() {
                        if common::get_program(program_id.into_origin()).is_some() {
                            // TODO: generate system signal for program (#647)

                            // Generate trap reply
                            let trap_message_id = MessageId::generate_reply(msg_id, core_processor::ERR_EXIT_CODE);
                            let packet = ReplyPacket::system(ExecutionErrorReason::OutOfRent.encode(), core_processor::ERR_EXIT_CODE);
                            let message = ReplyMessage::from_packet(trap_message_id, packet);
                            let dispatch = message.into_stored_dispatch(program_id, dispatch.source(), msg_id);

                            if pallet_gear_program::Pallet::<T>::program_exists(dispatch.destination()) {
                                // Enqueue the trap reply message
                                if let Err(e) = GasHandlerOf::<T>::split(
                                    msg_id.into_origin(),
                                    trap_message_id.into_origin(),
                                ) {
                                    log::debug!(
                                        target: "essential",
                                        "Failed to create value node for trap reply message: {:?}",
                                        e,
                                    );
                                }

                                QueueOf::<T>::queue(dispatch)
                                    .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
                            } else {
                                let message = match dispatch.exit_code() {
                                    Some(0) | None => dispatch.into_parts().1,
                                    _ => {
                                        let message = dispatch.into_parts().1;
                                        message
                                            .clone()
                                            .with_string_payload::<ExecutionErrorReason>()
                                            .unwrap_or(message)
                                    }
                                };

                                if MailboxOf::<T>::insert(message).is_err() {
                                    // TODO: update logic of insertion into mailbox following new
                                    // flow and deposit appropriate event (issue #1010).

                                    // TODO: deposit appropriate (Gear) event,
                                    // instead of silent insertion.
                                    log::debug!("Duplicate mailbox message");
                                }
                            }

                            // Consume the corresponding node
                            match GasHandlerOf::<T>::consume(msg_id.into_origin()) {
                                Err(e) => {
                                    // We only can get an error here if the gas tree is invalidated
                                    // TODO: throwing a panic is not appropriate here; decide, what to do
                                    log::debug!(
                                        target: "essential",
                                        "Gas tree invalidated: {:?}",
                                        e,
                                    );
                                }
                                Ok(maybe_outcome) => {
                                    if let Some((neg_imbalance, external)) = maybe_outcome {
                                        let gas_left = neg_imbalance.peek();
                                        log::debug!("Unreserve balance on message processed: {}", gas_left);

                                        let refund = T::GasPrice::gas_price(gas_left);

                                        let _ = <T as pallet_gear::Config>::Currency::unreserve(
                                            &<T::AccountId as Origin>::from_origin(external),
                                            refund,
                                        );
                                    }
                                }
                            }
                        } else {
                            // Wait init messages can't reach that, because if program init failed,
                            // then all waiting messages are moved to queue deleted.
                            log::debug!(
                                target: "essential",
                                "Program {:?} isn't in storage, but message with that dest is in WL",
                                program_id,
                            )
                        }
                    } // Message still got enough gas limit and may keep waiting.
                      // Updating gas limit value and re-inserting the message into wait list.
                    else if WaitlistOf::<T>::insert(dispatch).is_err() {
                        log::error!("Failed to insert dispatch into waitlist");
                    }


                    total_collected += 1;
                });
            total_collected
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
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

            let total_collected = Self::do_rent_collection(payees_list.clone(), who.as_ref());

            log::debug!("Collected {} from {:?}", total_collected, payees_list);

            if total_collected > 0 {
                Self::deposit_event(Event::WaitListRentCollected(total_collected));
            }

            Ok(().into())
        }
    }
}
