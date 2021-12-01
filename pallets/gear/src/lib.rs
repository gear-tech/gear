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

pub use pallet::*;
#[cfg(feature = "debug-mode")]
pub use pallet_gear_debug::DebugInfo;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub type Authorship<T> = pallet_authorship::Pallet<T>;

const GAS_VALUE_PREFIX: &[u8] = b"g::gas_tree";

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::{self, IntermediateMessage, Message, Origin};
    use frame_support::{
        dispatch::DispatchResultWithPostInfo,
        pallet_prelude::*,
        traits::{BalanceStatus, Currency, ExistenceRequirement, ReservableCurrency},
        weights::{IdentityFee, WeightToFeePolynomial},
    };
    use frame_system::pallet_prelude::*;
    use primitive_types::H256;
    use runner::BlockInfo;
    use scale_info::TypeInfo;
    use sp_runtime::traits::{Saturating, UniqueSaturatedInto};
    use sp_std::{collections::btree_map::BTreeMap, prelude::*};

    #[pallet::config]
    pub trait Config:
        frame_system::Config + pallet_authorship::Config + pallet_timestamp::Config
    {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        /// Gas and value transfer currency
        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;

        /// The maximum amount of gas that can be used within a single block.
        #[pallet::constant]
        type BlockGasLimit: Get<u64>;

        #[cfg(feature = "debug-mode")]
        type DebugInfo: DebugInfo;
    }

    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
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
        DispatchMessageEnqueued(MessageInfo),
        /// Dispatched message has resulted in an outcome
        MessageDispatched(DispatchOutcome),
        /// Some number of messages processed.
        // TODO: will be replaced by more comprehensive stats
        MessagesDequeued(u32),
        /// Value and gas has been claimed from a message in mailbox by the addressee
        ClaimedValueFromMailbox(H256),
        /// A message has been added to the wait list
        AddedToWaitList(common::Message),
        /// A message has been removed from the wait list
        RemovedFromWaitList(H256),
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
        /// Message gas tree is not found.
        ///
        /// When message claimed from mailbox has a corrupted or non-extant gas tree associated.
        NoMessageTree,
    }

    #[derive(Debug, Encode, Decode, Clone, PartialEq, TypeInfo)]
    pub enum Reason {
        Error,
        ValueTransfer,
        Dispatch(Vec<u8>),
    }

    #[derive(Debug, Encode, Decode, Clone, PartialEq, TypeInfo)]
    pub enum ExecutionResult {
        Success,
        Failure(Vec<u8>),
    }

    #[derive(Debug, Encode, Decode, Clone, PartialEq, TypeInfo)]
    pub struct DispatchOutcome {
        pub message_id: H256,
        pub outcome: ExecutionResult,
    }

    #[derive(Debug, Encode, Decode, Clone, PartialEq, TypeInfo)]
    pub struct MessageInfo {
        pub message_id: H256,
        pub program_id: H256,
        pub origin: H256,
    }

    #[pallet::storage]
    #[pallet::getter(fn message_queue)]
    pub type MessageQueue<T> = StorageValue<_, Vec<IntermediateMessage>>;

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
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    where
        T::AccountId: Origin,
    {
        /// Initialization
        fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
            // Reset block gas allowance
            GasAllowance::<T>::put(T::BlockGasLimit::get());
            T::DbWeight::get().writes(1)
        }

        /// Finalization
        fn on_finalize(_bn: BlockNumberFor<T>) {}

        /// Queue processing occurs after all normal extrinsics in the block
        ///
        /// There should always remain enough weight for this hook to be invoked
        fn on_idle(bn: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
            log::debug!(
                target: "runtime::gear",
                "{} of weight remains in block {:?} after normal extrinsics have been processed",
                remaining_weight,
                bn,
            );
            // Adjust the block gas allowance based on actual remaining weight
            GasAllowance::<T>::put(remaining_weight);
            let mut weight = T::DbWeight::get().writes(1);
            weight += Self::process_queue();

            weight
        }
    }

    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
        fn gas_to_fee(gas: u64) -> BalanceOf<T> {
            IdentityFee::<BalanceOf<T>>::calc(&gas)
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
            runner::gas_spent::<gear_backend_sandbox::SandboxEnvironment<runner::Ext>>(
                destination,
                payload,
                0,
            )
            .ok()
        }

        /// Returns true if a program resulted in an error during initialization
        /// but hasn't been explicitly removed from storage by its creator
        pub fn is_uninitialized(program_id: H256) -> bool {
            ProgramsLimbo::<T>::get(program_id)
                .map(|_| true)
                .unwrap_or(false)
        }

        /// Message Queue processing.
        ///
        /// Can emit the following events:
        /// - `InitSuccess(MessageInfo)` when initialization message is processed successfully;
        /// - `InitFailure(MessageInfo, Reason)` when initialization message fails;
        /// - `Log(Message)` when a dispatched message spawns other messages (including replies);
        /// - `MessageDispatched(H256)` when a dispatch message has been processed with some outcome.
        pub fn process_queue() -> Weight {
            // At the beginning of a new block, we process all queued messages
            let messages = <MessageQueue<T>>::take().unwrap_or_default();

            let mut weight = Self::gas_allowance() as Weight;
            let mut total_handled = 0u32;
            let block_info = BlockInfo {
                height: <frame_system::Pallet<T>>::block_number().unique_saturated_into(),
                timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
            };

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
                                target: "runtime::gear",
                                "⛽️ Block gas limit exceeded: init message {} will be re-queued",
                                init_message_id,
                            );
                            continue;
                        }

                        let mut gas_tree = common::value_tree::ValueView::get_or_create(
                            GAS_VALUE_PREFIX,
                            origin,
                            init_message_id,
                            gas_limit,
                        );

                        match runner::init_program::<
                            gear_backend_sandbox::SandboxEnvironment<runner::Ext>,
                        >(
                            origin,
                            program_id,
                            code.to_vec(),
                            init_message_id,
                            payload.to_vec(),
                            gas_limit,
                            value,
                            block_info,
                        ) {
                            Err(_) => {
                                // `init_program` in Runner can only return Err(_) in two cases:
                                // - failure to write program to Program Storage
                                // - failure to instrument the init code
                                // In both cases the function returns before any gas could be spent.
                                // Hence no need to adjust the remaining gas allowance.

                                // No code has run hense unreserving everything
                                T::Currency::unreserve(
                                    &<T::AccountId as Origin>::from_origin(origin),
                                    Self::gas_to_fee(gas_limit) + value.unique_saturated_into(),
                                );

                                // ProgramId must be placed in the "programs limbo" to forbid sending messages to it
                                ProgramsLimbo::<T>::insert(program_id, origin);
                                log::info!(
                                    target: "runtime::gear",
                                    "👻 Program {} will stay in limbo until explicitly removed",
                                    program_id
                                );
                                Self::deposit_event(Event::InitFailure(
                                    MessageInfo {
                                        message_id: init_message_id,
                                        program_id,
                                        origin,
                                    },
                                    Reason::Error,
                                ));
                            }
                            Ok(execution_report) => {
                                // In case of init, we can unreserve everything right away.
                                T::Currency::unreserve(
                                    &<T::AccountId as Origin>::from_origin(origin),
                                    Self::gas_to_fee(gas_limit) + value.unique_saturated_into(),
                                );

                                // Handle the stuff that should be taken care of regardless of the execution outcome
                                total_handled += 1;

                                // handle gas charge
                                for (_, gas_charge) in execution_report.gas_charges {
                                    // Adjust block gas allowance
                                    GasAllowance::<T>::mutate(|x| {
                                        *x = x.saturating_sub(gas_charge)
                                    });

                                    // TODO: weight to fee calculator might not be identity fee
                                    let charge = Self::gas_to_fee(gas_charge);

                                    gas_tree.spend(gas_charge);
                                    if let Err(e) = T::Currency::transfer(
                                        &<T::AccountId as Origin>::from_origin(origin),
                                        &Authorship::<T>::author(),
                                        charge,
                                        ExistenceRequirement::AllowDeath,
                                    ) {
                                        // should not be possible since there should've been reserved enough for
                                        // the transfer
                                        // TODO: audit this
                                        log::warn!(
                                            "Could not transfer enough gas to block producer: {:?}",
                                            e
                                        );
                                    }
                                }

                                for message in execution_report.log {
                                    Self::insert_to_mailbox(message.dest, message.clone());
                                    Self::deposit_event(Event::Log(message));
                                }

                                // Enqueuing outgoing messages
                                for message in execution_report.messages {
                                    gas_tree.split_off(message.id, message.gas_limit);
                                    common::queue_message(message);
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
                                        value.unique_saturated_into(),
                                        ExistenceRequirement::AllowDeath,
                                    )
                                    .is_err()
                                {
                                    // if transfer failed, gas left does not matter since initialization
                                    // had failed, and we already unreserved gas_limit deposit above.

                                    // ProgramId must be placed in the "programs limbo" to forbid sending messages to it
                                    ProgramsLimbo::<T>::insert(program_id, origin);
                                    log::info!(
                                        target: "runtime::gear",
                                        "👻 Program {} will stay in limbo until explicitly removed",
                                        program_id
                                    );

                                    Self::deposit_event(Event::InitFailure(
                                        MessageInfo {
                                            message_id: init_message_id,
                                            program_id,
                                            origin,
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
                                        origin,
                                    }));
                                }
                            }
                        }
                        #[cfg(feature = "debug-mode")]
                        if T::DebugInfo::is_enabled() {
                            T::DebugInfo::do_snapshot();
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
                        let _ = common::value_tree::ValueView::get_or_create(
                            GAS_VALUE_PREFIX,
                            origin,
                            id,
                            gas_limit,
                        );

                        let message = Message {
                            id,
                            source: origin,
                            payload,
                            gas_limit,
                            dest: destination,
                            value,
                            reply: reply.map(|r| (r, 0)),
                        };

                        if common::program_exists(destination) {
                            common::queue_message(message);
                        } else {
                            Self::insert_to_mailbox(destination, message.clone());
                            Self::deposit_event(Event::Log(message));
                        }
                    }
                }
            }

            while let Some(message) = common::dequeue_message() {
                // Check whether we have enough of gas allowed for message processing
                if message.gas_limit > GasAllowance::<T>::get() {
                    common::queue_message(message);
                    break;
                }

                let mut gas_tree =
                    match common::value_tree::ValueView::get(GAS_VALUE_PREFIX, message.id) {
                        Some(gas_tree) => gas_tree,
                        None => {
                            log::warn!(
                                "Message does not have associated gas and will be skipped: {:?}",
                                message.id
                            );
                            continue;
                        }
                    };

                match runner::process::<gear_backend_sandbox::SandboxEnvironment<runner::Ext>>(
                    message, block_info,
                ) {
                    Ok(execution_report) => {
                        total_handled += 1;

                        let origin = gas_tree.origin();

                        for (_, gas_charge) in execution_report.gas_charges {
                            gas_tree.spend(gas_charge);

                            let charge = Self::gas_to_fee(gas_charge);

                            let _ = T::Currency::repatriate_reserved(
                                &<T::AccountId as Origin>::from_origin(origin),
                                &Authorship::<T>::author(),
                                charge,
                                BalanceStatus::Free,
                            );

                            // Decrease block gas allowance
                            GasAllowance::<T>::mutate(|x| *x = x.saturating_sub(gas_charge));
                        }

                        for message in execution_report.log {
                            Self::insert_to_mailbox(message.dest, message.clone());

                            Self::deposit_event(Event::Log(message));
                        }

                        // Enqueuing outgoing messages
                        for message in execution_report.messages {
                            gas_tree.split_off(message.id, message.gas_limit);

                            common::queue_message(message);
                        }

                        let mut waited = false;
                        for msg in execution_report.wait_list {
                            Self::deposit_event(Event::AddedToWaitList(msg.clone()));
                            common::insert_waiting_message(msg.dest, msg.id, msg);
                            waited = true;
                        }

                        if !waited {
                            if let common::value_tree::ConsumeResult::RefundExternal(
                                external,
                                gas_left,
                            ) = gas_tree.consume()
                            {
                                let refund = Self::gas_to_fee(gas_left);

                                let _ = T::Currency::unreserve(
                                    &<T::AccountId as Origin>::from_origin(external),
                                    refund,
                                );
                            }
                        }

                        for msg_id in execution_report.awakening {
                            if let Some(msg) =
                                common::remove_waiting_message(execution_report.program_id, msg_id)
                            {
                                common::queue_message(msg);
                                Self::deposit_event(Event::RemovedFromWaitList(msg_id));
                            } else {
                                log::warn!("Unknown message awaken: {}", msg_id);
                            }
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
                    Err(e) => {
                        log::warn!(
                            "Message processing returned error and will be skipped: {:?}",
                            e
                        );
                        // TODO: make error event log record
                        continue;
                    }
                }

                #[cfg(feature = "debug-mode")]
                if T::DebugInfo::is_enabled() {
                    T::DebugInfo::do_snapshot();
                }
            }

            Self::deposit_event(Event::MessagesDequeued(total_handled));

            weight = weight.saturating_sub(Self::gas_allowance());
            weight
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
        /// Create a `Program` from wasm code and runs its init function.
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
        /// - program was created but the initialization code resulted in a trap.
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
        #[pallet::weight(
            <T as Config>::WeightInfo::submit_program(code.len() as u32, init_payload.len() as u32)
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
            ensure!(
                !common::program_exists(id),
                Error::<T>::ProgramAlreadyExists
            );

            // Check that provided `gas_limit` value does not exceed the block gas limit
            ensure!(
                gas_limit <= T::BlockGasLimit::get(),
                Error::<T>::GasLimitTooHigh
            );

            let reserve_fee = Self::gas_to_fee(gas_limit);

            // First we reserve enough funds on the account to pay for 'gas_limit'
            // and to transfer declared value.
            T::Currency::reserve(&who, reserve_fee + value)
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            let init_message_id = common::next_message_id(&init_payload);
            let origin = who.into_origin();
            <MessageQueue<T>>::append(IntermediateMessage::InitProgram {
                origin,
                code,
                program_id: id,
                init_message_id,
                payload: init_payload,
                gas_limit,
                value: value.unique_saturated_into(),
            });

            Self::deposit_event(Event::InitMessageEnqueued(MessageInfo {
                message_id: init_message_id,
                program_id: id,
                origin,
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
        /// the message is not enqueued for processing.
        ///
        /// Parameters:
        /// - `destination`: the message destination.
        /// - `payload`: in case of a program destination, parameters of the `handle` function.
        /// - `gas_limit`: maximum amount of gas the program can spend before it is halted.
        /// - `value`: balance to be transferred to the program once it's been created.
        ///
        /// Emits the following events:
        /// - `DispatchMessageEnqueued(MessageInfo)` when dispatch message is placed in the queue.
        #[frame_support::transactional]
        #[pallet::weight(<T as Config>::WeightInfo::send_message(payload.len() as u32))]
        pub fn send_message(
            origin: OriginFor<T>,
            destination: H256,
            payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            // Check that the message is not intended for an uninitialized program
            ensure!(
                !Self::is_uninitialized(destination),
                Error::<T>::ProgramIsNotInitialized
            );

            // Check that provided `gas_limit` value does not exceed the block gas limit
            ensure!(
                gas_limit <= T::BlockGasLimit::get(),
                Error::<T>::GasLimitTooHigh
            );

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
                origin: who.clone().into_origin(),
                destination,
                payload,
                gas_limit,
                value: value.unique_saturated_into(),
                reply: None,
            });

            Self::deposit_event(Event::DispatchMessageEnqueued(MessageInfo {
                message_id,
                origin: who.into_origin(),
                program_id: destination,
            }));

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
        #[frame_support::transactional]
        #[pallet::weight(<T as Config>::WeightInfo::send_reply(payload.len() as u32))]
        pub fn send_reply(
            origin: OriginFor<T>,
            reply_to_id: H256,
            payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            // Ensure the `gas_limit` allows the extrinsic to fit into a block
            ensure!(
                gas_limit <= T::BlockGasLimit::get(),
                Error::<T>::GasLimitTooHigh
            );

            let original_message =
                Self::remove_from_mailbox(who.clone().into_origin(), reply_to_id)
                    .ok_or(Error::<T>::NoMessageInMailbox)?;

            let destination = original_message.source;

            let locked_gas = original_message.gas_limit;
            // Offset the gas_limit against the gas passed to us in the original message
            let gas_limit_reserve = Self::gas_to_fee(gas_limit.saturating_sub(locked_gas));

            if gas_limit_reserve > 0_u32.into() {
                // First we reserve enough funds on the account to pay for 'gas_limit'
                T::Currency::reserve(&who, gas_limit_reserve)
                    .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;
            } else {
                // There still might be some gas leftover that needs to be refunded to the origin
                // Assuming the programs has enough balance
                T::Currency::transfer(
                    &<T::AccountId as Origin>::from_origin(destination),
                    &who,
                    Self::gas_to_fee(locked_gas.saturating_sub(gas_limit)),
                    ExistenceRequirement::AllowDeath,
                )?;
            }

            let locked_value: BalanceOf<T> = original_message.value.unique_saturated_into();
            // Tally up the `values` from the two messages to find out who owes who
            let offset_value = value.saturating_sub(locked_value);
            if offset_value > 0_u32.into() {
                // Some outstanding amount still remains to be transferred to the original message source
                T::Currency::transfer(
                    &who,
                    &<T::AccountId as Origin>::from_origin(destination),
                    offset_value,
                    ExistenceRequirement::AllowDeath,
                )?;
            } else {
                // The value owed to us exceeds the `value` to transfer out
                // TODO: here we assume that since the message ended up in the mailbox all necessary
                // checks had been done, including the validity of the `value`amount the program that had created
                // the message wants to transfer (in other words, it has enough balance). Need to audit this.
                T::Currency::transfer(
                    &<T::AccountId as Origin>::from_origin(destination),
                    &who,
                    locked_value.saturating_sub(value),
                    ExistenceRequirement::AllowDeath, // TODO: should we use ExistenceRequirement::KeepAlive instead?
                )?;
            }

            let message_id = common::next_message_id(&payload);
            <MessageQueue<T>>::append(IntermediateMessage::DispatchMessage {
                id: message_id,
                origin: who.clone().into_origin(),
                destination,
                payload,
                gas_limit,
                value: value.unique_saturated_into(),
                reply: Some(reply_to_id),
            });

            Self::deposit_event(Event::DispatchMessageEnqueued(MessageInfo {
                message_id,
                origin: who.into_origin(),
                program_id: destination,
            }));

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
        /// - `ProgramRemoved(id)` when successful.
        #[pallet::weight(<T as Config>::WeightInfo::remove_stale_program())]
        pub fn remove_stale_program(
            origin: OriginFor<T>,
            program_id: H256,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            if let Some(author) = ProgramsLimbo::<T>::get(program_id) {
                if who.clone().into_origin() == author {
                    ProgramsLimbo::<T>::remove(program_id);

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

        #[frame_support::transactional]
        #[pallet::weight(T::DbWeight::get().writes(1))]
        pub fn claim_value_from_mailbox(
            origin: OriginFor<T>,
            message_id: H256,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            let message = Self::remove_from_mailbox(who.clone().into_origin(), message_id)
                .ok_or(Error::<T>::NoMessageInMailbox)?;

            // gas should be returned to sender tree
            let gas_tree = common::value_tree::ValueView::get(GAS_VALUE_PREFIX, message.id)
                .ok_or(Error::<T>::NoMessageTree)?;

            if message.value > 0 {
                // Assuming the programs has enough balance
                T::Currency::transfer(
                    &<T::AccountId as Origin>::from_origin(message.source),
                    &who,
                    message.value.unique_saturated_into(),
                    ExistenceRequirement::AllowDeath,
                )?;
            }

            if let common::value_tree::ConsumeResult::RefundExternal(external, gas_left) =
                gas_tree.consume()
            {
                let refund = Self::gas_to_fee(gas_left);

                let _ = T::Currency::unreserve(
                    &<T::AccountId as Origin>::from_origin(external),
                    refund,
                );
            }

            Self::deposit_event(Event::ClaimedValueFromMailbox(message_id));

            Ok(().into())
        }
    }
}
