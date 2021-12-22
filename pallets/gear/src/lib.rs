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
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod manager;
pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub type Authorship<T> = pallet_authorship::Pallet<T>;

pub trait DebugInfo {
    fn do_snapshot();
    fn is_enabled() -> bool;
}

impl DebugInfo for () {
    fn do_snapshot() {}
    fn is_enabled() -> bool {
        false
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    use common::{
        self, CodeMetadata, GasToFeeConverter, IntermediateMessage, Message, Origin,
        GAS_VALUE_PREFIX,
    };
    use core_processor::{
        common::{Dispatch, DispatchKind, DispatchOutcome as CoreDispatchOutcome, JournalNote},
        configs::BlockInfo,
        Ext,
    };
    use frame_support::{
        dispatch::{DispatchError, DispatchResultWithPostInfo},
        pallet_prelude::*,
        traits::{BalanceStatus, Currency, ExistenceRequirement, ReservableCurrency},
    };
    use frame_system::pallet_prelude::*;
    use gear_backend_sandbox::SandboxEnvironment;
    use gear_core::program::Program;
    use manager::ExtManager;
    use primitive_types::H256;
    use scale_info::TypeInfo;
    use sp_runtime::traits::{Saturating, UniqueSaturatedInto};
    use sp_std::{
        collections::{btree_map::BTreeMap, vec_deque::VecDeque},
        prelude::*,
    };

    #[pallet::config]
    pub trait Config:
        frame_system::Config + pallet_authorship::Config + pallet_timestamp::Config
    {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        /// Gas and value transfer currency
        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

        /// Gas to Currency converter
        type GasConverter: GasToFeeConverter<Balance = BalanceOf<Self>>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;

        /// The maximum amount of gas that can be used within a single block.
        #[pallet::constant]
        type BlockGasLimit: Get<u64>;

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
        /// Program code with a calculated code hash is saved to the storage
        CodeSaved(H256),
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
        /// Code already exists
        ///
        /// Occurs when trying to save to storage a program code, that has been saved there.
        CodeAlreadyExists,
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

        pub fn get_gas_spent(dest: H256, payload: Vec<u8>) -> Option<u64> {
            let mut ext_manager = ExtManager::<T>::new();

            let block_info = BlockInfo {
                height: <frame_system::Pallet<T>>::block_number().unique_saturated_into(),
                timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
            };

            let mut messages: VecDeque<Message> = VecDeque::from([Message {
                id: common::next_message_id(&payload),
                source: 1.into_origin(),
                dest,
                gas_limit: u64::MAX,
                payload,
                value: 0,
                reply: None,
            }]);

            let mut gas_burned = 0;

            while let Some(message) = messages.pop_front() {
                let program = ext_manager.get_program(message.dest).or(None)?;

                let kind = if message.reply.is_none() {
                    DispatchKind::Handle
                } else {
                    DispatchKind::HandleReply
                };

                let dispatch = Dispatch {
                    kind,
                    message: message.into(),
                };

                let res = core_processor::process::<SandboxEnvironment<Ext>>(
                    program, dispatch, block_info,
                );

                for note in &res.journal {
                    match note {
                        JournalNote::GasBurned { amount, .. } => {
                            gas_burned = gas_burned.saturating_add(*amount)
                        }
                        JournalNote::MessageDispatched(outcome) => match outcome {
                            CoreDispatchOutcome::InitFailure { .. } => return None,
                            CoreDispatchOutcome::MessageTrap { .. } => return None,
                            _ => {}
                        },
                        _ => {}
                    }
                }

                core_processor::handle_journal(res.journal, &mut ext_manager);
            }

            Some(gas_burned)
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
            let mut ext_manager = ExtManager::<T>::new();

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

                        common::value_tree::ValueView::get_or_create(
                            GAS_VALUE_PREFIX,
                            origin,
                            init_message_id,
                            gas_limit,
                        );

                        // Successful outcome assumes a programs has been created and initialized so that
                        // messages sent to this `ProgramId` will be enqueued for processing.
                        let init_message = common::Message {
                            id: init_message_id,
                            source: origin,
                            dest: program_id,
                            payload: payload.to_vec(),
                            gas_limit,
                            value,
                            reply: None,
                        };

                        let H256(bytes) = program_id;

                        if let Ok(program) = Program::new(bytes.into(), code.to_vec()) {
                            let dispatch = Dispatch {
                                kind: DispatchKind::Init,
                                message: init_message.into(),
                            };

                            let res = core_processor::process::<SandboxEnvironment<Ext>>(
                                program, dispatch, block_info,
                            );

                            core_processor::handle_journal(res.journal, &mut ext_manager);

                            total_handled += 1;
                        } else {
                            T::Currency::unreserve(
                                &<T::AccountId as Origin>::from_origin(origin),
                                T::GasConverter::gas_to_fee(gas_limit)
                                    + value.unique_saturated_into(),
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
                        };
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

                if let Some(program) = ext_manager.get_program(message.dest) {
                    let kind = if message.reply.is_none() {
                        DispatchKind::Handle
                    } else {
                        DispatchKind::HandleReply
                    };

                    let dispatch = Dispatch {
                        kind,
                        message: message.into(),
                    };

                    let res = core_processor::process::<SandboxEnvironment<Ext>>(
                        program, dispatch, block_info,
                    );

                    core_processor::handle_journal(res.journal, &mut ext_manager);

                    total_handled += 1;
                } else {
                    log::warn!(
                        "Couldn't find program: {:?}, message with id: {:?} will be skipped",
                        message.dest,
                        message.id,
                    );
                    // TODO: make error event log record
                    continue;
                }

                if T::DebugInfo::is_enabled() {
                    T::DebugInfo::do_snapshot();
                }
            }

            if total_handled > 0 {
                Self::deposit_event(Event::MessagesDequeued(total_handled));
            }

            weight = weight.saturating_sub(Self::gas_allowance());
            weight
        }

        /// Sets `code` and metadata, if code doesn't exist in storage.
        ///
        /// On success returns Blake256 hash of the `code`. If code already
        /// exists (*so, metadata exists as well*), returns unit type as error.
        ///
        /// # Note
        /// Code existence in storage means that metadata is there too.
        fn set_code_with_metadata(code: &[u8], who: H256) -> Result<H256, ()> {
            let code_hash = sp_io::hashing::blake2_256(code).into();
            // *Important*: checks before storage mutations!
            if common::code_exists(code_hash) {
                return Err(());
            }
            let metadata = {
                let block_number =
                    <frame_system::Pallet<T>>::block_number().unique_saturated_into();
                CodeMetadata::new(who, block_number)
            };
            common::set_code_metadata(code_hash, metadata);
            common::set_code(code_hash, code);

            Ok(code_hash)
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
        /// Saves program `code` in storage.
        ///
        /// The extrinsic was created to provide _deploy program from program_ functionality.
        /// Anyone who wants to define a "factory" logic in program should first store the code and metadata for the "child"
        /// program in storage. So the code for the child will be initialized by program initialization request only if it exists in storage.
        ///
        /// More precisely, the code and its metadata are actually saved in the storage under the hash of the `code`. The code hash is computed
        /// as Blake256 hash. At the time of the call the `code` hash should not be in the storage. If it was stored previously, call will end up
        /// with an `CodeAlreadyExists` error. In this case user can be sure, that he can actually use the hash of his program's code bytes to define
        /// "program factory" logic in his program.
        ///
        /// Parameters
        /// - `code`: wasm code of a program as a byte vector.
        ///
        /// Emits the following events:
        /// - `SavedCode(H256)` - when the code is saved in storage.
        #[pallet::weight(
            <T as Config>::WeightInfo::submit_code(code.len() as u32)
        )]
        pub fn submit_code(origin: OriginFor<T>, code: Vec<u8>) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            let code_hash = Self::set_code_with_metadata(&code, who.into_origin())
                .map_err(|_| Error::<T>::CodeAlreadyExists)?;

            Self::deposit_event(Event::CodeSaved(code_hash));

            Ok(().into())
        }

        /// Creates program initialization request (message), that is scheduled to be run in the same block.
        ///
        /// There are no guarantees that initialization message will be run in the same block due to block
        /// gas limit restrictions. For example, when it will be the message's turn, required gas limit for it
        /// could be more than remaining block gas limit. Therefore, the message processing will be postponed
        /// until the next block.
        ///
        /// `ProgramId` is computed as Blake256 hash of concatenated bytes of `code` + `salt`. (todo #512 `code_hash` + `salt`)
        /// Such `ProgramId` must not exist in the Program Storage at the time of this call.
        ///
        /// There is the same guarantee here as in `submit_code`. That is, future program's
        /// `code` and metadata are stored before message was added to the queue and processed.
        ///
        /// The origin must be Signed and the sender must have sufficient funds to pay
        /// for `gas` and `value` (in case the latter is being transferred).
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
        /// # Note
        /// Faulty (uninitialized) programs still have a valid addresses (program ids) that can deterministically be derived on the
        /// caller's side upfront. It means that if messages are sent to such an address, they might still linger in the queue.
        ///
        /// In order to mitigate the risk of users' funds being sent to an address,
        /// where a valid program should have resided, while it's not,
        /// such "failed-to-initialize" programs are not silently deleted from the
        /// program storage but rather marked as "ghost" programs.
        /// Ghost program can be removed by their original author via an explicit call.
        /// The funds stored by a ghost program will be release to the author once the program
        /// has been removed.
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
            // TODO #512
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

            let reserve_fee = T::GasConverter::gas_to_fee(gas_limit);

            // First we reserve enough funds on the account to pay for 'gas_limit'
            // and to transfer declared value.
            T::Currency::reserve(&who, reserve_fee + value)
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            let init_message_id = common::next_message_id(&init_payload);
            let origin = who.into_origin();

            // By that call we follow the guarantee that we have in `Self::submit_code` -
            // if there's code in storage, there's also metadata for it.
            if let Ok(code_hash) = Self::set_code_with_metadata(&code, origin) {
                Self::deposit_event(Event::CodeSaved(code_hash))
            };

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

            let gas_limit_reserve = T::GasConverter::gas_to_fee(gas_limit);

            // First we reserve enough funds on the account to pay for 'gas_limit'
            T::Currency::reserve(&who, gas_limit_reserve)
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            let message_id = common::next_message_id(&payload);

            // Since messages a guaranteed to be dispatched, we transfer value immediately
            T::Currency::transfer(
                &who,
                &<T::AccountId as Origin>::from_origin(destination),
                value,
                ExistenceRequirement::AllowDeath,
            )?;

            // Only after reservation the message is actually put in the queue.
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
            let gas_limit_reserve =
                T::GasConverter::gas_to_fee(gas_limit.saturating_sub(locked_gas));

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
                    T::GasConverter::gas_to_fee(locked_gas.saturating_sub(gas_limit)),
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
                let refund = T::GasConverter::gas_to_fee(gas_left);

                let _ = T::Currency::unreserve(
                    &<T::AccountId as Origin>::from_origin(external),
                    refund,
                );
            }

            Self::deposit_event(Event::ClaimedValueFromMailbox(message_id));

            Ok(().into())
        }
    }

    impl<T: Config> common::PaymentProvider<T::AccountId> for Pallet<T>
    where
        T::AccountId: Origin,
    {
        type Balance = BalanceOf<T>;

        fn withhold_reserved(
            source: H256,
            dest: &T::AccountId,
            amount: Self::Balance,
        ) -> Result<(), DispatchError> {
            let _ = T::Currency::repatriate_reserved(
                &<T::AccountId as Origin>::from_origin(source),
                dest,
                amount,
                BalanceStatus::Free,
            )?;

            Ok(())
        }
    }
}
