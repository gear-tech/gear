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
        /// Program created and an init message enqueued.
        InitMessageEnqueued(MessageInfo),
        /// Program initialization error.
        InitFailure(MessageInfo, Reason),
        /// Program initialized.
        InitSuccess(MessageInfo),
        /// Dispatch message with a specific ID enqueued for processing.
        DispatchMessageEnqueued(H256),
        /// Dispatched message has resulted in an outcome
        MessageDispatched(DispatchOutcome),
        /// Some number of messages processed.
        // TODO: will be replaced by more comprehensive stats
        MessagesDequeued(u32),
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
        /// Program is not initialized.
        ///
        /// Occurs if a message is sent to a program that is in an uninitialized state.
        ProgramIsNotInitialized,
    }

    #[derive(Debug, Encode, Decode, Clone, PartialEq)]
    pub enum Reason {
        Error,
        ValueTransfer,
        Dispatch(Vec<u8>),
    }

    #[derive(Debug, Encode, Decode, Clone, PartialEq)]
    pub enum ExecutionResult {
        Success,
        Failure(Vec<u8>),
    }

    #[derive(Debug, Encode, Decode, Clone, PartialEq)]
    pub struct DispatchOutcome {
        pub message_id: H256,
        pub outcome: ExecutionResult,
    }

    #[derive(Debug, Encode, Decode, Clone, PartialEq)]
    pub struct MessageInfo {
        pub message_id: H256,
        pub program_id: H256,
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

    #[pallet::storage]
    pub type ProgramsLimbo<T: Config> = StorageMap<_, Identity, H256, H256>;

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

    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
        T: pallet_authorship::Config,
        <T::Currency as Currency<T::AccountId>>::Balance: Into<u128> + From<u128>,
    {
        fn gas_to_fee(gas: u64) -> BalanceOf<T> {
            IdentityFee::<BalanceOf<T>>::calc(&gas)
        }

        fn block_author() -> T::AccountId {
            <pallet_authorship::Pallet<T>>::author()
        }

        pub fn insert_to_mailbox(user: H256, message: common::Message) {
            let user_id = &<T::AccountId as Origin>::from_origin(user);

            <Mailbox<T>>::mutate(user_id, |value| {
                value
                    .get_or_insert(BTreeMap::new())
                    .insert(message.id, message)
            });
        }

        pub fn remove_from_mailbox(user: H256, message_id: H256) -> Option<common::Message> {
            let user_id = &<T::AccountId as Origin>::from_origin(user);

            <Mailbox<T>>::try_mutate(user_id, |value| match value {
                Some(ref mut btree) => Ok(btree.remove(&message_id)),
                None => Err(()),
            })
            .ok()
            .flatten()
        }

        pub fn get_gas_spent(destination: H256, payload: Vec<u8>) -> Option<u64> {
            rti::gear_executor::gas_spent(destination, payload, 0).ok()
        }

        pub fn is_uninitialized(program_id: H256) -> bool {
            ProgramsLimbo::<T>::get(program_id)
                .map(|_| true)
                .unwrap_or(false)
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
        T: pallet_authorship::Config,
        <T::Currency as Currency<T::AccountId>>::Balance: Into<u128> + From<u128>,
    {
        /// Creates a `Program` from wasm code and runs its init function.
        ///
        /// `ProgramId` is computed as Blake256 hash of concatenated bytes of `code` + `salt`.
        /// Such `ProgramId` must not exist in the Program Storage at the time of this call.
        ///
        /// The origin must be Signed and the sender must have sufficient funds to pay
        /// for `gas` and `value` (in case the latter is being transferred).
        ///
        /// Successful outcome assumes a programs has been created and initialized so that
        /// messages sent to this `ProgramId` will be enqueued for processing.
        ///
        /// Erroneous outcomes can be of two kinds:
        /// - program creation failed, that is there is no program in storage corresponding
        ///   to this `ProgramId`;
        /// - program was created but the initalization code resulted in a trap.
        ///
        /// Either of this cases indicates a program is in an undefined state:
        /// it either doesn't exist or is faulty (uninitialized).
        ///
        /// However, messages sent to such an address might still linger in the queue because
        /// the program id can deterministically be derived on the caller's side upfront.
        ///
        /// In order to mitigate the risk of users' funds being sent to an address,
        /// where a valid program should have resided, while it's not,
        /// such "failed-to-initialize" programs are not silently deleted from the
        /// program storage but rather marked as "ghost" programs.
        /// Ghost program can be removed by their original author via an explicit call.
        /// The funds stored by a ghost program will be release to the author once the program
        /// has been removed.
        ///
        /// Parameters:
        /// - `code`: wasm code of a program as a byte vector.
        /// - `salt`: randomness term (a seed) to allow programs with identical code
        ///   to be created independently.
        /// - `init_payload`: encoded parameters of the wasm module `init` function.
        /// - `gas_limit`: maximum amount of gas the program can spend before it is halted.
        /// - `value`: balance to be transferred to the program once it's been created.
        ///
        /// Emits the following events:
        /// - `InitMessageEnqueued(MessageInfo)` when init message is placed in the queue.
        ///
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

            let mut data = Vec::new();
            code.encode_to(&mut data);
            salt.encode_to(&mut data);

            let id: H256 = sp_io::hashing::blake2_256(&data[..]).into();

            // Make sure there is no program with such id in program storage
            if common::program_exists(id) {
                return Err(Error::<T>::ProgramAlreadyExists.into());
            }

            // Check that provided `gas_limit` value does not exceed the block gas limit
            if gas_limit > T::BlockGasLimit::get() {
                return Err(Error::<T>::GasLimitTooHigh.into());
            }

            let reserve_fee = Self::gas_to_fee(gas_limit);

            // First we reserve enough funds on the account to pay for 'gas_limit'
            // and to transfer declared value.
            T::Currency::reserve(&who, reserve_fee + value)
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            let init_message_id = common::next_message_id(&init_payload);
            <MessageQueue<T>>::append(IntermediateMessage::InitProgram {
                origin: who.into_origin(),
                code,
                program_id: id,
                init_message_id,
                payload: init_payload,
                gas_limit,
                value: value.into(),
            });

            Self::deposit_event(Event::InitMessageEnqueued(MessageInfo {
                message_id: init_message_id,
                program_id: id,
            }));

            Ok(().into())
        }

        /// Sends a message to a program or to another account.
        ///
        /// The origin must be Signed and the sender must have sufficient funds to pay
        /// for `gas` and `value` (in case the latter is being transferred).
        ///
        /// To avoid an undefined behavior a check is made that the destination address
        /// is not a program in uninitialized state. If the opposite holds true,
        /// the messsage is not enqueued for processing.
        ///
        /// Parameters:
        /// - `destination`: the message destination.
        /// - `payload`: in case of a program destination, parameters of the `handle` function.
        /// - `gas_limit`: maximum amount of gas the program can spend before it is halted.
        /// - `value`: balance to be transferred to the program once it's been created.
        ///
        /// Emits the following events:
        /// - `DispatchMessageEnqueued(H256)` when dispatch message is placed in the queue.
        ///
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

            // Check that the message is not intended for an uninitialized program
            if Self::is_uninitialized(destination) {
                return Err(Error::<T>::ProgramIsNotInitialized.into());
            }

            // Check that provided `gas_limit` value does not exceed the block gas limit
            if gas_limit > T::BlockGasLimit::get() {
                return Err(Error::<T>::GasLimitTooHigh.into());
            }

            let gas_limit_reserve = Self::gas_to_fee(gas_limit);

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
            let message_id = common::next_message_id(&payload);
            <MessageQueue<T>>::append(IntermediateMessage::DispatchMessage {
                id: message_id,
                origin: who.into_origin(),
                destination,
                payload,
                gas_limit,
                value: value.into(),
                reply: None,
            });

            Self::deposit_event(Event::DispatchMessageEnqueued(message_id));

            Ok(().into())
        }

        /// Sends a reply message.
        ///
        /// The origin must be Signed and the sender must have sufficient funds to pay
        /// for `gas` and `value` (in case the latter is being transferred).
        ///
        /// Parameters:
        /// - `reply_to_id`: the original message id.
        /// - `payload`: data expected by the original sender.
        /// - `gas_limit`: maximum amount of gas the program can spend before it is halted.
        /// - `value`: balance to be transferred to the program once it's been created.
        ///
        /// - `DispatchMessageEnqueued(H256)` when dispatch message is placed in the queue.
        ///
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

            let original_message =
                Self::remove_from_mailbox(who.clone().into_origin(), reply_to_id)
                    .ok_or(Error::<T>::NoMessageInMailbox)?;

            let destination = original_message.source;

            let gas_limit_reserve = Self::gas_to_fee(gas_limit);

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
            let message_id = common::next_message_id(&payload);
            <MessageQueue<T>>::append(IntermediateMessage::DispatchMessage {
                id: message_id,
                origin: who.into_origin(),
                destination,
                payload,
                gas_limit,
                value: value.into(),
                reply: Some(reply_to_id),
            });

            Self::deposit_event(Event::DispatchMessageEnqueued(message_id));

            Ok(().into())
        }

        /// Inherent extrinsic that processes the message queue.
        ///
        /// The origin must be None.
        ///
        /// Can emit the following events:
        /// - `InitSuccess(MessageInfo)` when initialization message is processed successfully;
        /// - `InitFailure(MessageInfo, Reason)` when initialization message fails;
        /// - `Log(Message)` when a dispatched message spawns other messages (including replies);
        /// - `MessageDispatched(H256)` when a dispatch message has been processed with some outcome.
        ///
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

            let mut total_handled = 0u32;

            for message in messages {
                match message {
                    // Initialization queue is handled separately and on the first place
                    // Any programs failed to initialize are deleted and further messages to them are not processed
                    IntermediateMessage::InitProgram {
                        origin,
                        ref code,
                        program_id,
                        init_message_id,
                        ref payload,
                        gas_limit,
                        value,
                    } => {
                        // Block gas allowance must be checked here for `InitProgram` messages
                        // as they are not placed in the internal message queue
                        if gas_limit > GasAllowance::<T>::get() {
                            // Put message back to storage to let it be processed in future blocks
                            MessageQueue::<T>::append(message);
                            log::info!(
                                "⛽️ Block gas limit exceeded: init message {} will be re-queued",
                                init_message_id,
                            );
                            continue;
                        }
                        match rti::gear_executor::init_program(
                            origin,
                            program_id,
                            code.to_vec(),
                            init_message_id,
                            payload.to_vec(),
                            gas_limit,
                            value,
                        ) {
                            Err(_) => {
                                // `init_program` in Runner can only return Err(_) in two cases:
                                // - failure to write program to Program Stroage
                                // - failure to instrument the init code
                                // In both cases the function returns before any gas could be spent.
                                // Hence no need to adjust the remaining gas allowance.

                                // No code has run hense unreserving everything
                                T::Currency::unreserve(
                                    &<T::AccountId as Origin>::from_origin(origin),
                                    Self::gas_to_fee(gas_limit) + value.into(),
                                );

                                // ProgramId must be placed in the "programs limbo" to forbid sending messages to it
                                ProgramsLimbo::<T>::insert(program_id, origin);
                                log::info!(
                                    "👻 Program {} will stay in limbo until explicitly removed",
                                    program_id
                                );
                                Self::deposit_event(Event::InitFailure(
                                    MessageInfo {
                                        message_id: init_message_id,
                                        program_id,
                                    },
                                    Reason::Error,
                                ));
                            }
                            Ok(execution_report) => {
                                // In case of init, we can unreserve everything right away.
                                T::Currency::unreserve(
                                    &<T::AccountId as Origin>::from_origin(origin),
                                    Self::gas_to_fee(gas_limit) + value.into(),
                                );

                                // Handle the stuff that should be taken care of regardless of the execution outcome
                                total_handled += execution_report.handled;

                                // handle refunds
                                for (destination, gas_charge) in execution_report.gas_charges {
                                    // Adjust block gas allowance
                                    GasAllowance::<T>::mutate(|x| {
                                        *x = x.saturating_sub(gas_charge)
                                    });

                                    // TODO: weight to fee calculator might not be identity fee
                                    let charge = Self::gas_to_fee(gas_charge);

                                    if let Err(_) = T::Currency::transfer(
                                        &<T::AccountId as Origin>::from_origin(destination),
                                        &Self::block_author(),
                                        charge,
                                        ExistenceRequirement::AllowDeath,
                                    ) {
                                        // should not be possible since there should've been reserved enough for
                                        // the transfer
                                        // TODO: audit this
                                    }
                                }

                                for (source, dest, gas_transfer) in execution_report.gas_transfers {
                                    let transfer_fee = Self::gas_to_fee(gas_transfer);

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

                                // Now, find out if the init message processing outcome is actually an error
                                let mut is_err = false;
                                let mut reason = Reason::Error;
                                for (_, exec_outcome) in execution_report.outcomes {
                                    if let Err(v) = exec_outcome {
                                        is_err = true;
                                        reason = Reason::Dispatch(v);
                                    }
                                }

                                if is_err
                                    || T::Currency::transfer(
                                        &<T::AccountId as Origin>::from_origin(origin),
                                        &<T::AccountId as Origin>::from_origin(program_id),
                                        value.into(),
                                        ExistenceRequirement::AllowDeath,
                                    )
                                    .is_err()
                                {
                                    // if transfer failed, gas left does not matter since initialization
                                    // had failed, and we already unreserved gas_limit deposit above.

                                    // ProgramId must be placed in the "programs limbo" to forbid sending messages to it
                                    ProgramsLimbo::<T>::insert(program_id, origin);
                                    log::info!(
                                        "👻 Program {} will stay in limbo until explicitly removed",
                                        program_id
                                    );

                                    Self::deposit_event(Event::InitFailure(
                                        MessageInfo {
                                            message_id: init_message_id,
                                            program_id,
                                        },
                                        if is_err {
                                            reason
                                        } else {
                                            Reason::ValueTransfer
                                        },
                                    ));
                                } else {
                                    Self::deposit_event(Event::InitSuccess(MessageInfo {
                                        message_id: init_message_id,
                                        program_id,
                                    }));
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
                            let refund = Self::gas_to_fee(gas_left);

                            let _ = T::Currency::unreserve(
                                &<T::AccountId as Origin>::from_origin(destination),
                                refund,
                            );
                        }

                        for (destination, gas_charge) in execution_report.gas_charges {
                            let charge = Self::gas_to_fee(gas_charge);

                            let _ = T::Currency::repatriate_reserved(
                                &<T::AccountId as Origin>::from_origin(destination),
                                &Self::block_author(),
                                charge,
                                BalanceStatus::Free,
                            );

                            // Decrease block gas allowance
                            GasAllowance::<T>::mutate(|x| *x = x.saturating_sub(gas_charge));
                        }

                        for (source, dest, gas_transfer) in execution_report.gas_transfers {
                            let transfer_fee = Self::gas_to_fee(gas_transfer);

                            let _ = T::Currency::repatriate_reserved(
                                &<T::AccountId as Origin>::from_origin(source),
                                &<T::AccountId as Origin>::from_origin(dest),
                                transfer_fee,
                                BalanceStatus::Free,
                            );
                        }

                        for message in execution_report.log {
                            Self::insert_to_mailbox(message.dest, message.clone());

                            Self::deposit_event(Event::Log(message));
                        }

                        for (message_id, outcome) in execution_report.outcomes {
                            Self::deposit_event(Event::MessageDispatched(DispatchOutcome {
                                message_id,
                                outcome: match outcome {
                                    Ok(_) => ExecutionResult::Success,
                                    Err(v) => ExecutionResult::Failure(v),
                                },
                            }));
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

        /// Removes stale program.
        ///
        /// The origin must be Signed and be the original creator of the program that
        /// got stuck in the "limbo" due to initialization failure.
        ///
        /// The gas and balance stored at the program's account will be transferred back
        /// to the original origin.
        ///
        /// Parameters:
        /// - `program_id`: the id of the program being removed.
        ///
        /// Emits the following events:
        /// - `ProgramRemoved(id)` when succesful.
        #[pallet::weight(T::DbWeight::get().writes(1))]
        pub fn remove_stale_program(
            origin: OriginFor<T>,
            program_id: H256,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            if let Some(author) = ProgramsLimbo::<T>::take(program_id) {
                if who.clone().into_origin() == author {
                    let account_id = &<T::AccountId as Origin>::from_origin(program_id);

                    // Remove program from program storage
                    common::remove_program(program_id);

                    // Complete transfer of the leftover balance back to the original sender
                    T::Currency::transfer(
                        account_id,
                        &who,
                        T::Currency::free_balance(account_id),
                        ExistenceRequirement::AllowDeath,
                    )?;
                }
            }

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
