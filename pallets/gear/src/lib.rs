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

#[cfg(feature = "lazy-pages")]
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
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    message::*,
    program::Program as NativeProgram,
};
use pallet_gear_program::Pallet as GearProgramPallet;
use primitive_types::H256;
use sp_runtime::traits::{Saturating, UniqueSaturatedInto, Zero};
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
pub(crate) type GasBalanceOf<T> = <<T as Config>::GasProvider as GasProvider>::Balance;
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
    /// The value may be returned if a program happens to be executed
    /// the second or next time in a block.
    pub may_be_returned: u64,
    /// Was the message placed into waitlist at the end of calculating.
    ///
    /// This flag shows, that `min_limit` makes sense and have some guarantees
    /// only before insertion into waitlist.
    pub waited: bool,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[cfg(feature = "lazy-pages")]
    pub(crate) use crate::ext::LazyPagesExt as Ext;
    #[cfg(not(feature = "lazy-pages"))]
    pub(crate) use core_processor::Ext;

    #[cfg(feature = "lazy-pages")]
    use gear_lazy_pages_common as lazy_pages;

    use crate::manager::{ExtManager, HandleKind, QueuePostProcessingData};
    use alloc::format;
    use common::{
        self, event::*, gas_provider::GasNodeId, BlockLimiter, CodeMetadata, GasPrice, GasProvider,
        GasTree, Origin, Program, ProgramState,
    };
    use core_processor::{
        common::{Actor, DispatchOutcome as CoreDispatchOutcome, ExecutableActorData, JournalNote},
        configs::{AllocationsConfig, BlockConfig, BlockInfo, MessageExecutionContext},
        PrepareResult,
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
            ReservationKey = ReservationId,
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
        /// Message wasn't found in mailbox.
        MessageNotFound,
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
        InactiveProgram,
        /// Message gas tree is not found.
        ///
        /// When message claimed from mailbox has a corrupted or non-extant gas tree associated.
        NoMessageTree,
        /// Code already exists.
        ///
        /// Occurs when trying to save to storage a program code, that has been saved there.
        CodeAlreadyExists,
        /// Code not exists.
        ///
        /// Occurs when trying to get a program code from storage, that doesn't exist.
        CodeNotExists,
        /// The code supplied to `upload_code` or `upload_program` exceeds the limit specified in the
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
        /// Messages storage corrupted.
        MessagesStorageCorrupted,
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
        pub fn upload_program_raw(
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
            CurrencyOf::<T>::reserve(&who, reserve_fee + value)
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            let origin = who.clone().into_origin();

            let code_id = code_and_id.code_id();

            // By that call we follow the guarantee that we have in `Self::upload_code` -
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

            // # Safety
            //
            // This is unreachable since the `message_id` is new generated
            // with `Self::next_message_id`.
            GasHandlerOf::<T>::create(
                who.clone(),
                message_id,
                packet.gas_limit().expect("Can't fail"),
            )
            .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

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
        pub fn upload_code_raw(origin: OriginFor<T>, code: Vec<u8>) -> DispatchResultWithPostInfo {
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
            log::debug!("\n===== CALCULATE GAS INFO =====\n");
            log::debug!("\n--- FIRST TRY ---\n");

            let GasInfo {
                min_limit, waited, ..
            } = Self::run_with_ext_copy(|| {
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

            log::debug!("\n--- SECOND TRY ---\n");

            let res = Self::run_with_ext_copy(|| {
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
                         reserved,
                         burned,
                         may_be_returned,
                         ..
                     }| GasInfo {
                        min_limit,
                        reserved,
                        burned,
                        may_be_returned,
                        waited,
                    },
                )
                .map_err(|e| {
                    String::from_utf8(e)
                        .unwrap_or_else(|_| String::from("Failed to parse error to string"))
                })
            });

            log::debug!("\n==============================\n");

            res
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

            let balance = CurrencyOf::<T>::free_balance(&account);
            let max_balance: BalanceOf<T> =
                T::GasPrice::gas_price(initial_gas) + value.unique_saturated_into();
            CurrencyOf::<T>::deposit_creating(&account, max_balance.saturating_sub(balance));

            let who = frame_support::dispatch::RawOrigin::Signed(account);
            let value: BalanceOf<T> = value.unique_saturated_into();

            QueueOf::<T>::clear();

            match kind {
                HandleKind::Init(code) => {
                    let salt = b"calculate_gas_salt".to_vec();
                    Self::upload_program(who.into(), code, salt, payload, initial_gas, value)
                        .map_err(|e| {
                            format!("Internal error: upload_program failed with '{:?}'", e)
                                .into_bytes()
                        })?;
                }
                HandleKind::InitByHash(code_id) => {
                    let salt = b"calculate_gas_salt".to_vec();
                    Self::create_program(who.into(), code_id, salt, payload, initial_gas, value)
                        .map_err(|e| {
                            format!("Internal error: create_program failed with '{:?}'", e)
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

            let existential_deposit = CurrencyOf::<T>::minimum_balance().unique_saturated_into();

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
                waitlist_cost: CostsPerBlockOf::<T>::waitlist(),
                reserve_for: CostsPerBlockOf::<T>::reserve_for().unique_saturated_into(),
            };

            let mut min_limit = 0;
            let mut reserved = 0;
            let mut burned = 0;
            let mut may_be_returned = 0;

            let mut ext_manager = ExtManager::<T>::default();

            while let Some(queued_dispatch) =
                QueueOf::<T>::dequeue().map_err(|_| b"MQ storage corrupted".to_vec())?
            {
                let actor_id = queued_dispatch.destination();

                let actor = ext_manager
                    .get_actor(actor_id)
                    .ok_or_else(|| b"Program not found in the storage".to_vec())?;

                let dispatch_id = queued_dispatch.id();
                let gas_limit = GasHandlerOf::<T>::get_limit(dispatch_id)
                    .map_err(|_| b"Internal error: unable to get gas limit".to_vec())?;

                let subsequent_execution = ext_manager.program_pages_loaded(&actor_id);
                let message_execution_context = MessageExecutionContext {
                    actor,
                    dispatch: queued_dispatch.into_incoming(gas_limit),
                    origin: ProgramId::from_origin(source),
                    gas_allowance: u64::MAX,
                    subsequent_execution,
                };

                let may_be_returned_context = (!subsequent_execution
                    && actor_id == main_program_id)
                    .then(|| MessageExecutionContext {
                        subsequent_execution: true,
                        ..message_execution_context.clone()
                    });

                let journal =
                    match core_processor::prepare(&block_config, message_execution_context) {
                        PrepareResult::Ok {
                            context,
                            pages_with_data,
                        } => {
                            #[cfg(feature = "lazy-pages")]
                            let memory_pages = {
                                let _ = pages_with_data;
                                assert!(lazy_pages::try_to_enable_lazy_pages());
                                Default::default()
                            };
                            #[cfg(not(feature = "lazy-pages"))]
                            let memory_pages = match common::get_program_data_for_pages(
                                actor_id.into_origin(),
                                pages_with_data.iter(),
                            ) {
                                Ok(data) => data,
                                Err(err) => {
                                    log::error!(
                                        "Page data in storage is in invalid state: {}",
                                        err
                                    );
                                    continue;
                                }
                            };

                            ext_manager.insert_program_id_loaded_pages(actor_id);

                            may_be_returned += may_be_returned_context
                                .map(|c| {
                                    let burned = match core_processor::prepare(&block_config, c) {
                                        PrepareResult::Ok { context, .. } => {
                                            context.gas_counter().burned()
                                        }
                                        _ => context.gas_counter().burned(),
                                    };

                                    context.gas_counter().burned() - burned
                                })
                                .unwrap_or(0);

                            core_processor::process::<Ext, SandboxEnvironment>(
                                &block_config,
                                context,
                                memory_pages,
                            )
                        }
                        PrepareResult::WontExecute(journal) | PrepareResult::Error(journal) => {
                            journal
                        }
                    };

                let get_main_limit = || GasHandlerOf::<T>::get_limit(main_message_id).ok();

                let get_origin_msg_of = |msg_id| {
                    GasHandlerOf::<T>::get_origin_key(GasNodeId::Node(msg_id))
                        .map_err(|_| b"Internal error: unable to get origin key".to_vec())
                };

                let from_main_chain =
                    |msg_id| get_origin_msg_of(msg_id).map(|v| v == main_message_id);

                // TODO: Check whether we charge gas fee for submitting code after #646
                for note in journal {
                    core_processor::handle_journal(vec![note.clone()], &mut ext_manager);

                    if let Some(remaining_gas) = get_main_limit() {
                        min_limit = min_limit.max(initial_gas.saturating_sub(remaining_gas));
                    }

                    match note {
                        JournalNote::SendDispatch { dispatch, .. } => {
                            let destination =
                                T::AccountId::from_origin(dispatch.destination().into_origin());
                            if MailboxOf::<T>::contains(&destination, &dispatch.id())
                                && from_main_chain(dispatch.id())?
                            {
                                let gas_limit = dispatch
                                    .gas_limit()
                                    .or_else(|| GasHandlerOf::<T>::get_limit(dispatch.id()).ok())
                                    .ok_or_else(|| {
                                        b"Internal error: unable to get gas limit after execution"
                                            .to_vec()
                                    })?;

                                reserved = reserved.saturating_add(gas_limit);
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

            let waited = WaitlistOf::<T>::contains(&main_program_id, &main_message_id);

            Ok(GasInfo {
                min_limit,
                reserved,
                burned,
                may_be_returned,
                waited,
            })
        }

        /// Returns true if a program has been successfully initialized
        pub fn is_initialized(program_id: ProgramId) -> bool {
            common::get_program(program_id.into_origin())
                .map(|p| p.is_initialized())
                .unwrap_or(false)
        }

        /// Returns true if id is a program and the program has active status.
        pub fn is_active(program_id: ProgramId) -> bool {
            common::get_program(program_id.into_origin())
                .map(|p| p.is_active())
                .unwrap_or_default()
        }

        /// Returns true if id is a program and the program has terminated status.
        pub fn is_terminated(program_id: ProgramId) -> bool {
            common::get_program(program_id.into_origin())
                .map(|p| p.is_terminated())
                .unwrap_or_default()
        }

        /// Returns true if id is a program and the program has exited status.
        pub fn is_exited(program_id: ProgramId) -> bool {
            common::get_program(program_id.into_origin())
                .map(|p| p.is_exited())
                .unwrap_or_default()
        }

        /// Returns exit argument of an exited program.
        pub fn exit_inheritor_of(program_id: ProgramId) -> Option<ProgramId> {
            common::get_program(program_id.into_origin())
                .map(|p| {
                    if let Program::Exited(id) = p {
                        Some(id)
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        }

        /// Returns inheritor of terminated (failed it's init) program.
        pub fn termination_inheritor_of(program_id: ProgramId) -> Option<ProgramId> {
            common::get_program(program_id.into_origin())
                .map(|p| {
                    if let Program::Terminated(id) = p {
                        Some(id)
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
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
                if GasAllowanceOf::<T>::get() <= T::DbWeight::get().writes(2) {
                    stopped_at = Some(*bn);
                    log::debug!("Stopping processing tasks at: {stopped_at:?}");
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
                    if GasAllowanceOf::<T>::get() <= T::DbWeight::get().writes(2) {
                        stopped_at = Some(*bn);
                        log::debug!("Stopping processing tasks at: {stopped_at:?}");
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

            let existential_deposit = CurrencyOf::<T>::minimum_balance().unique_saturated_into();

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
                waitlist_cost: CostsPerBlockOf::<T>::waitlist(),
                reserve_for: CostsPerBlockOf::<T>::reserve_for().unique_saturated_into(),
            };

            if T::DebugInfo::is_remap_id_enabled() {
                T::DebugInfo::remap_id();
            }

            while QueueProcessingOf::<T>::allowed() {
                if let Some(dispatch) = QueueOf::<T>::dequeue()
                    .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e))
                {
                    // Querying gas limit. Fails in cases of `GasTree` invalidations.
                    let gas_limit = GasHandlerOf::<T>::get_limit(dispatch.id())
                        .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

                    // Querying external id. Fails in cases of `GasTree` invalidations.
                    let external = GasHandlerOf::<T>::get_external(GasNodeId::Node(dispatch.id()))
                        .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

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
                                    None,
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

                            Some(ExecutableActorData {
                                program,
                                pages_with_data: prog.pages_with_data,
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

                    let balance =
                        CurrencyOf::<T>::free_balance(&<T::AccountId as Origin>::from_origin(
                            dispatch.destination().into_origin(),
                        ))
                        .unique_saturated_into();

                    let program_id = dispatch.destination();
                    let message_execution_context = MessageExecutionContext {
                        actor: Actor {
                            balance,
                            destination_program: program_id,
                            executable_data: active_actor_data,
                        },
                        dispatch: dispatch.into_incoming(gas_limit),
                        origin: ProgramId::from_origin(external.into_origin()),
                        gas_allowance: GasAllowanceOf::<T>::get(),
                        subsequent_execution: ext_manager.program_pages_loaded(&program_id),
                    };

                    let journal =
                        match core_processor::prepare(&block_config, message_execution_context) {
                            PrepareResult::Ok {
                                context,
                                pages_with_data,
                            } => {
                                #[cfg(feature = "lazy-pages")]
                                let memory_pages = {
                                    let _ = pages_with_data;
                                    assert!(lazy_pages::try_to_enable_lazy_pages());
                                    Default::default()
                                };
                                #[cfg(not(feature = "lazy-pages"))]
                                let memory_pages = match common::get_program_data_for_pages(
                                    program_id.into_origin(),
                                    pages_with_data.iter(),
                                ) {
                                    Ok(data) => data,
                                    Err(err) => {
                                        log::error!("Cannot get data for program pages: {err}");
                                        continue;
                                    }
                                };

                                ext_manager.insert_program_id_loaded_pages(program_id);

                                core_processor::process::<Ext, SandboxEnvironment>(
                                    &block_config,
                                    context,
                                    memory_pages,
                                )
                            }
                            PrepareResult::WontExecute(journal) | PrepareResult::Error(journal) => {
                                journal
                            }
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

        pub(crate) fn check_code(code: Vec<u8>) -> Result<CodeAndId, DispatchError> {
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

            Ok(CodeAndId::new(code))
        }

        pub(crate) fn check_gas_limit_and_value(
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> Result<(), DispatchError> {
            // Checking that applied gas limit doesn't exceed block limit.
            ensure!(
                gas_limit <= BlockGasLimitOf::<T>::get(),
                Error::<T>::GasLimitTooHigh
            );

            // Checking that applied value fits existence requirements:
            // it should be zero or not less than existential deposit.
            ensure!(
                value.is_zero() || value >= CurrencyOf::<T>::minimum_balance(),
                Error::<T>::ValueLessThanMinimal
            );

            Ok(())
        }

        pub(crate) fn init_packet(
            who: T::AccountId,
            code_id: CodeId,
            salt: Vec<u8>,
            init_payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> Result<InitPacket, DispatchError> {
            let packet = InitPacket::new_with_gas(
                code_id,
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

            Ok(packet)
        }

        pub(crate) fn do_create_program(
            who: T::AccountId,
            packet: InitPacket,
        ) -> Result<(), DispatchError> {
            let origin = who.clone().into_origin();

            let message_id = Self::next_message_id(origin);

            ExtManager::<T>::default().set_program(
                packet.destination(),
                packet.code_id(),
                message_id,
            );

            // # Safety
            //
            // This is unreachable since the `message_id is new generated
            // with `Self::next_message_id`.
            let _ = GasHandlerOf::<T>::create(
                who.clone(),
                message_id,
                packet.gas_limit().expect("Can't fail"),
            );

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

            Ok(())
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
            <T as Config>::WeightInfo::upload_code(code.len() as u32)
        )]
        pub fn upload_code(origin: OriginFor<T>, code: Vec<u8>) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            let code_id = Self::set_code_with_metadata(Self::check_code(code)?, who.into_origin())?;

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
        /// There is the same guarantee here as in `upload_code`. That is, future program's
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
            <T as Config>::WeightInfo::upload_program(code.len() as u32, salt.len() as u32)
        )]
        pub fn upload_program(
            origin: OriginFor<T>,
            code: Vec<u8>,
            salt: Vec<u8>,
            init_payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            Self::check_gas_limit_and_value(gas_limit, value)?;

            let code_and_id = Self::check_code(code)?;
            let packet = Self::init_packet(
                who.clone(),
                code_and_id.code_id(),
                salt,
                init_payload,
                gas_limit,
                value,
            )?;

            if !T::CodeStorage::exists(code_and_id.code_id()) {
                // By that call we follow the guarantee that we have in `Self::upload_code` -
                // if there's code in storage, there's also metadata for it.
                let code_hash =
                    Self::set_code_with_metadata(code_and_id, who.clone().into_origin())?;

                // TODO: replace this temporary (`None`) value
                // for expiration block number with properly
                // calculated one (issues #646 and #969).
                Self::deposit_event(Event::CodeChanged {
                    id: code_hash,
                    change: CodeChangeKind::Active { expiration: None },
                });
            }

            Self::do_create_program(who, packet)?;

            Ok(().into())
        }

        /// Creates program via `code_id` from storage.
        ///
        /// Parameters:
        /// - `code_id`: wasm code id in the code storage.
        /// - `salt`: randomness term (a seed) to allow programs with identical code
        ///   to be created independently.
        /// - `init_payload`: encoded parameters of the wasm module `init` function.
        /// - `gas_limit`: maximum amount of gas the program can spend before it is halted.
        /// - `value`: balance to be transferred to the program once it's been created.
        ///
        /// Emits the following events:
        /// - `InitMessageEnqueued(MessageInfo)` when init message is placed in the queue.
        ///
        /// # NOTE
        ///
        /// For the details of this extrinsic, see `upload_code`.
        #[pallet::weight(<T as Config>::WeightInfo::create_program(salt.len() as u32))]
        pub fn create_program(
            origin: OriginFor<T>,
            code_id: CodeId,
            salt: Vec<u8>,
            init_payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            // Check if code exists.
            if !T::CodeStorage::exists(code_id) {
                return Err(Error::<T>::CodeNotExists.into());
            }

            // Check `gas_limit` and `value`
            Self::check_gas_limit_and_value(gas_limit, value)?;

            // Construct packet.
            let packet =
                Self::init_packet(who.clone(), code_id, salt, init_payload, gas_limit, value)?;

            Self::do_create_program(who, packet)?;
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

            Self::check_gas_limit_and_value(gas_limit, value)?;

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
                ensure!(Self::is_active(destination), Error::<T>::InactiveProgram);

                // Message is not guaranteed to be executed, that's why value is not immediately transferred.
                // That's because destination can fail to be initialized, while this dispatch message is next
                // in the queue.
                CurrencyOf::<T>::reserve(&who, value.unique_saturated_into())
                    .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

                let gas_limit_reserve = T::GasPrice::gas_price(gas_limit);

                // First we reserve enough funds on the account to pay for `gas_limit`
                CurrencyOf::<T>::reserve(&who, gas_limit_reserve)
                    .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

                // # Safety
                //
                // This is unreachable since the `message_id` is new generated
                // with `Self::next_message_id`.
                GasHandlerOf::<T>::create(who.clone(), message.id(), gas_limit)
                    .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

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

                CurrencyOf::<T>::transfer(
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

        /// Send reply on message in `Mailbox`.
        ///
        /// Removes message by given `MessageId` from callers `Mailbox`:
        /// rent funds become free, associated with the message value
        /// transfers from message sender to extrinsic caller.
        ///
        /// Generates reply on removed message with given parameters
        /// and pushes it in `MessageQueue`.
        ///
        /// NOTE: source of the message in mailbox guaranteed to be a program.
        ///
        /// NOTE: only user who is destination of the message, can claim value
        /// or reply on the message from mailbox.
        #[pallet::weight(<T as Config>::WeightInfo::send_reply(payload.len() as u32))]
        pub fn send_reply(
            origin: OriginFor<T>,
            reply_to_id: MessageId,
            payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            // Validating origin.
            let origin = ensure_signed(origin)?;

            Self::check_gas_limit_and_value(gas_limit, value)?;

            // Reason for reading from mailbox.
            let reason = UserMessageReadRuntimeReason::MessageReplied.into_reason();

            // Reading message, if found, or failing extrinsic.
            let mailboxed = Self::read_message(origin.clone(), reply_to_id, reason)
                .ok_or(Error::<T>::MessageNotFound)?;

            // Checking that program, origin replies to, is not terminated.
            ensure!(
                Self::is_active(mailboxed.source()),
                Error::<T>::InactiveProgram
            );

            // Converting applied gas limit into value to reserve.
            let gas_limit_reserve = T::GasPrice::gas_price(gas_limit);

            // Reserving funds for gas limit and value sending.
            //
            // Note, that message is not guaranteed to be successfully executed,
            // that's why value is not immediately transferred.
            CurrencyOf::<T>::reserve(&origin, gas_limit_reserve + value)
                .map_err(|_| Error::<T>::NotEnoughBalanceForReserve)?;

            // Creating reply message.
            let message = ReplyMessage::from_packet(
                MessageId::generate_reply(mailboxed.id(), 0),
                ReplyPacket::new_with_gas(payload, gas_limit, value.unique_saturated_into()),
            );

            // Creating `GasNode` for the reply.
            //
            // # Safety
            //
            //  The error is unreachable since the `message_id` is new generated
            //  from the checked `original_message`."
            GasHandlerOf::<T>::create(origin.clone(), message.id(), gas_limit)
                .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

            // Converting reply message into appropriate type for queueing.
            let dispatch = message.into_stored_dispatch(
                ProgramId::from_origin(origin.clone().into_origin()),
                mailboxed.source(),
                mailboxed.id(),
            );

            // Pre-generating appropriate event to avoid dispatch cloning.
            let event = Event::MessageEnqueued {
                id: dispatch.id(),
                source: origin,
                destination: dispatch.destination(),
                entry: Entry::Reply(mailboxed.id()),
            };

            // Queueing dispatch.
            QueueOf::<T>::queue(dispatch)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));

            // Depositing pre-generated event.
            Self::deposit_event(event);

            Ok(().into())
        }

        /// Claim value from message in `Mailbox`.
        ///
        /// Removes message by given `MessageId` from callers `Mailbox`:
        /// rent funds become free, associated with the message value
        /// transfers from message sender to extrinsic caller.
        ///
        /// NOTE: only user who is destination of the message, can claim value
        /// or reply on the message from mailbox.
        #[pallet::weight(<T as Config>::WeightInfo::claim_value())]
        pub fn claim_value(
            origin: OriginFor<T>,
            message_id: MessageId,
        ) -> DispatchResultWithPostInfo {
            // Reason for reading from mailbox.
            let reason = UserMessageReadRuntimeReason::MessageClaimed.into_reason();

            // Reading message, if found, or failing extrinsic.
            Self::read_message(ensure_signed(origin)?, message_id, reason)
                .ok_or(Error::<T>::MessageNotFound)?;

            Ok(().into())
        }

        /// Reset all pallet associated storage.
        #[pallet::weight(0)]
        pub fn reset(origin: OriginFor<T>) -> DispatchResult {
            ensure_root(origin)?;
            <T as Config>::Scheduler::reset();
            <T as Config>::GasProvider::reset();
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
            let leftover = CurrencyOf::<T>::repatriate_reserved(
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
