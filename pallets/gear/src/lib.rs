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

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://substrate.dev/docs/en/knowledgebase/runtime/frame>
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
    use common::{self, IntermediateMessage, Message, Origin};
    use frame_support::inherent::{InherentData, InherentIdentifier};
    use frame_support::{
        dispatch::DispatchResultWithPostInfo,
        pallet_prelude::*,
        traits::Randomness,
        traits::{BalanceStatus, Currency, ExistenceRequirement, ReservableCurrency},
        weights::{IdentityFee, WeightToFeePolynomial},
    };
    use frame_system::pallet_prelude::*;
    use sp_core::H256;
    use sp_std::prelude::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        /// Gas and value transfer currency
        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

        #[pallet::constant]
        type SubmitWeightPerByte: Get<u64>;

        #[pallet::constant]
        type MessagePerByte: Get<u64>;

        type RandomnessSource: Randomness<H256, Self::BlockNumber>;
    }

    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::metadata(T::AccountId = "AccountId")]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Log event from the specific program.
        Log(common::Message),
        /// Program created in the network.
        NewProgram(H256),
        /// Program initialization error.
        InitFailure(H256, MessageError),
        /// Program initialized.
        ProgramInitialized(H256),
        /// Some number of messages processed.
        MessagesDequeued(u32),
        /// Message dispatch resulted in error
        MessageNotProcessed(MessageError),
    }

    // Gear pallet error.
    #[pallet::error]
    pub enum Error<T> {
        /// Not enough balance to reserve.
        ///
        /// Usually occurs when gas_limit specified is such that origin account can't afford the message.
        NotEnoughBalanceForReserve,
    }

    #[derive(Debug, Encode, Decode, Clone, PartialEq)]
    pub enum MessageError {
        ValueTransfer,
        Dispatch,
    }

    #[pallet::storage]
    #[pallet::getter(fn message_queue)]
    pub type MessageQueue<T> = StorageValue<_, Vec<IntermediateMessage>>;

    #[pallet::storage]
    #[pallet::getter(fn dequeue_limit)]
    pub type DequeueLimit<T> = StorageValue<_, u32>;

    #[pallet::storage]
    #[pallet::getter(fn messages_processed)]
    pub type MessagesProcessed<T> = StorageValue<_, u32, ValueQuery>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// Initialization
        fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
            0
        }

        /// Finalization
        fn on_finalize(_bn: BlockNumberFor<T>) {}
    }

    fn gas_to_fee<T: Config>(gas: u64) -> BalanceOf<T>
    where
        <T::Currency as Currency<T::AccountId>>::Balance: Into<u128> + From<u128>,
    {
        IdentityFee::<BalanceOf<T>>::calc(&gas)
    }

    fn block_author<T: Config + pallet_authorship::Config>() -> T::AccountId {
        <pallet_authorship::Pallet<T>>::author()
    }

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
        T: pallet_authorship::Config,
        <T::Currency as Currency<T::AccountId>>::Balance: Into<u128> + From<u128>,
    {
        #[pallet::weight(
			T::DbWeight::get().writes(4) +
			T::SubmitWeightPerByte::get()*(code.len() as u64) +
			T::MessagePerByte::get()*(init_payload.len() as u64)
		)]
        pub fn submit_program(
            origin: OriginFor<T>,
            code: Vec<u8>,
            salt: Vec<u8>,
            init_payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            let reserve_fee = gas_to_fee::<T>(gas_limit);

            // First we reserve enough funds on the account to pay for 'gas_limit'
            // and to transfer declared value.
            T::Currency::reserve(&who, reserve_fee + value)
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            let mut data = Vec::new();
            code.encode_to(&mut data);
            salt.encode_to(&mut data);

            let id: H256 = sp_io::hashing::blake2_256(&data[..]).into();

            <MessageQueue<T>>::mutate(|messages| {
                let mut actual_messages = messages.take().unwrap_or_default();
                actual_messages.push(IntermediateMessage::InitProgram {
                    origin: who.into_origin(),
                    code,
                    program_id: id,
                    payload: init_payload,
                    gas_limit,
                    value: value.into(),
                });

                *messages = Some(actual_messages);
            });

            Self::deposit_event(Event::NewProgram(id));

            Ok(().into())
        }

        #[pallet::weight(
			T::DbWeight::get().writes(4) +
			*gas_limit +
			T::MessagePerByte::get()*(payload.len() as u64)
		)]
        pub fn send_message(
            origin: OriginFor<T>,
            destination: H256,
            payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            let gas_limit_reserve = gas_to_fee::<T>(gas_limit);

            // First we reserve enough funds on the account to pay for 'gas_limit'
            T::Currency::reserve(&who, gas_limit_reserve)
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            // Since messages a guaranteed to be dispatched, we transfer value immediately
            T::Currency::transfer(
                &who,
                &<T::AccountId as Origin>::from_origin(destination),
                value,
                ExistenceRequirement::AllowDeath,
            )?;

            // Only after reservation the message is actually put in the queue.
            <MessageQueue<T>>::mutate(|messages| {
                let mut actual_messages = messages.take().unwrap_or_default();

                let message_id = payload.encode();

                let (message_id, _) = T::RandomnessSource::random(&message_id);

                actual_messages.push(IntermediateMessage::DispatchMessage {
                    id: message_id,
                    origin: who.into_origin(),
                    destination,
                    payload,
                    gas_limit,
                    value: value.into(),
                });

                *messages = Some(actual_messages);
            });

            Ok(().into())
        }

        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub fn process_queue(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;

            // At the beginning of a new block, we process all queued messages
            // TODO: When gas is introduced, processing should be limited to the specific max gas
            // TODO: When memory regions introduced, processing should be limited to the messages that touch
            //       specific pages.

            let messages = <MessageQueue<T>>::take().unwrap_or_default();
            let messages_processed = <MessagesProcessed<T>>::get();

            // `MessagesProcessed` counter should not be checked upfront because all the messages may turn out being the
            // `init_program` variant which does not call `common::queue_message` and, therefore, can still be processed.
            // TODO: consider moving this code inside the processing loop before the `rti::gear_executor::process()` call.
            if <DequeueLimit<T>>::get()
                .map(|limit| limit <= messages_processed)
                .unwrap_or(false)
            {
                return Ok(().into());
            }

            let mut stop_list = Vec::new();
            let mut total_handled = 0u32;

            for message in messages {
                match message {
                    // Initialization queue is handled separately and on the first place
                    // Any programs failed to initialize are deleted and further messages to them are not processed
                    IntermediateMessage::InitProgram {
                        origin,
                        code,
                        program_id,
                        payload,
                        gas_limit,
                        value,
                    } => {
                        match rti::gear_executor::init_program(
                            origin, program_id, code, payload, gas_limit, value,
                        ) {
                            Err(_) => {
                                stop_list.push(program_id);
                                Self::deposit_event(Event::InitFailure(
                                    program_id,
                                    MessageError::Dispatch,
                                ));
                            }
                            Ok(execution_report) => {
                                // In case of init, we can unreserve everything right away.
                                T::Currency::unreserve(
                                    &<T::AccountId as Origin>::from_origin(origin),
                                    gas_to_fee::<T>(gas_limit) + value.into(),
                                );

                                if let Err(_) = T::Currency::transfer(
                                    &<T::AccountId as Origin>::from_origin(origin),
                                    &<T::AccountId as Origin>::from_origin(program_id),
                                    value.into(),
                                    ExistenceRequirement::AllowDeath,
                                ) {
                                    // if transfer failed, gas spent and gas left does not matter since initialization
                                    // failed, and we unreserved gas_limit deposit already above.
                                    Self::deposit_event(Event::InitFailure(
                                        program_id,
                                        MessageError::ValueTransfer,
                                    ));
                                } else {
                                    Self::deposit_event(Event::ProgramInitialized(program_id));
                                    total_handled += execution_report.handled;

                                    // handle refunds
                                    for (destination, gas_charge) in execution_report.gas_charges {
                                        // TODO: weight to fee calculator might not be identity fee
                                        let charge = gas_to_fee::<T>(gas_charge);

                                        if let Err(_) = T::Currency::transfer(
                                            &<T::AccountId as Origin>::from_origin(destination),
                                            &block_author::<T>(),
                                            charge,
                                            ExistenceRequirement::AllowDeath,
                                        ) {
                                            // should not be possible since there should've been reserved enough for
                                            // the transfer
                                            // TODO: audit this
                                        }
                                    }

                                    for message in execution_report.log {
                                        Self::deposit_event(Event::Log(message));
                                    }
                                }
                            }
                        }
                    }
                    IntermediateMessage::DispatchMessage {
                        id,
                        origin,
                        destination,
                        payload,
                        gas_limit,
                        value,
                    } => {
                        common::queue_message(Message {
                            id,
                            source: origin,
                            payload,
                            gas_limit,
                            dest: destination,
                            value,
                            // TODO: user can actually reply to the messages with transactions
                            reply: None,
                        });
                    }
                }
            }

            loop {
                match rti::gear_executor::process() {
                    Ok(execution_report) => {
                        if execution_report.handled == 0 {
                            break;
                        }

                        total_handled += execution_report.handled;

                        <MessagesProcessed<T>>::mutate(|messages_processed| {
                            *messages_processed =
                                messages_processed.saturating_add(execution_report.handled)
                        });
                        let messages_processed = <MessagesProcessed<T>>::get();
                        if let Some(limit) = <DequeueLimit<T>>::get() {
                            if messages_processed >= limit {
                                break;
                            }
                        }

                        for (destination, gas_left) in execution_report.gas_refunds {
                            let refund = gas_to_fee::<T>(gas_left);

                            let _ = T::Currency::unreserve(
                                &<T::AccountId as Origin>::from_origin(destination),
                                refund,
                            );
                        }

                        for (destination, gas_charge) in execution_report.gas_charges {
                            let charge = gas_to_fee::<T>(gas_charge);

                            let _ = T::Currency::repatriate_reserved(
                                &<T::AccountId as Origin>::from_origin(destination),
                                &block_author::<T>(),
                                charge,
                                BalanceStatus::Free,
                            );
                        }

                        for (source, dest, gas_transfer) in execution_report.gas_transfers {
                            let transfer_fee = gas_to_fee::<T>(gas_transfer);

                            let _ = T::Currency::repatriate_reserved(
                                &<T::AccountId as Origin>::from_origin(source),
                                &<T::AccountId as Origin>::from_origin(dest),
                                transfer_fee,
                                BalanceStatus::Free,
                            );
                        }

                        for message in execution_report.log {
                            Self::deposit_event(Event::Log(message));
                        }
                    }
                    Err(_e) => {
                        // TODO: make error event log record
                        continue;
                    }
                }
            }

            Self::deposit_event(Event::MessagesDequeued(total_handled));

            Ok(().into())
        }
    }

    impl<T: Config> frame_support::inherent::ProvideInherent for Pallet<T>
    where
        T::AccountId: Origin,
        T: pallet_authorship::Config,
        <T::Currency as Currency<T::AccountId>>::Balance: Into<u128> + From<u128>,
    {
        type Call = Call<T>;
        type Error = sp_inherents::MakeFatalError<()>;
        const INHERENT_IDENTIFIER: InherentIdentifier = *b"gprocess";

        fn create_inherent(_data: &InherentData) -> Option<Self::Call> {
            Some(Call::process_queue())
        }

        fn is_inherent(call: &Self::Call) -> bool {
            matches!(call, Call::process_queue())
        }
    }
}
