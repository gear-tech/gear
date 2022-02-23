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

extern crate alloc;

pub use pallet::*;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod ext;
pub mod manager;
pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub type Authorship<T> = pallet_authorship::Pallet<T>;

pub trait DebugInfo {
    fn is_remap_id_enabled() -> bool;
    fn remap_id();
    fn do_snapshot();
    fn is_enabled() -> bool;
}

impl DebugInfo for () {
    fn is_remap_id_enabled() -> bool {
        false
    }
    fn remap_id() {}
    fn do_snapshot() {}
    fn is_enabled() -> bool {
        false
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    use common::{
        self, CodeMetadata, DAGBasedLedger, Dispatch, GasPrice, Message, Origin, Program,
        ProgramState, QueuedDispatch, QueuedMessage,
    };
    use core_processor::{
        common::{DispatchOutcome as CoreDispatchOutcome, ExecutableActor, JournalNote},
        configs::BlockInfo,
    };
    use frame_support::{
        dispatch::{DispatchError, DispatchResultWithPostInfo},
        pallet_prelude::*,
        traits::{BalanceStatus, Currency, Get, ReservableCurrency},
    };
    use frame_system::pallet_prelude::*;
    use gear_backend_sandbox::SandboxEnvironment;
    use gear_core::{
        message::DispatchKind,
        program::{Program as NativeProgram, ProgramId},
    };
    use manager::{ExtManager, HandleKind};
    use primitive_types::H256;
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

        /// Gas to Currency converter
        type GasPrice: GasPrice<Balance = BalanceOf<Self>>;

        /// Implementation of a ledger to account for gas creation and consumption
        type GasHandler: DAGBasedLedger<ExternalOrigin = H256, Key = H256, Balance = u64>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;

        /// The maximum amount of gas that can be used within a single block.
        #[pallet::constant]
        type BlockGasLimit: Get<u64>;

        /// The cost for a message to spend one block in the wait list
        #[pallet::constant]
        type WaitListFeePerBlock: Get<u64>;

        type DebugInfo: DebugInfo;
    }

    type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Log event from the specific program.
        Log(common::QueuedMessage),
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
        AddedToWaitList(common::QueuedMessage),
        /// A message has been removed from the wait list
        RemovedFromWaitList(H256),
        /// Program code with a calculated code hash is saved to the storage
        CodeSaved(H256),
        /// Pallet associated storage has been wiped.
        DatabaseWiped,
        /// Message was not executed
        MessageNotExecuted(H256),
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
        /// Program is terminated
        ///
        /// Program init ended up with failure, so such message destination is unavailable anymore
        ProgramIsTerminated,
        /// Message gas tree is not found.
        ///
        /// When message claimed from mailbox has a corrupted or non-extant gas tree associated.
        NoMessageTree,
        /// Code already exists
        ///
        /// Occurs when trying to save to storage a program code, that has been saved there.
        CodeAlreadyExists,
        /// Failed to create a program.
        FailedToConstructProgram,
        /// Value doesnt cover ExistenceDeposit
        ValueLessThanMinimal,
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

    #[pallet::type_value]
    pub fn DefaultForGasLimit<T: Config>() -> u64 {
        T::BlockGasLimit::get()
    }

    #[pallet::storage]
    #[pallet::getter(fn gas_allowance)]
    pub type GasAllowance<T> = StorageValue<_, u64, ValueQuery, DefaultForGasLimit<T>>;

    #[pallet::storage]
    #[pallet::getter(fn mailbox)]
    pub type Mailbox<T: Config> =
        StorageMap<_, Identity, T::AccountId, BTreeMap<H256, common::QueuedMessage>>;

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
        // Messages have only two options to be inserted in mailbox:
        // 1. While message processing called `gr_wait`.
        // 2. While message addressed to program, that hadn't finished it's initialization.
        //
        // This means that program always exists in storage in active or terminated status.
        //
        // We also remove messages from mailbox for cases of out of rent (in `pallet-usage`)
        // and once program initialized or failed it's inititalization.
        pub fn insert_to_mailbox(user: H256, message: common::QueuedMessage) {
            let user_id = &<T::AccountId as Origin>::from_origin(user);

            <Mailbox<T>>::mutate(user_id, |value| {
                value
                    .get_or_insert(BTreeMap::new())
                    .insert(message.id, message)
            });
        }

        pub fn get_from_mailbox(user: H256, message_id: H256) -> Option<common::QueuedMessage> {
            let user_id = &<T::AccountId as Origin>::from_origin(user);

            <Mailbox<T>>::try_get(user_id)
                .ok()
                .and_then(|mut messages| messages.remove(&message_id))
        }

        pub fn remove_from_mailbox(user: H256, message_id: H256) -> Option<common::QueuedMessage> {
            let user_id = &<T::AccountId as Origin>::from_origin(user);

            <Mailbox<T>>::try_mutate(user_id, |value| match value {
                Some(ref mut btree) => Ok(btree.remove(&message_id)),
                None => Err(()),
            })
            .ok()
            .flatten()
        }

        pub fn remove_and_claim_from_mailbox(
            user_id: &T::AccountId,
            message_id: H256,
        ) -> Result<common::QueuedMessage, DispatchError> {
            let message = Self::remove_from_mailbox(user_id.clone().into_origin(), message_id)
                .ok_or(Error::<T>::NoMessageInMailbox)?;

            if message.value > 0 {
                // Assuming the programs has enough balance
                T::Currency::repatriate_reserved(
                    &<T::AccountId as Origin>::from_origin(message.source),
                    user_id,
                    message.value.unique_saturated_into(),
                    BalanceStatus::Free,
                )?;
            }

            Ok(message)
        }

        pub fn get_gas_spent(source: H256, kind: HandleKind, payload: Vec<u8>) -> Option<u64> {
            let ext_manager = ExtManager::<T>::default();

            let block_info = BlockInfo {
                height: <frame_system::Pallet<T>>::block_number().unique_saturated_into(),
                timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
            };

            let existential_deposit = T::Currency::minimum_balance().unique_saturated_into();

            let (dest, reply) = match kind {
                HandleKind::Init(ref code) => (sp_io::hashing::blake2_256(code).into(), None),
                HandleKind::Handle(dest) => (dest, None),
                HandleKind::Reply(msg_id, exit_code) => {
                    let msg = Self::get_from_mailbox(source, msg_id)?;
                    (msg.source, Some((msg_id, exit_code)))
                }
            };

            let message = Message {
                id: common::next_message_id(&payload),
                source,
                dest,
                gas_limit: u64::MAX,
                payload,
                value: 0,
                reply,
            };

            let mut gas_burned = 0;
            let mut gas_to_send = 0;

            let (kind, actor) = match kind {
                HandleKind::Init(code) => {
                    gas_burned = gas_burned
                        .saturating_add(<T as Config>::WeightInfo::submit_code(code.len() as u32));
                    (
                        DispatchKind::Init,
                        ext_manager.executable_actor_from_code(dest, code)?,
                    )
                }
                HandleKind::Handle(dest) => (
                    DispatchKind::Handle,
                    ext_manager.get_executable_actor(dest)?,
                ),
                HandleKind::Reply(..) => (
                    DispatchKind::HandleReply,
                    ext_manager.get_executable_actor(dest)?,
                ),
            };

            let dispatch = Dispatch {
                kind,
                message,
                payload_store: None,
            };

            let journal = core_processor::process::<
                ext::LazyPagesExt,
                SandboxEnvironment<ext::LazyPagesExt>,
            >(
                Some(actor),
                dispatch.into(),
                block_info,
                existential_deposit,
                ProgramId::from_origin(source),
            );

            for note in &journal {
                match note {
                    JournalNote::GasBurned { amount, .. } => {
                        gas_burned = gas_burned.saturating_add(*amount);
                    }
                    JournalNote::SendDispatch { dispatch, .. } => {
                        gas_to_send = gas_to_send.saturating_add(dispatch.message.gas_limit());
                    }
                    JournalNote::MessageDispatched(CoreDispatchOutcome::MessageTrap { .. }) => {
                        return None;
                    }
                    _ => (),
                }
            }

            Some(gas_burned.saturating_add(gas_to_send))
        }

        pub(crate) fn decrease_gas_allowance(gas_charge: u64) {
            GasAllowance::<T>::mutate(|x| *x = x.saturating_sub(gas_charge));
        }

        /// Returns true if a program has been successfully initialized
        pub fn is_initialized(program_id: H256) -> bool {
            common::get_program(program_id)
                .map(|p| p.is_initialized())
                .unwrap_or(false)
        }

        /// Returns true if a program has terminated status
        pub fn is_terminated(program_id: H256) -> bool {
            common::get_program(program_id)
                .map(|p| p.is_terminated())
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
            let mut ext_manager = ExtManager::<T>::default();

            let mut weight = Self::gas_allowance() as Weight;
            let mut total_handled = 0u32;

            let block_info = BlockInfo {
                height: <frame_system::Pallet<T>>::block_number().unique_saturated_into(),
                timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
            };

            let existential_deposit = T::Currency::minimum_balance().unique_saturated_into();

            if T::DebugInfo::is_remap_id_enabled() {
                T::DebugInfo::remap_id();
            }
            while let Some(dispatch) = common::dequeue_dispatch() {
                // Update message gas limit for it may have changed in the meantime

                let (gas_limit, initiator) = T::GasHandler::get_limit(*dispatch.message_id())
                    .expect("Should never fail if ValueNode works properly");

                log::debug!(
                    "Processing message: {:?} to {:?} / gas_limit: {}",
                    dispatch.message_id(),
                    dispatch.message.dest,
                    gas_limit
                );

                // Check whether we have enough of gas allowed for message processing
                if gas_limit > GasAllowance::<T>::get() {
                    common::queue_dispatch(dispatch);
                    break;
                }

                let maybe_active_actor = {
                    let program_id = dispatch.message.dest;
                    let current_message_id = dispatch.message.id;
                    let maybe_message_reply = dispatch.message.reply;

                    let maybe_active_program = common::get_program(program_id)
                        .expect("program with id got from message is guaranteed to exist");

                    // Check whether message should be added to the wait list
                    if let Program::Active(ref prog) = maybe_active_program {
                        let is_for_wait_list = maybe_message_reply.is_none()
                            && matches!(prog.state, ProgramState::Uninitialized {message_id} if message_id != current_message_id);
                        if is_for_wait_list {
                            Self::deposit_event(Event::AddedToWaitList(dispatch.message.clone()));
                            common::waiting_init_append_message_id(program_id, current_message_id);
                            common::insert_waiting_message(
                                program_id,
                                current_message_id,
                                dispatch,
                                block_info.height,
                            );

                            continue;
                        }
                    }

                    maybe_active_program
                        .try_into_native(program_id)
                        .ok()
                        .map(|program| {
                            let balance = T::Currency::free_balance(
                                &<T::AccountId as Origin>::from_origin(program_id),
                            )
                            .unique_saturated_into();

                            ExecutableActor { program, balance }
                        })
                };

                let journal = core_processor::process::<
                    ext::LazyPagesExt,
                    SandboxEnvironment<ext::LazyPagesExt>,
                >(
                    maybe_active_actor,
                    dispatch.into_dispatch(gas_limit),
                    block_info,
                    existential_deposit,
                    ProgramId::from_origin(initiator),
                );

                core_processor::handle_journal(journal, &mut ext_manager);

                total_handled += 1;

                if T::DebugInfo::is_enabled() {
                    T::DebugInfo::do_snapshot();
                }

                if T::DebugInfo::is_remap_id_enabled() {
                    T::DebugInfo::remap_id();
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
        /// exists (*so, metadata exists as well*), returns unit `CodeAlreadyExists` error.
        ///
        /// # Note
        /// Code existence in storage means that metadata is there too.
        fn set_code_with_metadata(code: &[u8], who: H256) -> Result<H256, Error<T>> {
            let code_hash = sp_io::hashing::blake2_256(code).into();
            // *Important*: checks before storage mutations!
            ensure!(
                !common::code_exists(code_hash),
                Error::<T>::CodeAlreadyExists
            );

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

            NativeProgram::new(Default::default(), code.clone())
                .map_err(|_| Error::<T>::FailedToConstructProgram)?;

            let code_hash = Self::set_code_with_metadata(&code, who.into_origin())?;

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

            // Check that provided `gas_limit` value does not exceed the block gas limit
            ensure!(
                gas_limit <= T::BlockGasLimit::get(),
                Error::<T>::GasLimitTooHigh
            );

            let mut data = Vec::new();
            // TODO #512
            code.encode_to(&mut data);
            salt.encode_to(&mut data);

            // Make sure there is no program with such id in program storage
            let id: H256 = sp_io::hashing::blake2_256(&data[..]).into();
            ensure!(
                !common::program_exists(id),
                Error::<T>::ProgramAlreadyExists
            );

            let H256(id_bytes) = id;
            let program = NativeProgram::new(id_bytes.into(), code.to_vec())
                .map_err(|_| Error::<T>::FailedToConstructProgram)?;

            let reserve_fee = T::GasPrice::gas_price(gas_limit);

            // First we reserve enough funds on the account to pay for `gas_limit`
            // and to transfer declared value.
            T::Currency::reserve(&who, reserve_fee + value)
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            let origin = who.into_origin();

            // By that call we follow the guarantee that we have in `Self::submit_code` -
            // if there's code in storage, there's also metadata for it.
            if let Ok(code_hash) = Self::set_code_with_metadata(&code, origin) {
                Self::deposit_event(Event::CodeSaved(code_hash));
            }

            let init_message_id = common::next_message_id(&init_payload);
            ExtManager::<T>::default().set_program(program, init_message_id);

            let _ = T::GasHandler::create(origin, init_message_id, gas_limit);

            let message = common::QueuedMessage {
                id: init_message_id,
                source: origin,
                dest: id,
                payload: init_payload,
                value: value.unique_saturated_into(),
                reply: None,
            };
            common::queue_dispatch(QueuedDispatch::new_init(message));

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

            let numeric_value: u128 = value.unique_saturated_into();
            let minimum: u128 = T::Currency::minimum_balance().unique_saturated_into();

            // Check that provided `gas_limit` value does not exceed the block gas limit
            ensure!(
                gas_limit <= T::BlockGasLimit::get(),
                Error::<T>::GasLimitTooHigh
            );

            // Check that provided `value` equals 0 or greater than existential deposit
            ensure!(
                0 == numeric_value || numeric_value >= minimum,
                Error::<T>::ValueLessThanMinimal
            );

            ensure!(
                !Self::is_terminated(destination),
                Error::<T>::ProgramIsTerminated
            );

            let message_id = common::next_message_id(&payload);

            // Message is not guaranteed to be executed, that's why value is not immediately transferred.
            // That's because destination can fail to be initialized, while this dispatch message is next
            // in the queue.
            T::Currency::reserve(&who, value.unique_saturated_into())
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            if common::program_exists(destination) {
                let gas_limit_reserve = T::GasPrice::gas_price(gas_limit);

                // First we reserve enough funds on the account to pay for `gas_limit`
                T::Currency::reserve(&who, gas_limit_reserve)
                    .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

                let origin = who.into_origin();

                let _ = T::GasHandler::create(origin, message_id, gas_limit);

                let message = QueuedMessage {
                    id: message_id,
                    source: origin,
                    payload,
                    dest: destination,
                    value: value.unique_saturated_into(),
                    reply: None,
                };
                common::queue_dispatch(QueuedDispatch::new_handle(message));

                Self::deposit_event(Event::DispatchMessageEnqueued(MessageInfo {
                    message_id,
                    origin,
                    program_id: destination,
                }));
            } else {
                // Message in mailbox is not meant for any processing, hence 0 gas limit
                // and no gas tree needs to be created
                let message = QueuedMessage {
                    id: message_id,
                    source: who.into_origin(),
                    payload,
                    dest: destination,
                    value: value.unique_saturated_into(),
                    reply: None,
                };

                Self::insert_to_mailbox(destination, message.clone());
                Self::deposit_event(Event::Log(message));
            }

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

            let numeric_value: u128 = value.unique_saturated_into();
            let minimum: u128 = T::Currency::minimum_balance().unique_saturated_into();

            // Ensure the `gas_limit` allows the extrinsic to fit into a block
            ensure!(
                gas_limit <= T::BlockGasLimit::get(),
                Error::<T>::GasLimitTooHigh
            );

            // Check that provided `value` equals 0 or greater than existential deposit
            ensure!(
                0 == numeric_value || numeric_value >= minimum,
                Error::<T>::ValueLessThanMinimal
            );

            // Claim outstanding value from the original message first
            let original_message = Self::remove_and_claim_from_mailbox(&who, reply_to_id)?;
            let destination = original_message.source;

            // Message is not guaranteed to be executed, that's why value is not immediately transferred.
            // That's because destination can fail to be initialized, while this dispatch message is next
            // in the queue.
            T::Currency::reserve(&who, value.unique_saturated_into())
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            let message_id = common::next_message_id(&payload);

            if common::program_exists(destination) {
                let gas_limit_reserve = T::GasPrice::gas_price(gas_limit);

                // First we reserve enough funds on the account to pay for `gas_limit`
                T::Currency::reserve(&who, gas_limit_reserve)
                    .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

                let origin = who.into_origin();

                let _ = T::GasHandler::create(origin, message_id, gas_limit);

                let message = QueuedMessage {
                    id: message_id,
                    source: origin,
                    payload,
                    dest: destination,
                    value: value.unique_saturated_into(),
                    reply: Some((reply_to_id, 0)),
                };
                common::queue_dispatch(QueuedDispatch::new_reply(message));

                Self::deposit_event(Event::DispatchMessageEnqueued(MessageInfo {
                    message_id,
                    origin,
                    program_id: destination,
                }));
            } else {
                // Message in mailbox is not meant for any processing, hence 0 gas limit
                // and no gas tree needs to be created
                let message = QueuedMessage {
                    id: message_id,
                    source: who.into_origin(),
                    payload,
                    dest: destination,
                    value: value.unique_saturated_into(),
                    reply: Some((reply_to_id, 0)),
                };

                Self::insert_to_mailbox(destination, message.clone());
                Self::deposit_event(Event::Log(message));
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

            let _ = Self::remove_and_claim_from_mailbox(&who, message_id)?;

            Self::deposit_event(Event::ClaimedValueFromMailbox(message_id));

            Ok(().into())
        }

        /// Reset all pallet associated storage.
        #[pallet::weight(0)]
        pub fn reset(origin: OriginFor<T>) -> DispatchResult {
            ensure_root(origin)?;
            <Mailbox<T>>::remove_all(None);
            common::reset_storage();

            Self::deposit_event(Event::DatabaseWiped);

            Ok(())
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
