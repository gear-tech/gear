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
#![cfg_attr(feature = "runtime-benchmarks", recursion_limit = "512")]

extern crate alloc;

use codec::{Decode, Encode};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
mod ext;
mod internal;
mod schedule;

pub mod manager;
pub mod migration;
pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub use crate::{
    manager::{ExtManager, HandleKind},
    pallet::*,
    schedule::{HostFnWeights, InstructionWeights, Limits, Schedule},
};
pub use weights::WeightInfo;

use common::{scheduler::*, storage::*, BlockLimiter, CodeStorage, GasProvider};
use frame_support::{
    traits::{Currency, StorageVersion},
    weights::Weight,
};
use gear_backend_sandbox::SandboxEnvironment;
use gear_core::{
    code::{Code, CodeAndId, InstrumentedCode, InstrumentedCodeAndId},
    ids::{CodeId, MessageId, ProgramId},
    message::*,
    program::Program as NativeProgram,
};
use pallet_gear_program::Pallet as GearProgramPallet;
use primitive_types::H256;
use sp_runtime::traits::{Saturating, UniqueSaturatedInto};
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    convert::TryInto,
    prelude::*,
};

pub(crate) use frame_system::Pallet as SystemPallet;

pub(crate) type CurrencyOf<T> = <T as Config>::Currency;
pub(crate) type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
pub(crate) type SentOf<T> = <<T as Config>::Messenger as Messenger>::Sent;
pub(crate) type DequeuedOf<T> = <<T as Config>::Messenger as Messenger>::Dequeued;
pub(crate) type QueueProcessingOf<T> = <<T as Config>::Messenger as Messenger>::QueueProcessing;
pub(crate) type QueueOf<T> = <<T as Config>::Messenger as Messenger>::Queue;
pub(crate) type MailboxOf<T> = <<T as Config>::Messenger as Messenger>::Mailbox;
pub(crate) type WaitlistOf<T> = <<T as Config>::Messenger as Messenger>::Waitlist;
pub(crate) type MessengerCapacityOf<T> = <<T as Config>::Messenger as Messenger>::Capacity;
pub(crate) type TaskPoolOf<T> = <<T as Config>::Scheduler as Scheduler>::TaskPool;
pub(crate) type MissedBlocksOf<T> = <<T as Config>::Scheduler as Scheduler>::MissedBlocks;
pub(crate) type CostsPerBlockOf<T> = <<T as Config>::Scheduler as Scheduler>::CostsPerBlock;
pub(crate) type SchedulingCostOf<T> = <<T as Config>::Scheduler as Scheduler>::Cost;
pub type Authorship<T> = pallet_authorship::Pallet<T>;
pub type GasAllowanceOf<T> = <<T as Config>::BlockLimiter as BlockLimiter>::GasAllowance;
pub type GasHandlerOf<T> = <<T as Config>::GasProvider as GasProvider>::GasTree;
pub type BlockGasLimitOf<T> = <<T as Config>::BlockLimiter as BlockLimiter>::BlockGasLimit;

/// The current storage version.
const GEAR_STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

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

/// The struct contains results of gas calculation required to process
/// a message.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, scale_info::TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
pub struct GasInfo {
    /// Represents minimum gas limit required for execution.
    pub min_limit: u64,
    /// Gas amount that we reserve for some other on-chain interactions.
    pub reserved: u64,
    /// Contains number of gas burned during message processing.
    pub burned: u64,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    use crate::{
        ext::LazyPagesExt,
        manager::{ExtManager, HandleKind, QueuePostProcessingData},
    };
    use alloc::format;
    use common::{
        self, event::*, lazy_pages, BlockLimiter, CodeMetadata, GasPrice, GasProvider, GasTree,
        Origin, Program, ProgramState,
    };
    use core_processor::{
        common::{
            Actor, DispatchOutcome as CoreDispatchOutcome, ExecutableActorData, JournalHandler,
            JournalNote,
        },
        configs::{AllocationsConfig, BlockConfig, BlockInfo, MessageExecutionContext},
        Ext,
    };
    use frame_support::{
        dispatch::{DispatchError, DispatchResultWithPostInfo},
        ensure,
        pallet_prelude::*,
        traits::{
            BalanceStatus, Currency, ExistenceRequirement, Get, LockableCurrency,
            ReservableCurrency,
        },
    };
    use frame_system::pallet_prelude::*;

    #[pallet::config]
    pub trait Config:
        frame_system::Config
        + pallet_authorship::Config
        + pallet_timestamp::Config
        + pallet_gear_program::Config<Currency = <Self as Config>::Currency>
    {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        /// Balances management trait for gas/value migrations.
        type Currency: LockableCurrency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

        /// Gas to Currency converter
        type GasPrice: GasPrice<Balance = BalanceOf<Self>>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;

        /// Cost schedule and limits.
        #[pallet::constant]
        type Schedule: Get<Schedule<Self>>;

        /// The maximum amount of messages that can be produced in single run.
        #[pallet::constant]
        type OutgoingLimit: Get<u32>;

        type DebugInfo: DebugInfo;

        type CodeStorage: CodeStorage;

        /// The minimal gas amount for message to be inserted in mailbox.
        ///
        /// This gas will be consuming as rent for storing and message will be available
        /// for reply or claim, once gas ends, message removes.
        ///
        /// Messages with gas limit less than that minimum will not be added in mailbox,
        /// but will be seen in events.
        #[pallet::constant]
        type MailboxThreshold: Get<u64>;

        /// Messenger.
        type Messenger: Messenger<
            BlockNumber = Self::BlockNumber,
            Capacity = u32,
            OutputError = DispatchError,
            MailboxFirstKey = Self::AccountId,
            MailboxSecondKey = MessageId,
            MailboxedMessage = StoredMessage,
            QueuedDispatch = StoredDispatch,
            WaitlistFirstKey = ProgramId,
            WaitlistSecondKey = MessageId,
            WaitlistedMessage = StoredDispatch,
        >;

        /// Implementation of a ledger to account for gas creation and consumption
        type GasProvider: GasProvider<
            ExternalOrigin = Self::AccountId,
            Key = MessageId,
            Balance = u64,
            Error = DispatchError,
        >;

        /// Block limits.
        type BlockLimiter: BlockLimiter<Balance = u64>;

        /// Scheduler.
        type Scheduler: Scheduler<
            BlockNumber = Self::BlockNumber,
            Cost = u64,
            Task = ScheduledTask<Self::AccountId>,
            MissedBlocksCollection = BTreeSet<Self::BlockNumber>,
        >;
    }

    #[pallet::pallet]
    #[pallet::storage_version(GEAR_STORAGE_VERSION)]
    #[pallet::without_storage_info]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// User send message to program, which was successfully
        /// added to gear message queue.
        MessageEnqueued {
            /// Generated id of the message.
            id: MessageId,
            /// Account id of the source of the message.
            source: T::AccountId,
            /// Program id, who is a destination of the message.
            destination: ProgramId,
            /// Entry point for processing of the message.
            /// On the sending stage, processing function
            /// of program is always known.
            entry: Entry,
        },

        /// Somebody sent message to user.
        UserMessageSent {
            /// Message sent.
            message: StoredMessage,
            /// Block number of expiration from `Mailbox`.
            ///
            /// Equals `Some(_)` with block number when message
            /// will be removed from `Mailbox` due to some
            /// reasons (see #642, #646 and #1010).
            ///
            /// Equals `None` if message wasn't inserted to
            /// `Mailbox` and appears as only `Event`.
            expiration: Option<T::BlockNumber>,
        },

        /// Message marked as "read" and removes it from `Mailbox`.
        /// This event only affects messages, which were
        /// already inserted in `Mailbox` before.
        UserMessageRead {
            /// Id of the message read.
            id: MessageId,
            /// The reason of the reading (removal from `Mailbox`).
            ///
            /// NOTE: See more docs about reasons at `gear_common::event`.
            reason: UserMessageReadReason,
        },

        /// The result of the messages processing within the block.
        MessagesDispatched {
            /// Total amount of messages removed from message queue.
            total: MessengerCapacityOf<T>,
            /// Execution statuses of the messages, which were already known
            /// by `Event::MessageEnqueued` (sent from user to program).
            statuses: BTreeMap<MessageId, DispatchStatus>,
            /// Ids of programs, which state changed during queue processing.
            state_changes: BTreeSet<ProgramId>,
        },

        /// Temporary `Event` variant, showing that all storages was cleared.
        ///
        /// Will be removed in favor of proper database migrations.
        DatabaseWiped,

        /// Messages execution delayed (waited) and it was successfully
        /// added to gear waitlist.
        MessageWaited {
            /// Id of the message waited.
            id: MessageId,
            /// Origin message id, which started messaging chain with programs,
            /// where currently waited message was created.
            ///
            /// Used for identifying by user, that this message associated
            /// with him and with the concrete initial message.
            origin: Option<MessageId>,
            /// The reason of the waiting (addition to `Waitlist`).
            ///
            /// NOTE: See more docs about reasons at `gear_common::event`.
            reason: MessageWaitedReason,
            /// Block number of expiration from `Waitlist`.
            ///
            /// Equals block number when message will be removed from `Waitlist`
            /// due to some reasons (see #642, #646 and #1010).
            expiration: T::BlockNumber,
        },

        /// Message is ready to continue its execution
        /// and was removed from `Waitlist`.
        MessageWoken {
            /// Id of the message woken.
            id: MessageId,
            /// The reason of the waking (removal from `Waitlist`).
            ///
            /// NOTE: See more docs about reasons at `gear_common::event`.
            reason: MessageWokenReason,
        },

        /// Any data related to programs codes changed.
        CodeChanged {
            /// Id of the code affected.
            id: CodeId,
            /// Change applied on code with current id.
            ///
            /// NOTE: See more docs about change kinds at `gear_common::event`.
            change: CodeChangeKind<T::BlockNumber>,
        },

        /// Any data related to programs changed.
        ProgramChanged {
            /// Id of the program affected.
            id: ProgramId,
            /// Change applied on program with current id.
            ///
            /// NOTE: See more docs about change kinds at `gear_common::event`.
            change: ProgramChangeKind<T::BlockNumber>,
        },
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
        /// Program is terminated.
        ///
        /// Program init ended up with failure, so such message destination is unavailable anymore.
        ProgramIsTerminated,
        /// Message gas tree is not found.
        ///
        /// When message claimed from mailbox has a corrupted or non-extant gas tree associated.
        NoMessageTree,
        /// Code already exists.
        ///
        /// Occurs when trying to save to storage a program code, that has been saved there.
        CodeAlreadyExists,
        /// The code supplied to `submit_code` or `submit_program` exceeds the limit specified in the
        /// current schedule.
        CodeTooLarge,
        /// Failed to create a program.
        FailedToConstructProgram,
        /// Value doesn't cover ExistentialDeposit.
        ValueLessThanMinimal,
        /// Unable to instrument program code.
        GasInstrumentationFailed,
        /// No code could be found at the supplied code hash.
        CodeNotFound,
        /// Messages has alread been replied.
        MessagesAlreadyReplied,
        /// Messages storage corrupted.
        MessagesStorageCorrupted,
        /// User contains mailboxed message from other user.
        UserRepliesToUser,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    where
        T::AccountId: Origin,
    {
        fn on_runtime_upgrade() -> Weight {
            log::debug!(target: "runtime::gear", "⚙️ Runtime upgrade");

            Weight::MAX
        }

        /// Initialization
        fn on_initialize(bn: BlockNumberFor<T>) -> Weight {
            log::debug!(target: "runtime::gear", "⚙️ Initialization of block #{:?}", bn);

            0
        }

        /// Finalization
        fn on_finalize(bn: BlockNumberFor<T>) {
            log::debug!(target: "runtime::gear", "⚙️ Finalization of block #{:?}", bn);
        }

        /// Queue processing occurs after all normal extrinsics in the block
        ///
        /// There should always remain enough weight for this hook to be invoked
        fn on_idle(bn: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
            log::debug!(
                target: "runtime::gear",
                "⚙️ Queue and tasks processing of block #{:?} with weight='{:?}'",
                bn,
                remaining_weight,
            );

            // Adjust the block gas allowance based on actual remaining weight.
            //
            // This field already was affected by gas pallet within the block,
            // so we don't need to include that db write.
            GasAllowanceOf::<T>::put(remaining_weight);

            // Ext manager creation.
            // It will be processing messages execution results following its `JournalHandler` trait implementation.
            // It also will handle delayed tasks following `TasksHandler`.
            let mut ext_manager = Default::default();

            // Processing regular and delayed tasks.
            Self::process_tasks(&mut ext_manager);

            // Processing message queue.
            Self::process_queue(ext_manager);

            // Calculating weight burned within the block.
            let weight = remaining_weight.saturating_sub(GasAllowanceOf::<T>::get() as Weight);

            log::debug!(
                target: "runtime::gear",
                "⚙️ Weight '{:?}' burned in block #{:?}",
                weight,
                bn,
            );

            weight
        }
    }

    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
        /// Submit program for benchmarks which does not check nor instrument the code.
        #[cfg(feature = "runtime-benchmarks")]
        pub fn submit_program_raw(
            origin: OriginFor<T>,
            code: Vec<u8>,
            salt: Vec<u8>,
            init_payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            let schedule = T::Schedule::get();

            let module = wasm_instrument::parity_wasm::deserialize_buffer(&code).map_err(|e| {
                log::debug!("Module failed to load: {:?}", e);
                Error::<T>::FailedToConstructProgram
            })?;

            let code = Code::new_raw(
                code,
                schedule.instruction_weights.version,
                Some(module),
                false,
            )
            .map_err(|e| {
                log::debug!("Code failed to load: {:?}", e);
                Error::<T>::FailedToConstructProgram
            })?;

            let code_and_id = CodeAndId::new(code);

            let packet = InitPacket::new_with_gas(
                code_and_id.code_id(),
                salt,
                init_payload,
                gas_limit,
                value.unique_saturated_into(),
            );

            let program_id = packet.destination();
            // Make sure there is no program with such id in program storage
            ensure!(
                !GearProgramPallet::<T>::program_exists(program_id),
                Error::<T>::ProgramAlreadyExists
            );

            let reserve_fee = T::GasPrice::gas_price(gas_limit);

            // First we reserve enough funds on the account to pay for `gas_limit`
            // and to transfer declared value.
            <T as Config>::Currency::reserve(&who, reserve_fee + value)
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            let origin = who.clone().into_origin();

            let code_id = code_and_id.code_id();

            // By that call we follow the guarantee that we have in `Self::submit_code` -
            // if there's code in storage, there's also metadata for it.
            if let Ok(code_id) = Self::set_code_with_metadata(code_and_id, origin) {
                // TODO: replace this temporary (`None`) value
                // for expiration block number with properly
                // calculated one (issues #646 and #969).
                Self::deposit_event(Event::CodeChanged {
                    id: code_id,
                    change: CodeChangeKind::Active { expiration: None },
                });
            }

            let message_id = Self::next_message_id(origin);

            ExtManager::<T>::default().set_program(program_id, code_id, message_id);

            let _ = GasHandlerOf::<T>::create(
                who.clone(),
                message_id,
                packet.gas_limit().expect("Can't fail"),
            )
            .unwrap_or_else(|e| {
                // # Safty
                //
                // This is unreachable since the `message_id` is new generated
                // with `Self::next_message_id`.
                unreachable!("GasTree corrupted! {:?}", e)
            });

            let message = InitMessage::from_packet(message_id, packet);
            let dispatch = message
                .into_dispatch(ProgramId::from_origin(origin))
                .into_stored();

            QueueOf::<T>::queue(dispatch).map_err(|_| Error::<T>::MessagesStorageCorrupted)?;

            Self::deposit_event(Event::MessageEnqueued {
                id: message_id,
                source: who,
                destination: program_id,
                entry: Entry::Init,
            });

            Ok(().into())
        }

        /// Submit code for benchmarks which does not check nor instrument the code.
        #[cfg(feature = "runtime-benchmarks")]
        pub fn submit_code_raw(origin: OriginFor<T>, code: Vec<u8>) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            let schedule = T::Schedule::get();

            let code = Code::new_raw(code, schedule.instruction_weights.version, None, false)
                .map_err(|e| {
                    log::debug!("Code failed to load: {:?}", e);
                    Error::<T>::FailedToConstructProgram
                })?;

            let code_id = Self::set_code_with_metadata(CodeAndId::new(code), who.into_origin())?;

            // TODO: replace this temporary (`None`) value
            // for expiration block number with properly
            // calculated one (issues #646 and #969).
            Self::deposit_event(Event::CodeChanged {
                id: code_id,
                change: CodeChangeKind::Active { expiration: None },
            });

            Ok(().into())
        }

        #[cfg(not(test))]
        pub fn calculate_gas_info(
            source: H256,
            kind: HandleKind,
            payload: Vec<u8>,
            value: u128,
            allow_other_panics: bool,
            initial_gas: Option<u64>,
        ) -> Result<GasInfo, Vec<u8>> {
            Self::calculate_gas_info_impl(
                source,
                kind,
                initial_gas.unwrap_or_else(BlockGasLimitOf::<T>::get),
                payload,
                value,
                allow_other_panics,
            )
        }

        #[cfg(test)]
        pub fn calculate_gas_info(
            source: H256,
            kind: HandleKind,
            payload: Vec<u8>,
            value: u128,
            allow_other_panics: bool,
        ) -> Result<GasInfo, String> {
            let GasInfo { min_limit, .. } = Self::run_with_ext_copy(|| {
                let initial_gas = BlockGasLimitOf::<T>::get();
                Self::calculate_gas_info_impl(
                    source,
                    kind.clone(),
                    initial_gas,
                    payload.clone(),
                    value,
                    allow_other_panics,
                )
                .map_err(|e| {
                    String::from_utf8(e)
                        .unwrap_or_else(|_| String::from("Failed to parse error to string"))
                })
            })?;

            Self::run_with_ext_copy(|| {
                Self::calculate_gas_info_impl(
                    source,
                    kind,
                    min_limit,
                    payload,
                    value,
                    allow_other_panics,
                )
                .map(
                    |GasInfo {
                         reserved, burned, ..
                     }| GasInfo {
                        min_limit,
                        reserved,
                        burned,
                    },
                )
                .map_err(|e| {
                    String::from_utf8(e)
                        .unwrap_or_else(|_| String::from("Failed to parse error to string"))
                })
            })
        }

        pub fn run_with_ext_copy<R, F: FnOnce() -> R>(f: F) -> R {
            sp_externalities::with_externalities(|ext| {
                ext.storage_start_transaction();
            })
            .expect("externalities should be set");

            let result = f();

            sp_externalities::with_externalities(|ext| {
                ext.storage_rollback_transaction()
                    .expect("transaction was started");
            })
            .expect("externalities should be set");

            result
        }

        fn calculate_gas_info_impl(
            source: H256,
            kind: HandleKind,
            initial_gas: u64,
            payload: Vec<u8>,
            value: u128,
            allow_other_panics: bool,
        ) -> Result<GasInfo, Vec<u8>> {
            let account = <T::AccountId as Origin>::from_origin(source);

            let balance = <T as Config>::Currency::free_balance(&account);
            let max_balance: BalanceOf<T> =
                T::GasPrice::gas_price(initial_gas) + value.unique_saturated_into();
            <T as Config>::Currency::deposit_creating(
                &account,
                max_balance.saturating_sub(balance),
            );

            let who = frame_support::dispatch::RawOrigin::Signed(account);
            let value: BalanceOf<T> = value.unique_saturated_into();

            QueueOf::<T>::clear();

            match kind {
                HandleKind::Init(code) => {
                    let salt = b"calculate_gas_salt".to_vec();
                    Self::submit_program(who.into(), code, salt, payload, initial_gas, value)
                        .map_err(|e| {
                            format!("Internal error: submit_program failed with '{:?}'", e)
                                .into_bytes()
                        })?;
                }
                HandleKind::Handle(destination) => {
                    Self::send_message(who.into(), destination, payload, initial_gas, value)
                        .map_err(|e| {
                            format!("Internal error: send_message failed with '{:?}'", e)
                                .into_bytes()
                        })?;
                }
                HandleKind::Reply(reply_to_id, _exit_code) => {
                    Self::send_reply(who.into(), reply_to_id, payload, initial_gas, value)
                        .map_err(|e| {
                            format!("Internal error: send_reply failed with '{:?}'", e).into_bytes()
                        })?;
                }
            };

            let (main_message_id, main_program_id) = QueueOf::<T>::iter()
                .next()
                .ok_or_else(|| b"Internal error: failed to get last message".to_vec())
                .and_then(|queued| {
                    queued
                        .map_err(|_| b"Internal error: failed to retrieve queued dispatch".to_vec())
                        .map(|dispatch| (dispatch.id(), dispatch.destination()))
                })?;

            let block_info = BlockInfo {
                height: <frame_system::Pallet<T>>::block_number().unique_saturated_into(),
                timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
            };

            let existential_deposit =
                <T as Config>::Currency::minimum_balance().unique_saturated_into();

            let schedule = T::Schedule::get();

            let allocations_config = AllocationsConfig {
                max_pages: gear_core::memory::WasmPageNumber(schedule.limits.memory_pages),
                init_cost: schedule.memory_weights.initial_cost,
                alloc_cost: schedule.memory_weights.allocation_cost,
                mem_grow_cost: schedule.memory_weights.grow_cost,
                load_page_cost: schedule.memory_weights.load_cost,
            };

            let block_config = BlockConfig {
                block_info,
                allocations_config,
                existential_deposit,
                outgoing_limit: T::OutgoingLimit::get(),
                host_fn_weights: schedule.host_fn_weights.into_core(),
                forbidden_funcs: ["gr_gas_available"].into(),
                mailbox_threshold: T::MailboxThreshold::get(),
            };

            let mut min_limit = 0;
            let mut reserved = 0;
            let mut burned = 0;

            let mut ext_manager = ExtManager::<T>::default();

            while let Some(queued_dispatch) =
                QueueOf::<T>::dequeue().map_err(|_| b"MQ storage corrupted".to_vec())?
            {
                let actor_id = queued_dispatch.destination();

                let lazy_pages_enabled =
                    cfg!(feature = "lazy-pages") && lazy_pages::try_to_enable_lazy_pages();

                let actor = ext_manager
                    .get_actor(actor_id, !lazy_pages_enabled)
                    .ok_or_else(|| b"Program not found in the storage".to_vec())?;

                let dispatch_id = queued_dispatch.id();
                let (gas_limit, _) = GasHandlerOf::<T>::get_limit(dispatch_id)
                    .ok()
                    .flatten()
                    .ok_or_else(|| {
                        b"Internal error: unable to get gas limit after execution".to_vec()
                    })?;

                let message_execution_context = MessageExecutionContext {
                    actor,
                    dispatch: queued_dispatch.into_incoming(gas_limit),
                    origin: ProgramId::from_origin(source),
                    gas_allowance: u64::MAX,
                };

                let journal = if lazy_pages_enabled {
                    core_processor::process::<LazyPagesExt, SandboxEnvironment<_>>(
                        &block_config,
                        message_execution_context,
                    )
                } else {
                    core_processor::process::<Ext, SandboxEnvironment<_>>(
                        &block_config,
                        message_execution_context,
                    )
                };

                let get_main_limit = || {
                    GasHandlerOf::<T>::get_limit(main_message_id).map_err(|_| {
                        b"Internal error: unable to get gas limit after execution".to_vec()
                    })
                };

                let get_origin_msg_of = |msg_id| {
                    GasHandlerOf::<T>::get_origin_key(msg_id)
                        .map_err(|_| b"Internal error: unable to get origin key".to_vec())
                        .map(|v| v.unwrap_or(msg_id))
                };

                let from_main_chain =
                    |msg_id| get_origin_msg_of(msg_id).map(|v| v == main_message_id);

                // TODO: Check whether we charge gas fee for submitting code after #646
                for note in journal {
                    core_processor::handle_journal(vec![note.clone()], &mut ext_manager);

                    if let Some((remaining_gas, _)) = get_main_limit()? {
                        min_limit = min_limit.max(initial_gas.saturating_sub(remaining_gas));
                    }

                    match note {
                        JournalNote::SendDispatch { dispatch, .. } => {
                            if from_main_chain(dispatch.id())? {
                                let gas_limit = dispatch
                                    .gas_limit()
                                    .or_else(|| {
                                        GasHandlerOf::<T>::get_limit(dispatch.id())
                                            .ok()
                                            .flatten()
                                            .map(|(g, _)| g)
                                    })
                                    .ok_or_else(|| {
                                        b"Internal error: unable to get gas limit after execution"
                                            .to_vec()
                                    })?;

                                if gas_limit >= T::MailboxThreshold::get() {
                                    reserved = reserved.saturating_add(gas_limit);
                                }
                            }
                        }

                        JournalNote::GasBurned { amount, message_id } => {
                            if from_main_chain(message_id)? {
                                burned = burned.saturating_add(amount);
                            }
                        }

                        JournalNote::MessageDispatched {
                            outcome: CoreDispatchOutcome::MessageTrap { trap, program_id },
                            ..
                        } if program_id == main_program_id || !allow_other_panics => {
                            return Err(
                                format!("Program terminated with a trap: {}", trap).into_bytes()
                            );
                        }

                        _ => (),
                    }
                }
            }

            Ok(GasInfo {
                min_limit,
                reserved,
                burned,
            })
        }

        /// Returns true if a program has been successfully initialized
        pub fn is_initialized(program_id: ProgramId) -> bool {
            common::get_program(program_id.into_origin())
                .map(|p| p.is_initialized())
                .unwrap_or(false)
        }

        /// Returns true if a program has terminated status
        pub fn is_terminated(program_id: ProgramId) -> bool {
            common::get_program(program_id.into_origin())
                .map(|p| p.is_terminated())
                .unwrap_or(false)
        }

        /// Returns MessageId for newly created user message.
        pub fn next_message_id(user_id: H256) -> MessageId {
            let nonce = SentOf::<T>::get();
            SentOf::<T>::increase();
            let block_number = <frame_system::Pallet<T>>::block_number().unique_saturated_into();
            let user_id = ProgramId::from_origin(user_id);

            MessageId::generate_from_user(block_number, user_id, nonce.into())
        }

        /// Delayed tasks processing.
        pub fn process_tasks(ext_manager: &mut ExtManager<T>) {
            // Current block number.
            let bn = <frame_system::Pallet<T>>::block_number();

            // Taking block numbers, where some incomplete tasks held.
            // If there are no such values, we charge for single read, because
            // nothing changing in database, otherwise we delete previous
            // value and charge for single write.
            //
            // We also append current bn to process it together, by iterating
            // over sorted bns set (that's the reason why `BTreeSet` used).
            let (missed_blocks, were_empty) = MissedBlocksOf::<T>::take()
                .map(|mut set| {
                    GasAllowanceOf::<T>::decrease(T::DbWeight::get().writes(1));
                    set.insert(bn);
                    (set, false)
                })
                .unwrap_or_else(|| {
                    GasAllowanceOf::<T>::decrease(T::DbWeight::get().reads(1));
                    ([bn].into(), true)
                });

            // When we had to stop processing due to insufficient gas allowance.
            let mut stopped_at = None;

            // Iterating over blocks.
            for bn in &missed_blocks {
                // Tasks drain iterator.
                let tasks = TaskPoolOf::<T>::drain_prefix_keys(*bn);

                // Checking gas allowance.
                //
                // Making sure we have gas to remove next task
                // or update missed blocks.
                if were_empty {
                    if GasAllowanceOf::<T>::get() <= T::DbWeight::get().writes(2) {
                        stopped_at = Some(*bn);
                        break;
                    }
                } else if GasAllowanceOf::<T>::get() < T::DbWeight::get().writes(2) {
                    stopped_at = Some(*bn);
                    break;
                }

                // Iterating over tasks, scheduled on `bn`.
                for task in tasks {
                    log::debug!("Processing task: {:?}", task);

                    // Decreasing gas allowance due to DB deletion.
                    GasAllowanceOf::<T>::decrease(T::DbWeight::get().writes(1));

                    // Processing task.
                    //
                    // NOTE: Gas allowance decrease should be implemented
                    // inside `TaskHandler` trait and/or inside other
                    // generic types, which interact with storage.
                    task.process_with(ext_manager);

                    // Checking gas allowance.
                    //
                    // Making sure we have gas to remove next task
                    // or update missed blocks.
                    if were_empty {
                        if GasAllowanceOf::<T>::get() <= T::DbWeight::get().writes(2) {
                            stopped_at = Some(*bn);
                            break;
                        }
                    } else if GasAllowanceOf::<T>::get() < T::DbWeight::get().writes(2) {
                        stopped_at = Some(*bn);
                        break;
                    }
                }

                // Stopping iteration over blocks if no resources left.
                if stopped_at.is_some() {
                    break;
                }
            }

            // If we didn't process all tasks and stopped at some block number,
            // then there is new missed blocks set we should store.
            if let Some(stopped_at) = stopped_at {
                // Avoiding `PartialEq` trait bound for `T::BlockNumber`.
                let stopped_at: u32 = stopped_at.unique_saturated_into();

                let actual_missed_blocks = missed_blocks
                    .into_iter()
                    .skip_while(|&x| {
                        // Avoiding `PartialEq` trait bound for `T::BlockNumber`.
                        let x: u32 = x.unique_saturated_into();
                        x != stopped_at
                    })
                    .collect();

                // Charging for inserting into missing blocks,
                // if we were reading it only (they were empty).
                if were_empty {
                    GasAllowanceOf::<T>::decrease(T::DbWeight::get().writes(1));
                }

                MissedBlocksOf::<T>::put(actual_missed_blocks);
            }
        }

        /// Message Queue processing.
        pub fn process_queue(mut ext_manager: ExtManager<T>) {
            let block_info = BlockInfo {
                height: <frame_system::Pallet<T>>::block_number().unique_saturated_into(),
                timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
            };

            let existential_deposit =
                <T as Config>::Currency::minimum_balance().unique_saturated_into();

            let schedule = T::Schedule::get();

            let allocations_config = AllocationsConfig {
                max_pages: gear_core::memory::WasmPageNumber(schedule.limits.memory_pages),
                init_cost: schedule.memory_weights.initial_cost,
                alloc_cost: schedule.memory_weights.allocation_cost,
                mem_grow_cost: schedule.memory_weights.grow_cost,
                load_page_cost: schedule.memory_weights.load_cost,
            };

            let block_config = BlockConfig {
                block_info,
                allocations_config,
                existential_deposit,
                outgoing_limit: T::OutgoingLimit::get(),
                host_fn_weights: schedule.host_fn_weights.into_core(),
                forbidden_funcs: Default::default(),
                mailbox_threshold: T::MailboxThreshold::get(),
            };

            if T::DebugInfo::is_remap_id_enabled() {
                T::DebugInfo::remap_id();
            }

            let lazy_pages_enabled =
                cfg!(feature = "lazy-pages") && lazy_pages::try_to_enable_lazy_pages();

            while QueueProcessingOf::<T>::allowed() {
                if let Some(dispatch) = QueueOf::<T>::dequeue()
                    .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e))
                {
                    // Querying gas limit. Fails in cases of `GasTree` invalidations.
                    let opt_limit = GasHandlerOf::<T>::get_limit(dispatch.id())
                        .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

                    // Gas limit may not be found only for inexistent node.
                    let (gas_limit, _) =
                        opt_limit.unwrap_or_else(|| unreachable!("Non existent GasNode queried"));

                    // Querying external id. Fails in cases of `GasTree` invalidations.
                    let opt_external = GasHandlerOf::<T>::get_external(dispatch.id())
                        .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

                    // External id may not be found only for inexistent node.
                    let external = opt_external
                        .unwrap_or_else(|| unreachable!("Non existent GasNode queried"));

                    log::debug!(
                        "QueueProcessing message: {:?} to {:?} / gas_limit: {}, gas_allowance: {}",
                        dispatch.id(),
                        dispatch.destination(),
                        gas_limit,
                        GasAllowanceOf::<T>::get(),
                    );

                    let active_actor_data = if let Some(maybe_active_program) =
                        common::get_program(dispatch.destination().into_origin())
                    {
                        // Check whether message should be added to the wait list
                        if let Program::Active(prog) = maybe_active_program {
                            let schedule = T::Schedule::get();
                            let code_id = CodeId::from_origin(prog.code_hash);
                            let code = if let Some(code) = T::CodeStorage::get_code(code_id) {
                                if code.instruction_weights_version()
                                    == schedule.instruction_weights.version
                                {
                                    code
                                } else if let Ok(code) = Self::reinstrument_code(code_id, &schedule)
                                {
                                    // todo: charge for code instrumenting
                                    code
                                } else {
                                    // todo: mark code as unable for instrument to skip next time
                                    log::debug!(
                                        "Can not instrument code '{:?}' for program '{:?}'",
                                        code_id,
                                        dispatch.destination()
                                    );
                                    continue;
                                }
                            } else {
                                // This branch is considered unreachable,
                                // because there can't be a program
                                // without code.
                                //
                                // Reaching this code is a sign of a serious
                                // storage or logic corruption.
                                log::error!(
                                    "Code '{:?}' not found for program '{:?}'",
                                    code_id,
                                    dispatch.destination()
                                );

                                continue;
                            };

                            if matches!(prog.state, ProgramState::Uninitialized {message_id} if message_id != dispatch.id())
                                && dispatch.reply().is_none()
                            {
                                // Adding id in on-init wake list.
                                common::waiting_init_append_message_id(
                                    dispatch.destination(),
                                    dispatch.id(),
                                );

                                Self::wait_dispatch(
                                    dispatch,
                                    MessageWaitedSystemReason::ProgramIsNotInitialized
                                        .into_reason(),
                                );
                                continue;
                            }

                            let program = NativeProgram::from_parts(
                                dispatch.destination(),
                                code,
                                prog.allocations,
                                matches!(prog.state, ProgramState::Initialized),
                            );

                            let pages_data = if lazy_pages_enabled {
                                Default::default()
                            } else {
                                match common::get_program_data_for_pages(
                                    dispatch.destination().into_origin(),
                                    prog.pages_with_data.iter(),
                                ) {
                                    Ok(data) => data,
                                    Err(err) => {
                                        log::error!("Cannot get data for program pages: {}", err);
                                        continue;
                                    }
                                }
                            };

                            Some(ExecutableActorData {
                                program,
                                pages_data,
                            })
                        } else {
                            // Reaching this branch is possible when init message was processed with failure, while other kind of messages
                            // were already in the queue/were added to the queue (for example. moved from wait list in case of async init)
                            log::debug!("Program '{:?}' is not active", dispatch.destination());
                            None
                        }
                    } else {
                        // When an actor sends messages, which is intended to be added to the queue
                        // it's destination existence is always checked. The only case this doesn't
                        // happen is when program tries to submit another program with non-existing
                        // code hash. That's the only known case for reaching that branch.
                        //
                        // However there is another case with pausing program, but this API is unstable currently.
                        None
                    };

                    let balance = <T as Config>::Currency::free_balance(
                        &<T::AccountId as Origin>::from_origin(
                            dispatch.destination().into_origin(),
                        ),
                    )
                    .unique_saturated_into();

                    let message_execution_context = MessageExecutionContext {
                        actor: Actor {
                            balance,
                            destination_program: dispatch.destination(),
                            executable_data: active_actor_data,
                        },
                        dispatch: dispatch.into_incoming(gas_limit),
                        origin: ProgramId::from_origin(external.into_origin()),
                        gas_allowance: GasAllowanceOf::<T>::get(),
                    };

                    let journal = if lazy_pages_enabled {
                        core_processor::process::<LazyPagesExt, SandboxEnvironment<_>>(
                            &block_config,
                            message_execution_context,
                        )
                    } else {
                        core_processor::process::<Ext, SandboxEnvironment<_>>(
                            &block_config,
                            message_execution_context,
                        )
                    };

                    core_processor::handle_journal(journal, &mut ext_manager);

                    if T::DebugInfo::is_enabled() {
                        T::DebugInfo::do_snapshot();
                    }

                    if T::DebugInfo::is_remap_id_enabled() {
                        T::DebugInfo::remap_id();
                    }
                } else {
                    break;
                }
            }

            let post_data: QueuePostProcessingData = ext_manager.into();
            let total_handled = DequeuedOf::<T>::get();

            if total_handled > 0 {
                Self::deposit_event(Event::MessagesDispatched {
                    total: total_handled,
                    statuses: post_data.dispatch_statuses,
                    state_changes: post_data.state_changes,
                });
            }
        }

        /// Sets `code` and metadata, if code doesn't exist in storage.
        ///
        /// On success returns Blake256 hash of the `code`. If code already
        /// exists (*so, metadata exists as well*), returns unit `CodeAlreadyExists` error.
        ///
        /// # Note
        /// Code existence in storage means that metadata is there too.
        pub(crate) fn set_code_with_metadata(
            code_and_id: CodeAndId,
            who: H256,
        ) -> Result<CodeId, Error<T>> {
            let code_id = code_and_id.code_id();

            let metadata = {
                let block_number =
                    <frame_system::Pallet<T>>::block_number().unique_saturated_into();
                CodeMetadata::new(who, block_number)
            };

            T::CodeStorage::add_code(code_and_id, metadata)
                .map_err(|_| Error::<T>::CodeAlreadyExists)?;

            Ok(code_id)
        }

        pub(crate) fn reinstrument_code(
            code_id: CodeId,
            schedule: &Schedule<T>,
        ) -> Result<InstrumentedCode, DispatchError> {
            let original_code =
                T::CodeStorage::get_original_code(code_id).ok_or(Error::<T>::CodeNotFound)?;
            let code = Code::try_new(
                original_code,
                schedule.instruction_weights.version,
                |module| schedule.rules(module),
            )
            .map_err(|e| {
                log::debug!("Code failed to load: {:?}", e);
                Error::<T>::FailedToConstructProgram
            })?;

            let code_and_id = CodeAndId::from_parts_unchecked(code, code_id);
            let code_and_id = InstrumentedCodeAndId::from(code_and_id);
            T::CodeStorage::update_code(code_and_id.clone());

            Ok(code_and_id.into_parts().0)
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

            let schedule = T::Schedule::get();

            ensure!(
                code.len() as u32 <= schedule.limits.code_len,
                Error::<T>::CodeTooLarge
            );

            let code = Code::try_new(code, schedule.instruction_weights.version, |module| {
                schedule.rules(module)
            })
            .map_err(|e| {
                log::debug!("Code failed to load: {:?}", e);
                Error::<T>::FailedToConstructProgram
            })?;

            ensure!(
                code.code().len() as u32 <= schedule.limits.code_len,
                Error::<T>::CodeTooLarge
            );

            let code_id = Self::set_code_with_metadata(CodeAndId::new(code), who.into_origin())?;

            // TODO: replace this temporary (`None`) value
            // for expiration block number with properly
            // calculated one (issues #646 and #969).
            Self::deposit_event(Event::CodeChanged {
                id: code_id,
                change: CodeChangeKind::Active { expiration: None },
            });

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
            <T as Config>::WeightInfo::submit_program(code.len() as u32, salt.len() as u32)
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
                gas_limit <= BlockGasLimitOf::<T>::get(),
                Error::<T>::GasLimitTooHigh
            );

            let numeric_value: u128 = value.unique_saturated_into();
            let minimum: u128 = <T as Config>::Currency::minimum_balance().unique_saturated_into();

            // Check that provided `value` equals 0 or greater than existential deposit
            ensure!(
                0 == numeric_value || numeric_value >= minimum,
                Error::<T>::ValueLessThanMinimal
            );

            let schedule = T::Schedule::get();

            ensure!(
                code.len() as u32 <= schedule.limits.code_len,
                Error::<T>::CodeTooLarge
            );

            let code = Code::try_new(code, schedule.instruction_weights.version, |module| {
                schedule.rules(module)
            })
            .map_err(|e| {
                log::debug!("Code failed to load: {:?}", e);
                Error::<T>::FailedToConstructProgram
            })?;

            ensure!(
                code.code().len() as u32 <= schedule.limits.code_len,
                Error::<T>::CodeTooLarge
            );

            let code_and_id = CodeAndId::new(code);

            let packet = InitPacket::new_with_gas(
                code_and_id.code_id(),
                salt,
                init_payload,
                gas_limit,
                value.unique_saturated_into(),
            );

            let program_id = packet.destination();
            // Make sure there is no program with such id in program storage
            ensure!(
                !GearProgramPallet::<T>::program_exists(program_id),
                Error::<T>::ProgramAlreadyExists
            );

            let reserve_fee = T::GasPrice::gas_price(gas_limit);

            // First we reserve enough funds on the account to pay for `gas_limit`
            // and to transfer declared value.
            <T as Config>::Currency::reserve(&who, reserve_fee + value)
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            let origin = who.clone().into_origin();

            let code_id = code_and_id.code_id();

            // By that call we follow the guarantee that we have in `Self::submit_code` -
            // if there's code in storage, there's also metadata for it.
            if let Ok(code_hash) = Self::set_code_with_metadata(code_and_id, origin) {
                // TODO: replace this temporary (`None`) value
                // for expiration block number with properly
                // calculated one (issues #646 and #969).
                Self::deposit_event(Event::CodeChanged {
                    id: code_hash,
                    change: CodeChangeKind::Active { expiration: None },
                });
            }

            let message_id = Self::next_message_id(origin);

            ExtManager::<T>::default().set_program(program_id, code_id, message_id);

            let _ = GasHandlerOf::<T>::create(
                who.clone(),
                message_id,
                packet.gas_limit().expect("Can't fail"),
            )
            .unwrap_or_else(|e| {
                // # Safty
                //
                // This is unreachable since the `message_id is new generated
                // with `Self::next_message_id`.
                unreachable!("GasTree corrupted! {:?}", e)
            });

            let message = InitMessage::from_packet(message_id, packet);
            let dispatch = message
                .into_dispatch(ProgramId::from_origin(origin))
                .into_stored();

            let event = Event::MessageEnqueued {
                id: dispatch.id(),
                source: who,
                destination: dispatch.destination(),
                entry: Entry::Init,
            };

            QueueOf::<T>::queue(dispatch).map_err(|_| Error::<T>::MessagesStorageCorrupted)?;

            Self::deposit_event(event);

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
        #[pallet::weight(<T as Config>::WeightInfo::send_message(payload.len() as u32))]
        pub fn send_message(
            origin: OriginFor<T>,
            destination: ProgramId,
            payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            let origin = who.clone().into_origin();

            let numeric_value: u128 = value.unique_saturated_into();
            let minimum: u128 = <T as Config>::Currency::minimum_balance().unique_saturated_into();

            // Check that provided `gas_limit` value does not exceed the block gas limit
            ensure!(
                gas_limit <= BlockGasLimitOf::<T>::get(),
                Error::<T>::GasLimitTooHigh
            );

            // Check that provided `value` equals 0 or greater than existential deposit
            ensure!(
                0 == numeric_value || numeric_value >= minimum,
                Error::<T>::ValueLessThanMinimal
            );

            let message = HandleMessage::from_packet(
                Self::next_message_id(origin),
                HandlePacket::new_with_gas(
                    destination,
                    payload,
                    gas_limit,
                    value.unique_saturated_into(),
                ),
            );

            if GearProgramPallet::<T>::program_exists(destination) {
                ensure!(
                    !Self::is_terminated(destination),
                    Error::<T>::ProgramIsTerminated
                );

                // Message is not guaranteed to be executed, that's why value is not immediately transferred.
                // That's because destination can fail to be initialized, while this dispatch message is next
                // in the queue.
                <T as Config>::Currency::reserve(&who, value.unique_saturated_into())
                    .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

                let gas_limit_reserve = T::GasPrice::gas_price(gas_limit);

                // First we reserve enough funds on the account to pay for `gas_limit`
                <T as Config>::Currency::reserve(&who, gas_limit_reserve)
                    .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

                let _ = GasHandlerOf::<T>::create(who.clone(), message.id(), gas_limit)
                    .unwrap_or_else(|e| {
                        // # Safty
                        //
                        // This is unreachable since the `message_id` is new generated
                        // with `Self::next_message_id`.
                        unreachable!("GasTree corrupted! {:?}", e)
                    });

                let message = message.into_stored_dispatch(ProgramId::from_origin(origin));

                Self::deposit_event(Event::MessageEnqueued {
                    id: message.id(),
                    source: who,
                    destination: message.destination(),
                    entry: Entry::Handle,
                });

                QueueOf::<T>::queue(message).map_err(|_| Error::<T>::MessagesStorageCorrupted)?;
            } else {
                let message = message.into_stored(ProgramId::from_origin(origin));

                <T as Config>::Currency::transfer(
                    &who,
                    &<T as frame_system::Config>::AccountId::from_origin(
                        message.destination().into_origin(),
                    ),
                    value.unique_saturated_into(),
                    ExistenceRequirement::AllowDeath,
                )
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

                Pallet::<T>::deposit_event(Event::UserMessageSent {
                    message,
                    expiration: None,
                });
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
        #[pallet::weight(<T as Config>::WeightInfo::send_reply(payload.len() as u32))]
        pub fn send_reply(
            origin: OriginFor<T>,
            reply_to_id: MessageId,
            payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            let numeric_value: u128 = value.unique_saturated_into();
            let minimum: u128 = <T as Config>::Currency::minimum_balance().unique_saturated_into();

            // Ensure the `gas_limit` allows the extrinsic to fit into a block
            ensure!(
                gas_limit <= BlockGasLimitOf::<T>::get(),
                Error::<T>::GasLimitTooHigh
            );

            // Check that provided `value` equals 0 or greater than existential deposit
            ensure!(
                0 == numeric_value || numeric_value >= minimum,
                Error::<T>::ValueLessThanMinimal
            );

            // Claim outstanding value from the original message first
            let (original_message, _bn) = MailboxOf::<T>::remove(who.clone(), reply_to_id)?;
            // TODO: burn here for holding #646.
            let mut ext_manager: ExtManager<T> = Default::default();
            ext_manager.message_consumed(reply_to_id);
            let destination = original_message.source();

            // There should be no possibility to modify mailbox if two users interact.
            ensure!(
                GearProgramPallet::<T>::program_exists(destination),
                Error::<T>::UserRepliesToUser
            );

            ensure!(
                !Self::is_terminated(original_message.source()),
                Error::<T>::ProgramIsTerminated
            );

            // Message is not guaranteed to be executed, that's why value is not immediately transferred.
            // That's because destination can fail to be initialized, while this dispatch message is next
            // in the queue.
            <T as Config>::Currency::reserve(&who, value.unique_saturated_into())
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            let origin = who.clone();

            let message_id = MessageId::generate_reply(original_message.id(), 0);
            let packet =
                ReplyPacket::new_with_gas(payload, gas_limit, value.unique_saturated_into());
            let message = ReplyMessage::from_packet(message_id, packet);

            let gas_limit_reserve = T::GasPrice::gas_price(gas_limit);

            // First we reserve enough funds on the account to pay for `gas_limit`
            <T as Config>::Currency::reserve(&who, gas_limit_reserve)
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            let _ = GasHandlerOf::<T>::create(origin.clone(), message_id, gas_limit)
                .map_err(|_| Error::<T>::MessagesAlreadyReplied)?;

            Self::deposit_event(Event::UserMessageRead {
                id: reply_to_id,
                reason: UserMessageReadRuntimeReason::MessageReplied.into_reason(),
            });

            let event = Event::MessageEnqueued {
                id: message.id(),
                source: who,
                destination,
                entry: Entry::Reply(reply_to_id),
            };

            QueueOf::<T>::queue(message.into_stored_dispatch(
                ProgramId::from_origin(origin.into_origin()),
                destination,
                original_message.id(),
            ))
            .map_err(|_| Error::<T>::MessagesStorageCorrupted)?;

            Self::deposit_event(event);

            Ok(().into())
        }

        #[pallet::weight(<T as Config>::WeightInfo::claim_value_from_mailbox())]
        pub fn claim_value_from_mailbox(
            origin: OriginFor<T>,
            message_id: MessageId,
        ) -> DispatchResultWithPostInfo {
            let (_, _bn) = MailboxOf::<T>::remove(ensure_signed(origin)?, message_id)?;
            // TODO: burn here for holding #646.
            let mut ext_manager: ExtManager<T> = Default::default();
            ext_manager.message_consumed(message_id);

            Self::deposit_event(Event::UserMessageRead {
                id: message_id,
                reason: UserMessageReadRuntimeReason::MessageClaimed.into_reason(),
            });

            Ok(().into())
        }

        /// Reset all pallet associated storage.
        #[pallet::weight(0)]
        pub fn reset(origin: OriginFor<T>) -> DispatchResult {
            ensure_root(origin)?;
            <T as Config>::Messenger::reset();
            GearProgramPallet::<T>::reset_storage();
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
            let leftover = <T as Config>::Currency::repatriate_reserved(
                &<T::AccountId as Origin>::from_origin(source),
                dest,
                amount,
                BalanceStatus::Free,
            )?;

            if leftover > 0_u128.unique_saturated_into() {
                log::debug!(
                    target: "essential",
                    "Reserved funds not fully repatriated from {} to 0x{:?} : amount = {:?}, leftover = {:?}",
                    source,
                    dest,
                    amount,
                    leftover,
                );
            }

            Ok(())
        }
    }
}
