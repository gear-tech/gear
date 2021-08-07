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
        traits::{BalanceStatus, Currency, ExistenceRequirement, ReservableCurrency},
        weights::{IdentityFee, WeightToFeePolynomial},
    };
    use frame_system::pallet_prelude::*;
    use sp_core::H256;
    use sp_std::{collections::btree_map::BTreeMap, prelude::*};

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

        #[pallet::constant]
        type BlockGasLimit: Get<u64>;
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
        InitFailure(H256, Reason),
        /// Program initialized.
        ProgramInitialized(H256),
        /// Some number of messages processed.
        MessagesDequeued(u32),
        /// Message dispatch resulted in error
        MessageNotProcessed(Reason),
    }

    // Gear pallet error.
    #[pallet::error]
    pub enum Error<T> {
        /// Not enough balance to reserve.
        ///
        /// Usually occurs when gas_limit specified is such that origin account can't afford the message.
        NotEnoughBalanceForReserve,
        /// Gas limit too high.
        ///
        /// Occurs when an extrinsic's declared `gas_limit` is greater than a block's maximum gas limit.
        GasLimitTooHigh,

        /// Program already exists.
        ///
        /// Occurs if a program with some specific program id already exists in program storage.
        ProgramAlreadyExists,
        /// No message in the mailbox.
        ///
        /// The user tried to reply on message that was not found in his personal mailbox.
        NoMessageInMailbox,
    }

    #[derive(Debug, Encode, Decode, Clone, PartialEq)]
    pub enum Reason {
        ValueTransfer,
        Dispatch,
        BlockGasLimitExceeded,
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

    #[pallet::type_value]
    pub fn DefaultForGasLimit<T: Config>() -> u64 {
        T::BlockGasLimit::get()
    }

    #[pallet::storage]
    #[pallet::getter(fn gas_allowance)]
    pub type GasAllowance<T> = StorageValue<_, u64, ValueQuery, DefaultForGasLimit<T>>;

    #[pallet::storage]
    pub type Mailbox<T: Config> =
        StorageMap<_, Identity, T::AccountId, BTreeMap<H256, common::Message>>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// Initialization
        fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
            // Reset block gas allowance
            GasAllowance::<T>::put(T::BlockGasLimit::get());
            T::DbWeight::get().writes(1)
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

    pub fn insert_to_mailbox<T: Config>(user: H256, message: common::Message)
    where
        T::AccountId: Origin,
    {
        let user_id = &<T::AccountId as Origin>::from_origin(user);

        <Mailbox<T>>::mutate(user_id, |value| {
            value
                .get_or_insert(BTreeMap::new())
                .insert(message.id, message)
        });
    }

    pub fn remove_from_mailbox<T: Config>(user: H256, message_id: H256) -> Option<common::Message>
    where
        T::AccountId: Origin,
    {
        let user_id = &<T::AccountId as Origin>::from_origin(user);

        <Mailbox<T>>::try_mutate(user_id, |value| match value {
            Some(ref mut btree) => Ok(btree.remove(&message_id)),
            None => Err(()),
        })
        .ok()
        .flatten()
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

            // Check that provided `gas_limit` value does not exceed the block gas limit
            if gas_limit > T::BlockGasLimit::get() {
                return Err(Error::<T>::GasLimitTooHigh.into());
            }

            let mut data = Vec::new();
            code.encode_to(&mut data);
            salt.encode_to(&mut data);

            let id: H256 = sp_io::hashing::blake2_256(&data[..]).into();

            // Make sure there is no program with such id in program storage
            if common::program_exists(id) {
                return Err(Error::<T>::ProgramAlreadyExists.into());
            }

            let reserve_fee = gas_to_fee::<T>(gas_limit);

            // First we reserve enough funds on the account to pay for 'gas_limit'
            // and to transfer declared value.
            T::Currency::reserve(&who, reserve_fee + value)
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            <MessageQueue<T>>::append(IntermediateMessage::InitProgram {
                origin: who.into_origin(),
                code,
                program_id: id,
                payload: init_payload,
                gas_limit,
                value: value.into(),
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

            // Check that provided `gas_limit` value does not exceed the block gas limit
            if gas_limit > T::BlockGasLimit::get() {
                return Err(Error::<T>::GasLimitTooHigh.into());
            }

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
            let nonce = common::nonce_fetch_inc();
            let mut message_id = payload.encode();
            message_id.extend_from_slice(&nonce.to_le_bytes());
            let message_id: H256 = sp_io::hashing::blake2_256(&message_id).into();
            <MessageQueue<T>>::append(IntermediateMessage::DispatchMessage {
                id: message_id,
                origin: who.into_origin(),
                destination,
                payload,
                gas_limit,
                value: value.into(),
                reply: None,
            });

            Ok(().into())
        }

        #[pallet::weight(
			T::DbWeight::get().writes(4) +
			*gas_limit +
			T::MessagePerByte::get()*(payload.len() as u64)
		)]
        pub fn send_reply(
            origin: OriginFor<T>,
            reply_to_id: H256,
            payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            let original_message = remove_from_mailbox::<T>(who.clone().into_origin(), reply_to_id)
                .ok_or(Error::<T>::NoMessageInMailbox)?;

            let destination = original_message.source;

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

                let nonce = common::nonce_fetch_inc();

                let mut message_id = payload.encode();
                message_id.extend_from_slice(&nonce.to_le_bytes());
                let message_id: H256 = sp_io::hashing::blake2_256(&message_id).into();

                actual_messages.push(IntermediateMessage::DispatchMessage {
                    id: message_id,
                    origin: who.into_origin(),
                    destination,
                    payload,
                    gas_limit,
                    value: value.into(),
                    reply: Some(reply_to_id),
                });

                *messages = Some(actual_messages);
            });

            Ok(().into())
        }

        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub fn process_queue(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;

            // At the beginning of a new block, we process all queued messages
            let messages = <MessageQueue<T>>::take().unwrap_or_default();
            let messages_processed = <MessagesProcessed<T>>::get();

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
                        ref code,
                        program_id,
                        ref payload,
                        gas_limit,
                        value,
                    } => {
                        // Block gas allowance must be checked here for `InitProgram` messages
                        // as they are not placed in the internal message queue
                        if gas_limit > GasAllowance::<T>::get() {
                            // Put message back to storage to let it be processed in future blocks
                            MessageQueue::<T>::append(message);
                            Self::deposit_event(Event::MessageNotProcessed(
                                Reason::BlockGasLimitExceeded,
                            ));
                            continue;
                        }
                        match rti::gear_executor::init_program(
                            origin,
                            program_id,
                            code.to_vec(),
                            payload.to_vec(),
                            gas_limit,
                            value,
                        ) {
                            Err(_) => {
                                stop_list.push(program_id);
                                Self::deposit_event(Event::InitFailure(
                                    program_id,
                                    Reason::Dispatch,
                                ));
                                // Decrease remaining block gas allowance
                                // TODO: audit if it is safe to use the self-declared `gas_limit` here.
                                // Alternatively, find a way to report the acutal amount of gas spent.
                                GasAllowance::<T>::mutate(|x| *x = x.saturating_sub(gas_limit));
                                Self::deposit_event(Event::InitFailure(
                                    program_id,
                                    Reason::Dispatch,
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
                                    // if transfer failed, gas left does not matter since initialization
                                    // had failed, and we already unreserved gas_limit deposit above.
                                    Self::deposit_event(Event::InitFailure(
                                        program_id,
                                        Reason::ValueTransfer,
                                    ));
                                    // However, spend pas should still be accounted for to adjust global allowance
                                    let gas_spent = execution_report
                                        .gas_charges
                                        .iter()
                                        .fold(0, |acc, (_, x)| acc + x);
                                    // Decrease block gas allowance
                                    GasAllowance::<T>::mutate(|x| *x = x.saturating_sub(gas_spent));
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

                                        // Decrease block gas allowance
                                        GasAllowance::<T>::mutate(|x| {
                                            *x = x.saturating_sub(gas_charge)
                                        });
                                    }

                                    for (source, dest, gas_transfer) in
                                        execution_report.gas_transfers
                                    {
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
                        reply,
                    } => {
                        common::queue_message(Message {
                            id,
                            source: origin,
                            payload,
                            gas_limit,
                            dest: destination,
                            value,
                            reply: reply.map(|r| (r, 0)),
                        });
                    }
                }
            }

            loop {
                match rti::gear_executor::process(GasAllowance::<T>::get()) {
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

                            // Decrease block gas allowance
                            GasAllowance::<T>::mutate(|x| *x = x.saturating_sub(gas_charge));
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
                            insert_to_mailbox::<T>(message.dest, message.clone());

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

    impl<T: Config> Pallet<T> {
        pub fn get_gas_spent(destination: H256, payload: Vec<u8>) -> Option<u64> {
            rti::gear_executor::gas_spent(destination, payload, 0).ok()
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
