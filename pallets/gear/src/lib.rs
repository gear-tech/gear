// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
#![cfg_attr(feature = "runtime-benchmarks", recursion_limit = "1024")]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]
#![doc(html_favicon_url = "https://gear-tech.io/favicons/favicon.ico")]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod internal;
mod queue;
mod runtime_api;
mod schedule;

pub mod manager;
pub mod weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub mod pallet_tests;

pub use crate::{
    manager::{ExtManager, HandleKind},
    pallet::*,
    schedule::{HostFnWeights, InstructionWeights, Limits, MemoryWeights, Schedule},
};
pub use gear_core::gas::GasInfo;
pub use weights::WeightInfo;

use alloc::{format, string::String};
use common::{
    self, event::*, gas_provider::GasNodeId, paused_program_storage::SessionId, scheduler::*,
    storage::*, BlockLimiter, CodeMetadata, CodeStorage, GasProvider, GasTree, Origin,
    PausedProgramStorage, Program, ProgramState, ProgramStorage, QueueRunner,
};
use core::marker::PhantomData;
use core_processor::{
    common::{DispatchOutcome as CoreDispatchOutcome, ExecutableActorData, JournalNote},
    configs::{BlockConfig, BlockInfo},
    Ext,
};
use frame_support::{
    dispatch::{DispatchError, DispatchResultWithPostInfo, PostDispatchInfo},
    ensure,
    pallet_prelude::*,
    traits::{ConstBool, Currency, ExistenceRequirement, Get, Randomness, StorageVersion},
    weights::Weight,
};
use frame_system::pallet_prelude::{BlockNumberFor, *};
use gear_core::{
    code::{Code, CodeAndId, CodeError, InstrumentedCode, InstrumentedCodeAndId},
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    message::*,
    percent::Percent,
};
use manager::{CodeInfo, QueuePostProcessingData};
use pallet_gear_voucher::{PrepaidCall, PrepaidCallsDispatcher};
use primitive_types::H256;
use sp_runtime::{
    traits::{Bounded, One, Saturating, UniqueSaturatedInto, Zero},
    SaturatedConversion,
};
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    convert::TryInto,
    prelude::*,
};

pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
pub(crate) type CurrencyOf<T> = <T as pallet_gear_bank::Config>::Currency;
pub(crate) type BalanceOf<T> = <CurrencyOf<T> as Currency<AccountIdOf<T>>>::Balance;
pub(crate) type SentOf<T> = <<T as Config>::Messenger as Messenger>::Sent;
pub(crate) type DbWeightOf<T> = <T as frame_system::Config>::DbWeight;
pub(crate) type DequeuedOf<T> = <<T as Config>::Messenger as Messenger>::Dequeued;
pub(crate) type QueueProcessingOf<T> = <<T as Config>::Messenger as Messenger>::QueueProcessing;
pub(crate) type QueueOf<T> = <<T as Config>::Messenger as Messenger>::Queue;
pub(crate) type MailboxOf<T> = <<T as Config>::Messenger as Messenger>::Mailbox;
pub(crate) type WaitlistOf<T> = <<T as Config>::Messenger as Messenger>::Waitlist;
pub(crate) type MessengerCapacityOf<T> = <<T as Config>::Messenger as Messenger>::Capacity;
pub type TaskPoolOf<T> = <<T as Config>::Scheduler as Scheduler>::TaskPool;
pub(crate) type FirstIncompleteTasksBlockOf<T> =
    <<T as Config>::Scheduler as Scheduler>::FirstIncompleteTasksBlock;
pub(crate) type CostsPerBlockOf<T> = <<T as Config>::Scheduler as Scheduler>::CostsPerBlock;
pub(crate) type SchedulingCostOf<T> = <<T as Config>::Scheduler as Scheduler>::Cost;
pub(crate) type DispatchStashOf<T> = <<T as Config>::Messenger as Messenger>::DispatchStash;
pub type Authorship<T> = pallet_authorship::Pallet<T>;
pub type GasAllowanceOf<T> = <<T as Config>::BlockLimiter as BlockLimiter>::GasAllowance;
pub type GasHandlerOf<T> = <<T as Config>::GasProvider as GasProvider>::GasTree;
pub type GasNodeIdOf<T> = <GasHandlerOf<T> as GasTree>::NodeId;
pub type BlockGasLimitOf<T> = <<T as Config>::BlockLimiter as BlockLimiter>::BlockGasLimit;
pub type GasBalanceOf<T> = <<T as Config>::GasProvider as GasProvider>::Balance;
pub type ProgramStorageOf<T> = <T as Config>::ProgramStorage;
pub(crate) type GearBank<T> = pallet_gear_bank::Pallet<T>;

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

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::config]
    pub trait Config:
        frame_system::Config
        + pallet_authorship::Config
        + pallet_timestamp::Config
        + pallet_gear_bank::Config
    {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>>
            + TryInto<Event<Self>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The generator used to supply randomness to programs through `seal_random`
        type Randomness: Randomness<Self::Hash, BlockNumberFor<Self>>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;

        /// Cost schedule and limits.
        #[pallet::constant]
        type Schedule: Get<Schedule<Self>>;

        /// The maximum amount of messages that can be produced in single run.
        #[pallet::constant]
        type OutgoingLimit: Get<u32>;

        /// Performance multiplier.
        #[pallet::constant]
        type PerformanceMultiplier: Get<Percent>;

        type DebugInfo: DebugInfo;

        /// Implementation of a storage for program binary codes.
        type CodeStorage: CodeStorage;

        /// Implementation of a storage for programs.
        type ProgramStorage: PausedProgramStorage<
            BlockNumber = BlockNumberFor<Self>,
            Error = DispatchError,
            AccountId = Self::AccountId,
        >;

        /// The minimal gas amount for message to be inserted in mailbox.
        ///
        /// This gas will be consuming as rent for storing and message will be available
        /// for reply or claim, once gas ends, message removes.
        ///
        /// Messages with gas limit less than that minimum will not be added in mailbox,
        /// but will be seen in events.
        #[pallet::constant]
        type MailboxThreshold: Get<u64>;

        /// Amount of reservations can exist for 1 program.
        #[pallet::constant]
        type ReservationsLimit: Get<u64>;

        /// Messenger.
        type Messenger: Messenger<
            BlockNumber = BlockNumberFor<Self>,
            Capacity = u32,
            OutputError = DispatchError,
            MailboxFirstKey = Self::AccountId,
            MailboxSecondKey = MessageId,
            MailboxedMessage = UserStoredMessage,
            QueuedDispatch = StoredDispatch,
            WaitlistFirstKey = ProgramId,
            WaitlistSecondKey = MessageId,
            WaitlistedMessage = StoredDispatch,
            DispatchStashKey = MessageId,
        >;

        /// Implementation of a ledger to account for gas creation and consumption
        type GasProvider: GasProvider<
            ExternalOrigin = Self::AccountId,
            NodeId = GasNodeId<MessageId, ReservationId>,
            Balance = u64,
            Funds = BalanceOf<Self>,
            Error = DispatchError,
        >;

        /// Block limits.
        type BlockLimiter: BlockLimiter<Balance = GasBalanceOf<Self>>;

        /// Scheduler.
        type Scheduler: Scheduler<
            BlockNumber = BlockNumberFor<Self>,
            Cost = u64,
            Task = ScheduledTask<Self::AccountId>,
        >;

        /// Message Queue processing routing provider.
        type QueueRunner: QueueRunner<Gas = GasBalanceOf<Self>>;

        /// The free of charge period of rent.
        #[pallet::constant]
        type ProgramRentFreePeriod: Get<BlockNumberFor<Self>>;

        /// The minimal amount of blocks to resume.
        #[pallet::constant]
        type ProgramResumeMinimalRentPeriod: Get<BlockNumberFor<Self>>;

        /// The program rent cost per block.
        #[pallet::constant]
        type ProgramRentCostPerBlock: Get<BalanceOf<Self>>;

        /// The amount of blocks for processing resume session.
        #[pallet::constant]
        type ProgramResumeSessionDuration: Get<BlockNumberFor<Self>>;

        /// The flag determines if program rent mechanism enabled.
        #[pallet::constant]
        type ProgramRentEnabled: Get<bool>;

        /// The constant defines value that is added if the program
        /// rent is disabled.
        #[pallet::constant]
        type ProgramRentDisabledDelta: Get<BlockNumberFor<Self>>;
    }

    #[pallet::pallet]
    #[pallet::storage_version(GEAR_STORAGE_VERSION)]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// User sends message to program, which was successfully
        /// added to the Gear message queue.
        MessageQueued {
            /// Generated id of the message.
            id: MessageId,
            /// Account id of the source of the message.
            source: T::AccountId,
            /// Program id, who is the message's destination.
            destination: ProgramId,
            /// Entry point for processing of the message.
            /// On the sending stage, the processing function
            /// of the program is always known.
            entry: MessageEntry,
        },

        /// Somebody sent a message to the user.
        UserMessageSent {
            /// Message sent.
            message: UserMessage,
            /// Block number of expiration from `Mailbox`.
            ///
            /// Equals `Some(_)` with block number when message
            /// will be removed from `Mailbox` due to some
            /// reasons (see #642, #646 and #1010).
            ///
            /// Equals `None` if message wasn't inserted to
            /// `Mailbox` and appears as only `Event`.
            expiration: Option<BlockNumberFor<T>>,
        },

        /// Message marked as "read" and removes it from `Mailbox`.
        /// This event only affects messages that were
        /// already inserted in `Mailbox`.
        UserMessageRead {
            /// Id of the message read.
            id: MessageId,
            /// The reason for the reading (removal from `Mailbox`).
            ///
            /// NOTE: See more docs about reasons at `gear_common::event`.
            reason: UserMessageReadReason,
        },

        /// The result of processing the messages within the block.
        MessagesDispatched {
            /// Total amount of messages removed from message queue.
            total: MessengerCapacityOf<T>,
            /// Execution statuses of the messages, which were already known
            /// by `Event::MessageQueued` (sent from user to program).
            statuses: BTreeMap<MessageId, DispatchStatus>,
            /// Ids of programs, which state changed during queue processing.
            state_changes: BTreeSet<ProgramId>,
        },

        /// Messages execution delayed (waited) and successfully
        /// added to gear waitlist.
        MessageWaited {
            /// Id of the message waited.
            id: MessageId,
            /// Origin message id, which started messaging chain with programs,
            /// where currently waited message was created.
            ///
            /// Used to identify by the user that this message associated
            /// with him and the concrete initial message.
            origin: Option<GasNodeId<MessageId, ReservationId>>,
            /// The reason of the waiting (addition to `Waitlist`).
            ///
            /// NOTE: See more docs about reasons at `gear_common::event`.
            reason: MessageWaitedReason,
            /// Block number of expiration from `Waitlist`.
            ///
            /// Equals block number when message will be removed from `Waitlist`
            /// due to some reasons (see #642, #646 and #1010).
            expiration: BlockNumberFor<T>,
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

        /// Any data related to program codes changed.
        CodeChanged {
            /// Id of the code affected.
            id: CodeId,
            /// Change applied on code with current id.
            ///
            /// NOTE: See more docs about change kinds at `gear_common::event`.
            change: CodeChangeKind<BlockNumberFor<T>>,
        },

        /// Any data related to programs changed.
        ProgramChanged {
            /// Id of the program affected.
            id: ProgramId,
            /// Change applied on program with current id.
            ///
            /// NOTE: See more docs about change kinds at `gear_common::event`.
            change: ProgramChangeKind<BlockNumberFor<T>>,
        },

        /// The pseudo-inherent extrinsic that runs queue processing rolled back or not executed.
        QueueNotProcessed,

        /// Program resume session has been started.
        ProgramResumeSessionStarted {
            /// Id of the session.
            session_id: SessionId,
            /// Owner of the session.
            account_id: T::AccountId,
            /// Id of the program affected.
            program_id: ProgramId,
            /// Block number when the session will be removed if not finished.
            session_end_block: BlockNumberFor<T>,
        },
    }

    // Gear pallet error.
    #[pallet::error]
    pub enum Error<T> {
        /// Message wasn't found in the mailbox.
        MessageNotFound,
        /// Not enough balance to execute an action.
        ///
        /// Usually occurs when the gas_limit specified is such that the origin account can't afford the message.
        InsufficientBalance,
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
        /// Program init failed, so such message destination is no longer unavailable.
        InactiveProgram,
        /// Message gas tree is not found.
        ///
        /// When a message claimed from the mailbox has a corrupted or non-extant gas tree associated.
        NoMessageTree,
        /// Code already exists.
        ///
        /// Occurs when trying to save to storage a program code that has been saved there.
        CodeAlreadyExists,
        /// Code does not exist.
        ///
        /// Occurs when trying to get a program code from storage, that doesn't exist.
        CodeDoesntExist,
        /// The code supplied to `upload_code` or `upload_program` exceeds the limit specified in the
        /// current schedule.
        CodeTooLarge,
        /// Failed to create a program.
        ProgramConstructionFailed,
        /// Value doesn't cover ExistentialDeposit.
        ValueLessThanMinimal,
        /// Message queue processing is disabled.
        MessageQueueProcessingDisabled,
        /// Block count doesn't cover MinimalResumePeriod.
        ResumePeriodLessThanMinimal,
        /// Program with the specified id is not found.
        ProgramNotFound,
        /// Gear::run() already included in current block.
        GearRunAlreadyInBlock,
        /// The program rent logic is disabled.
        ProgramRentDisabled,
    }

    #[cfg(feature = "runtime-benchmarks")]
    #[pallet::storage]
    pub(crate) type BenchmarkStorage<T> = StorageMap<_, Identity, u32, Vec<u8>>;

    /// A flag indicating whether the message queue should be processed at the end of a block
    ///
    /// If not set, the inherent extrinsic that processes the queue will keep throwing an error
    /// thereby making the block builder exclude it from the block.
    #[pallet::storage]
    #[pallet::getter(fn execute_inherent)]
    pub(crate) type ExecuteInherent<T> = StorageValue<_, bool, ValueQuery, ConstBool<true>>;

    /// The current block number being processed.
    ///
    /// It shows block number in which queue is processed.
    /// May be less than system pallet block number if panic occurred previously.
    #[pallet::storage]
    #[pallet::getter(fn block_number)]
    pub(crate) type BlockNumber<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

    impl<T: Config> Get<BlockNumberFor<T>> for Pallet<T> {
        fn get() -> BlockNumberFor<T> {
            Self::block_number()
        }
    }

    /// A guard to prohibit all but the first execution of `pallet_gear::run()` call in a block.
    ///
    /// Set to `Some(())` if the extrinsic is executed for the first time in a block.
    /// All subsequent attempts would fail with `Error::<T>::GearRunAlreadyInBlock` error.
    /// Set back to `None` in the `on_finalize()` hook at the end of the block.
    #[pallet::storage]
    pub(crate) type GearRunInBlock<T> = StorageValue<_, ()>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    where
        T::AccountId: Origin,
    {
        /// Initialization
        fn on_initialize(bn: BlockNumberFor<T>) -> Weight {
            // Incrementing Gear block number
            BlockNumber::<T>::mutate(|bn| *bn = bn.saturating_add(One::one()));

            log::debug!(target: "gear::runtime", "⚙️  Initialization of block #{bn:?} (gear #{:?})", Self::block_number());

            T::DbWeight::get().writes(1)
        }

        /// Finalization
        fn on_finalize(bn: BlockNumberFor<T>) {
            // Check if the queue has been processed.
            // If not (while the queue processing enabled), fire an event and revert
            // the Gear internal block number increment made in `on_initialize()`.
            if GearRunInBlock::<T>::take().is_none() && Self::execute_inherent() {
                Self::deposit_event(Event::QueueNotProcessed);
                BlockNumber::<T>::mutate(|bn| *bn = bn.saturating_sub(One::one()));
            }

            log::debug!(target: "gear::runtime", "⚙️  Finalization of block #{bn:?} (gear #{:?})", Self::block_number());
        }
    }

    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
        /// Set gear block number.
        ///
        /// For tests only.
        #[cfg(any(feature = "std", feature = "runtime-benchmarks", test))]
        pub fn set_block_number(bn: BlockNumberFor<T>) {
            <BlockNumber<T>>::put(bn);
        }

        /// Upload program to the chain without stack limit injection and
        /// does not make some checks for code.
        #[cfg(feature = "runtime-benchmarks")]
        pub fn upload_program_raw(
            origin: OriginFor<T>,
            code: Vec<u8>,
            salt: Vec<u8>,
            init_payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            use gear_core::code::TryNewCodeConfig;

            let who = ensure_signed(origin)?;

            let code = Code::try_new_mock_const_or_no_rules(
                code,
                true,
                TryNewCodeConfig {
                    // actual version to avoid re-instrumentation
                    version: T::Schedule::get().instruction_weights.version,
                    // some benchmarks have data in user stack memory
                    check_and_canonize_stack_end: false,
                    // without stack end canonization, program has mutable globals.
                    check_mut_global_exports: false,
                    ..Default::default()
                },
            )
            .map_err(|e| {
                log::debug!("Code failed to load: {:?}", e);
                Error::<T>::ProgramConstructionFailed
            })?;

            let code_and_id = CodeAndId::new(code);
            let code_info = CodeInfo::from_code_and_id(&code_and_id);

            let packet = InitPacket::new_from_user(
                code_and_id.code_id(),
                salt.try_into()
                    .map_err(|err: PayloadSizeError| DispatchError::Other(err.into()))?,
                init_payload
                    .try_into()
                    .map_err(|err: PayloadSizeError| DispatchError::Other(err.into()))?,
                gas_limit,
                value.unique_saturated_into(),
            );

            let program_id = packet.destination();
            // Make sure there is no program with such id in program storage
            ensure!(
                !Self::program_exists(program_id),
                Error::<T>::ProgramAlreadyExists
            );

            // First we reserve enough funds on the account to pay for `gas_limit`
            // and to transfer declared value.
            GearBank::<T>::deposit_gas(&who, gas_limit, false)?;
            GearBank::<T>::deposit_value(&who, value, false)?;

            let origin = who.clone().into_origin();

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
            let block_number = Self::block_number();

            ExtManager::<T>::default().set_program(
                program_id,
                &code_info,
                message_id,
                block_number,
            );

            Self::create(
                who.clone(),
                message_id,
                packet.gas_limit().expect("Infallible"),
                false,
            );

            let message = InitMessage::from_packet(message_id, packet);
            let dispatch = message.into_dispatch(origin.cast()).into_stored();

            QueueOf::<T>::queue(dispatch)
                .unwrap_or_else(|e| unreachable!("Messages storage corrupted: {e:?}"));

            Self::deposit_event(Event::MessageQueued {
                id: message_id,
                source: who,
                destination: program_id,
                entry: MessageEntry::Init,
            });

            Ok(().into())
        }

        /// Upload code to the chain without gas and stack limit injection.
        #[cfg(feature = "runtime-benchmarks")]
        pub fn upload_code_raw(origin: OriginFor<T>, code: Vec<u8>) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            let code = Code::try_new_mock_const_or_no_rules(code, false, Default::default())
                .map_err(|e| {
                    log::debug!("Code failed to load: {e:?}");
                    Error::<T>::ProgramConstructionFailed
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

        pub fn read_state_using_wasm(
            program_id: H256,
            payload: Vec<u8>,
            fn_name: Vec<u8>,
            wasm: Vec<u8>,
            argument: Option<Vec<u8>>,
            gas_allowance: Option<u64>,
        ) -> Result<Vec<u8>, Vec<u8>> {
            let fn_name = String::from_utf8(fn_name)
                .map_err(|_| "Non-utf8 function name".as_bytes().to_vec())?;

            Self::read_state_using_wasm_impl(
                program_id.cast(),
                payload,
                fn_name,
                wasm,
                argument,
                gas_allowance,
            )
            .map_err(String::into_bytes)
        }

        pub fn read_state(
            program_id: H256,
            payload: Vec<u8>,
            gas_allowance: Option<u64>,
        ) -> Result<Vec<u8>, Vec<u8>> {
            Self::read_state_impl(program_id.cast(), payload, gas_allowance)
                .map_err(String::into_bytes)
        }

        pub fn read_metahash(
            program_id: H256,
            gas_allowance: Option<u64>,
        ) -> Result<H256, Vec<u8>> {
            Self::read_metahash_impl(program_id.cast(), gas_allowance).map_err(String::into_bytes)
        }

        #[cfg(not(test))]
        pub fn calculate_gas_info(
            source: H256,
            kind: HandleKind,
            payload: Vec<u8>,
            value: u128,
            allow_other_panics: bool,
            initial_gas: Option<u64>,
            gas_allowance: Option<u64>,
        ) -> Result<GasInfo, Vec<u8>> {
            Self::calculate_gas_info_impl(
                source,
                kind,
                initial_gas.unwrap_or_else(BlockGasLimitOf::<T>::get),
                payload,
                value,
                allow_other_panics,
                false,
                gas_allowance,
            )
        }

        #[cfg(test)]
        pub fn calculate_gas_info(
            source: H256,
            kind: HandleKind,
            payload: Vec<u8>,
            value: u128,
            allow_other_panics: bool,
            allow_skip_zero_replies: bool,
        ) -> Result<GasInfo, String> {
            log::debug!("\n===== CALCULATE GAS INFO =====\n");
            log::debug!("\n--- FIRST TRY ---\n");

            let calc_gas = |initial_gas| {
                // `calculate_gas_info_impl` may change `GasAllowanceOf` and `QueueProcessingOf`.
                // We don't wanna this behavior in tests, so restore old gas allowance value
                // after gas calculation.
                let gas_allowance = GasAllowanceOf::<T>::get();
                let queue_processing = QueueProcessingOf::<T>::allowed();
                let res = Self::calculate_gas_info_impl(
                    source,
                    kind.clone(),
                    initial_gas,
                    payload.clone(),
                    value,
                    allow_other_panics,
                    allow_skip_zero_replies,
                    None,
                );
                GasAllowanceOf::<T>::put(gas_allowance);
                if queue_processing {
                    QueueProcessingOf::<T>::allow();
                } else {
                    QueueProcessingOf::<T>::deny();
                }
                res
            };

            let GasInfo {
                min_limit, waited, ..
            } = Self::run_with_ext_copy(|| {
                calc_gas(BlockGasLimitOf::<T>::get()).map_err(|e| {
                    String::from_utf8(e)
                        .unwrap_or_else(|_| String::from("Failed to parse error to string"))
                })
            })?;

            log::debug!("\n--- SECOND TRY ---\n");

            let res = Self::run_with_ext_copy(|| {
                calc_gas(min_limit)
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

        /// Returns true if a program has been successfully initialized
        pub fn is_initialized(program_id: ProgramId) -> bool {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|program| program.is_initialized())
                .unwrap_or(false)
        }

        /// Returns true if id is a program and the program has active status.
        pub fn is_active(program_id: ProgramId) -> bool {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|program| program.is_active())
                .unwrap_or_default()
        }

        /// Returns true if id is a program and the program has terminated status.
        pub fn is_terminated(program_id: ProgramId) -> bool {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|program| program.is_terminated())
                .unwrap_or_default()
        }

        /// Returns true if id is a program and the program has exited status.
        pub fn is_exited(program_id: ProgramId) -> bool {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|program| program.is_exited())
                .unwrap_or_default()
        }

        /// Returns true if there is a program with the specified id (it may be paused).
        pub fn program_exists(program_id: ProgramId) -> bool {
            ProgramStorageOf::<T>::program_exists(program_id)
                || ProgramStorageOf::<T>::paused_program_exists(&program_id)
        }

        /// Returns exit argument of an exited program.
        pub fn exit_inheritor_of(program_id: ProgramId) -> Option<ProgramId> {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|program| {
                    if let Program::Exited(inheritor) = program {
                        Some(inheritor)
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        }

        /// Returns inheritor of terminated (failed it's init) program.
        pub fn termination_inheritor_of(program_id: ProgramId) -> Option<ProgramId> {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|program| {
                    if let Program::Terminated(inheritor) = program {
                        Some(inheritor)
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

            MessageId::generate_from_user(block_number, user_id.cast(), nonce.into())
        }

        /// Delayed tasks processing.
        pub fn process_tasks(ext_manager: &mut ExtManager<T>) {
            // Current block number.
            let current_bn = Self::block_number();

            // Taking the first block number, where some incomplete tasks held.
            // If there is no such value, we charge for single read, because
            // nothing changing in database, otherwise we delete previous
            // value and charge for single write.
            //
            // We also iterate up to current bn (including) to process it together
            let (first_incomplete_block, were_empty) = FirstIncompleteTasksBlockOf::<T>::take()
                .map(|block| {
                    GasAllowanceOf::<T>::decrease(DbWeightOf::<T>::get().writes(1).ref_time());
                    (block, false)
                })
                .unwrap_or_else(|| {
                    GasAllowanceOf::<T>::decrease(DbWeightOf::<T>::get().reads(1).ref_time());
                    (current_bn, true)
                });

            // When we had to stop processing due to insufficient gas allowance.
            let mut stopped_at = None;

            // Iterating over blocks.
            let missing_blocks = (first_incomplete_block.saturated_into::<u64>()
                ..=current_bn.saturated_into())
                .map(|block| block.saturated_into::<BlockNumberFor<T>>());
            for bn in missing_blocks {
                let tasks = TaskPoolOf::<T>::drain_prefix_keys(bn);

                // Checking gas allowance.
                //
                // Making sure we have gas to remove next task
                // or update the first block of incomplete tasks.
                if GasAllowanceOf::<T>::get() <= DbWeightOf::<T>::get().writes(2).ref_time() {
                    stopped_at = Some(bn);
                    log::debug!("Stopping processing tasks at: {stopped_at:?}");
                    break;
                }

                // Iterating over tasks, scheduled on `bn`.
                let mut last_task = None;
                for task in tasks {
                    // Decreasing gas allowance due to DB deletion.
                    GasAllowanceOf::<T>::decrease(DbWeightOf::<T>::get().writes(1).ref_time());

                    // gas required to process task.
                    let max_task_gas = manager::get_maximum_task_gas::<T>(&task);
                    log::debug!("Processing task: {task:?}, max gas = {max_task_gas}");

                    // Checking gas allowance.
                    //
                    // Making sure we have gas to process the current task
                    // and update the first block of incomplete tasks.
                    if GasAllowanceOf::<T>::get().saturating_sub(max_task_gas)
                        <= DbWeightOf::<T>::get().writes(1).ref_time()
                    {
                        // Since the task is not processed write DB cost should be refunded.
                        // In the same time gas allowance should be charged for read DB cost.
                        GasAllowanceOf::<T>::put(
                            GasAllowanceOf::<T>::get()
                                .saturating_add(DbWeightOf::<T>::get().writes(1).ref_time())
                                .saturating_sub(DbWeightOf::<T>::get().reads(1).ref_time()),
                        );

                        last_task = Some(task);
                        log::debug!("Not enough gas to process task at: {bn:?}");

                        break;
                    }

                    // Processing task and update allowance of gas.
                    let task_gas = task.process_with(ext_manager);
                    GasAllowanceOf::<T>::decrease(task_gas);

                    // Check that there is enough gas allowance to query next task and update the first block of incomplete tasks.
                    if GasAllowanceOf::<T>::get()
                        <= DbWeightOf::<T>::get().reads_writes(1, 1).ref_time()
                    {
                        stopped_at = Some(bn);
                        log::debug!("Stopping processing tasks at (read next): {stopped_at:?}");
                        break;
                    }
                }

                if let Some(task) = last_task {
                    stopped_at = Some(bn);

                    // since there is the overlay mechanism we don't need to subtract write cost
                    // from gas allowance on task insertion.
                    GasAllowanceOf::<T>::put(
                        GasAllowanceOf::<T>::get()
                            .saturating_add(DbWeightOf::<T>::get().writes(1).ref_time()),
                    );
                    TaskPoolOf::<T>::add(bn, task)
                        .unwrap_or_else(|e| unreachable!("Scheduling logic invalidated! {:?}", e));
                }

                // Stopping iteration over blocks if no resources left.
                if stopped_at.is_some() {
                    break;
                }
            }

            // If we didn't process all tasks and stopped at some block number,
            // then there are missed blocks set we should handle in next time.
            if let Some(stopped_at) = stopped_at {
                // Charging for inserting into storage of the first block of incomplete tasks,
                // if we were reading it only (they were empty).
                if were_empty {
                    GasAllowanceOf::<T>::decrease(DbWeightOf::<T>::get().writes(1).ref_time());
                }

                FirstIncompleteTasksBlockOf::<T>::put(stopped_at);
            }
        }

        pub(crate) fn enable_lazy_pages() {
            let prefix = ProgramStorageOf::<T>::pages_final_prefix();
            if !gear_lazy_pages_interface::try_to_enable_lazy_pages(prefix) {
                unreachable!("By some reasons we cannot run lazy-pages on this machine");
            }
        }

        pub(crate) fn block_config() -> BlockConfig {
            let block_info = BlockInfo {
                height: Self::block_number().unique_saturated_into(),
                timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
            };

            let existential_deposit = CurrencyOf::<T>::minimum_balance().unique_saturated_into();

            let schedule = T::Schedule::get();

            BlockConfig {
                block_info,
                performance_multiplier: T::PerformanceMultiplier::get().into(),
                max_pages: schedule.limits.memory_pages.into(),
                page_costs: schedule.memory_weights.clone().into(),
                existential_deposit,
                outgoing_limit: T::OutgoingLimit::get(),
                host_fn_weights: schedule.host_fn_weights.into_core(),
                forbidden_funcs: Default::default(),
                mailbox_threshold: T::MailboxThreshold::get(),
                waitlist_cost: CostsPerBlockOf::<T>::waitlist(),
                dispatch_hold_cost: CostsPerBlockOf::<T>::dispatch_stash(),
                reserve_for: CostsPerBlockOf::<T>::reserve_for().unique_saturated_into(),
                reservation: CostsPerBlockOf::<T>::reservation().unique_saturated_into(),
                read_cost: DbWeightOf::<T>::get().reads(1).ref_time(),
                write_cost: DbWeightOf::<T>::get().writes(1).ref_time(),
                write_per_byte_cost: schedule.db_write_per_byte.ref_time(),
                read_per_byte_cost: schedule.db_read_per_byte.ref_time(),
                module_instantiation_byte_cost: schedule.module_instantiation_per_byte.ref_time(),
                max_reservations: T::ReservationsLimit::get(),
                code_instrumentation_cost: schedule.code_instrumentation_cost.ref_time(),
                code_instrumentation_byte_cost: schedule.code_instrumentation_byte_cost.ref_time(),
                gas_multiplier: <T as pallet_gear_bank::Config>::GasMultiplier::get().into(),
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
                let block_number = Self::block_number().unique_saturated_into();
                CodeMetadata::new(who, block_number)
            };

            T::CodeStorage::add_code(code_and_id, metadata)
                .map_err(|_| Error::<T>::CodeAlreadyExists)?;

            Ok(code_id)
        }

        /// Re - instruments the code under `code_id` with new gas costs for instructions
        ///
        /// The procedure of re - instrumentation is considered infallible for several reasons:
        /// 1. Once (de)serialized valid wasm module can't fail (de)serialization after inserting new gas costs.
        /// 2. The checked (for expected exports and etc.) structure of the Wasm module remains unchanged.
        /// 3. `gas` calls injection is considered infallible for once instrumented program.
        /// One detail should be mentioned here. The injection can actually fail, if cost for some wasm instruction
        /// is removed. But this case is prevented by the Gear node protocol and checked in backwards compatibility
        /// test (`schedule::tests::instructions_backward_compatibility`)
        pub(crate) fn reinstrument_code(
            code_id: CodeId,
            schedule: &Schedule<T>,
        ) -> Result<InstrumentedCode, CodeError> {
            debug_assert!(T::CodeStorage::get_code(code_id).is_some());

            // By the invariant set in CodeStorage trait, original code can't exist in storage
            // without the instrumented code
            let original_code = T::CodeStorage::get_original_code(code_id).unwrap_or_else(|| unreachable!(
                "Code storage is corrupted: instrumented code with id {:?} exists while original not",
                code_id
            ));

            let code = Code::try_new(
                original_code,
                schedule.instruction_weights.version,
                |module| schedule.rules(module),
                schedule.limits.stack_height,
            )?;

            let code_and_id = CodeAndId::from_parts_unchecked(code, code_id);
            let code_and_id = InstrumentedCodeAndId::from(code_and_id);
            T::CodeStorage::update_code(code_and_id.clone());
            let (code, _) = code_and_id.into_parts();

            Ok(code)
        }

        pub(crate) fn try_new_code(code: Vec<u8>) -> Result<CodeAndId, DispatchError> {
            let schedule = T::Schedule::get();

            ensure!(
                code.len() as u32 <= schedule.limits.code_len,
                Error::<T>::CodeTooLarge
            );

            let code = Code::try_new(
                code,
                schedule.instruction_weights.version,
                |module| schedule.rules(module),
                schedule.limits.stack_height,
            )
            .map_err(|e| {
                log::debug!("Code failed to load: {:?}", e);
                Error::<T>::ProgramConstructionFailed
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
            keep_alive: bool,
        ) -> Result<InitPacket, DispatchError> {
            let packet = InitPacket::new_from_user(
                code_id,
                salt.try_into()
                    .map_err(|err: PayloadSizeError| DispatchError::Other(err.into()))?,
                init_payload
                    .try_into()
                    .map_err(|err: PayloadSizeError| DispatchError::Other(err.into()))?,
                gas_limit,
                value.unique_saturated_into(),
            );

            let program_id = packet.destination();
            // Make sure there is no program with such id in program storage
            ensure!(
                !Self::program_exists(program_id),
                Error::<T>::ProgramAlreadyExists
            );

            // First we reserve enough funds on the account to pay for `gas_limit`
            // and to transfer declared value.
            GearBank::<T>::deposit_gas(&who, gas_limit, keep_alive)?;
            GearBank::<T>::deposit_value(&who, value, keep_alive)?;

            Ok(packet)
        }

        pub(crate) fn do_create_program(
            who: T::AccountId,
            packet: InitPacket,
            code_info: CodeInfo,
        ) -> Result<(), DispatchError> {
            let origin = who.clone().into_origin();

            let message_id = Self::next_message_id(origin);
            let block_number = Self::block_number();

            ExtManager::<T>::default().set_program(
                packet.destination(),
                &code_info,
                message_id,
                block_number,
            );

            let program_id = packet.destination();
            let program_event = Event::ProgramChanged {
                id: program_id,
                change: ProgramChangeKind::ProgramSet {
                    expiration: BlockNumberFor::<T>::max_value(),
                },
            };

            Self::create(
                who.clone(),
                message_id,
                packet.gas_limit().expect("Infallible"),
                false,
            );

            let message = InitMessage::from_packet(message_id, packet);
            let dispatch = message.into_dispatch(origin.cast()).into_stored();

            let event = Event::MessageQueued {
                id: dispatch.id(),
                source: who,
                destination: dispatch.destination(),
                entry: MessageEntry::Init,
            };

            QueueOf::<T>::queue(dispatch)
                .unwrap_or_else(|e| unreachable!("Messages storage corrupted: {e:?}"));

            Self::deposit_event(program_event);
            Self::deposit_event(event);

            Ok(())
        }

        pub fn run_call(max_gas: Option<GasBalanceOf<T>>) -> Call<T> {
            Call::run { max_gas }
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
        #[pallet::call_index(0)]
        #[pallet::weight(
            <T as Config>::WeightInfo::upload_code(code.len() as u32 / 1024)
        )]
        pub fn upload_code(origin: OriginFor<T>, code: Vec<u8>) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            Self::upload_code_impl(who, code)
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
        #[pallet::call_index(1)]
        #[pallet::weight(
            <T as Config>::WeightInfo::upload_program(code.len() as u32 / 1024, salt.len() as u32)
        )]
        pub fn upload_program(
            origin: OriginFor<T>,
            code: Vec<u8>,
            salt: Vec<u8>,
            init_payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
            keep_alive: bool,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            Self::check_gas_limit_and_value(gas_limit, value)?;

            let code_and_id = Self::try_new_code(code)?;
            let code_info = CodeInfo::from_code_and_id(&code_and_id);
            let packet = Self::init_packet(
                who.clone(),
                code_and_id.code_id(),
                salt,
                init_payload,
                gas_limit,
                value,
                keep_alive,
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

            Self::do_create_program(who, packet, code_info)?;

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
        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::create_program(salt.len() as u32))]
        pub fn create_program(
            origin: OriginFor<T>,
            code_id: CodeId,
            salt: Vec<u8>,
            init_payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
            keep_alive: bool,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            // Check if code exists.
            let code = T::CodeStorage::get_code(code_id).ok_or(Error::<T>::CodeDoesntExist)?;

            // Check `gas_limit` and `value`
            Self::check_gas_limit_and_value(gas_limit, value)?;

            // Construct packet.
            let packet = Self::init_packet(
                who.clone(),
                code_id,
                salt,
                init_payload,
                gas_limit,
                value,
                keep_alive,
            )?;

            Self::do_create_program(who, packet, CodeInfo::from_code(&code_id, &code))?;
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
        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::send_message(payload.len() as u32))]
        pub fn send_message(
            origin: OriginFor<T>,
            destination: ProgramId,
            payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
            keep_alive: bool,
        ) -> DispatchResultWithPostInfo {
            // Validating origin.
            let who = ensure_signed(origin)?;

            Self::send_message_impl(
                who,
                destination,
                payload,
                gas_limit,
                value,
                keep_alive,
                None,
            )
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
        #[pallet::call_index(4)]
        #[pallet::weight(<T as Config>::WeightInfo::send_reply(payload.len() as u32))]
        pub fn send_reply(
            origin: OriginFor<T>,
            reply_to_id: MessageId,
            payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
            keep_alive: bool,
        ) -> DispatchResultWithPostInfo {
            // Validating origin.
            let who = ensure_signed(origin)?;

            Self::send_reply_impl(
                who,
                reply_to_id,
                payload,
                gas_limit,
                value,
                keep_alive,
                None,
            )
        }

        /// Claim value from message in `Mailbox`.
        ///
        /// Removes message by given `MessageId` from callers `Mailbox`:
        /// rent funds become free, associated with the message value
        /// transfers from message sender to extrinsic caller.
        ///
        /// NOTE: only user who is destination of the message, can claim value
        /// or reply on the message from mailbox.
        #[pallet::call_index(5)]
        #[pallet::weight(<T as Config>::WeightInfo::claim_value())]
        pub fn claim_value(
            origin: OriginFor<T>,
            message_id: MessageId,
        ) -> DispatchResultWithPostInfo {
            // Validating origin.
            let origin = ensure_signed(origin)?;

            // Reason for reading from mailbox.
            let reason = UserMessageReadRuntimeReason::MessageClaimed.into_reason();

            // Reading message, if found, or failing extrinsic.
            let mailboxed = Self::read_message(origin.clone(), message_id, reason)
                .ok_or(Error::<T>::MessageNotFound)?;

            if Self::is_active(mailboxed.source()) {
                // Creating reply message.
                let message = ReplyMessage::auto(mailboxed.id());

                Self::create(origin.clone(), message.id(), 0, true);

                // Converting reply message into appropriate type for queueing.
                let dispatch =
                    message.into_stored_dispatch(origin.cast(), mailboxed.source(), mailboxed.id());

                // Queueing dispatch.
                QueueOf::<T>::queue(dispatch)
                    .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));
            };

            Ok(().into())
        }

        /// Process message queue
        #[pallet::call_index(6)]
        #[pallet::weight((<T as frame_system::Config>::BlockWeights::get().max_block, DispatchClass::Mandatory))]
        pub fn run(
            origin: OriginFor<T>,
            max_gas: Option<GasBalanceOf<T>>,
        ) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;

            ensure!(
                ExecuteInherent::<T>::get(),
                Error::<T>::MessageQueueProcessingDisabled
            );

            ensure!(
                !GearRunInBlock::<T>::exists(),
                Error::<T>::GearRunAlreadyInBlock
            );
            // The below doesn't create an extra db write, because the value will be "taken"
            // (set to `None`) at the end of the block, therefore, will only exist in the
            // overlay and never be committed to storage.
            GearRunInBlock::<T>::set(Some(()));

            let max_weight = <T as frame_system::Config>::BlockWeights::get().max_block;

            // Subtract extrinsic weight from the current block weight to get used weight in the current block.
            let weight_used = <frame_system::Pallet<T>>::block_weight()
                .total()
                .saturating_sub(max_weight);
            let remaining_weight = max_weight.saturating_sub(weight_used);

            // Remaining weight may exceed the minimum block gas limit set by the Limiter trait.
            let mut adjusted_gas = GasAllowanceOf::<T>::get().max(remaining_weight.ref_time());
            // Gas for queue processing can never exceed the hard limit, if the latter is provided.
            if let Some(max_gas) = max_gas {
                adjusted_gas = adjusted_gas.min(max_gas);
            };

            log::debug!(
                target: "gear::runtime",
                "⚙️  Queue and tasks processing of gear block #{:?} with {adjusted_gas}",
                Self::block_number(),
            );

            let actual_weight = <T as Config>::QueueRunner::run_queue(adjusted_gas);

            log::debug!(
                target: "gear::runtime",
                "⚙️  {} burned in gear block #{:?}",
                actual_weight,
                Self::block_number(),
            );

            Ok(PostDispatchInfo {
                actual_weight: Some(
                    Weight::from_parts(actual_weight, 0)
                        .saturating_add(T::DbWeight::get().writes(1)),
                ),
                pays_fee: Pays::No,
            })
        }

        /// Sets `ExecuteInherent` flag.
        ///
        /// Requires root origin (eventually, will only be set via referendum)
        #[pallet::call_index(7)]
        #[pallet::weight(DbWeightOf::<T>::get().writes(1))]
        pub fn set_execute_inherent(origin: OriginFor<T>, value: bool) -> DispatchResult {
            ensure_root(origin)?;

            log::debug!(target: "gear::runtime", "⚙️  Set ExecuteInherent flag to {}", value);
            ExecuteInherent::<T>::put(value);

            Ok(())
        }
    }

    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
        /// Underlying implementation of `GearPallet::send_message`.
        pub fn send_message_impl(
            origin: AccountIdOf<T>,
            destination: ProgramId,
            payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
            keep_alive: bool,
            gas_sponsor: Option<AccountIdOf<T>>,
        ) -> DispatchResultWithPostInfo {
            let payload = payload
                .try_into()
                .map_err(|err: PayloadSizeError| DispatchError::Other(err.into()))?;

            let who = origin;
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

            if Self::program_exists(destination) {
                ensure!(Self::is_active(destination), Error::<T>::InactiveProgram);

                // Message is not guaranteed to be executed, that's why value is not immediately transferred.
                // That's because destination can fail to be initialized, while this dispatch message is next
                // in the queue.
                // Note: reservation is always made against the user's account regardless whether
                // a voucher exists. The latter can only be used to pay for gas or transaction fee.
                GearBank::<T>::deposit_value(&who, value, keep_alive)?;

                // If voucher or any other prepaid mechanism is not used,
                // gas limit is taken from user's account.
                let gas_sponsor = gas_sponsor.unwrap_or_else(|| who.clone());
                GearBank::<T>::deposit_gas(&gas_sponsor, gas_limit, keep_alive)?;
                Self::create(gas_sponsor, message.id(), gas_limit, false);

                let message = message.into_stored_dispatch(origin.cast());

                Self::deposit_event(Event::MessageQueued {
                    id: message.id(),
                    source: who,
                    destination: message.destination(),
                    entry: MessageEntry::Handle,
                });

                QueueOf::<T>::queue(message)
                    .unwrap_or_else(|e| unreachable!("Messages storage corrupted: {e:?}"));
            } else {
                let message = message.into_stored(origin.cast());
                let message: UserMessage = message
                    .try_into()
                    .unwrap_or_else(|_| unreachable!("Signal message sent to user"));

                let existence_requirement = if keep_alive {
                    ExistenceRequirement::KeepAlive
                } else {
                    ExistenceRequirement::AllowDeath
                };

                CurrencyOf::<T>::transfer(
                    &who,
                    &message.destination().cast(),
                    value.unique_saturated_into(),
                    existence_requirement,
                )?;

                Pallet::<T>::deposit_event(Event::UserMessageSent {
                    message,
                    expiration: None,
                });
            }

            Ok(().into())
        }

        /// Underlying implementation of `GearPallet::send_reply`.
        pub fn send_reply_impl(
            origin: AccountIdOf<T>,
            reply_to_id: MessageId,
            payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
            keep_alive: bool,
            gas_sponsor: Option<AccountIdOf<T>>,
        ) -> DispatchResultWithPostInfo {
            let payload = payload
                .try_into()
                .map_err(|err: PayloadSizeError| DispatchError::Other(err.into()))?;

            // Reason for reading from mailbox.
            let reason = UserMessageReadRuntimeReason::MessageReplied.into_reason();

            // Reading message, if found, or failing extrinsic.
            let mailboxed = Self::read_message(origin.clone(), reply_to_id, reason)
                .ok_or(Error::<T>::MessageNotFound)?;

            Self::check_gas_limit_and_value(gas_limit, value)?;

            let destination = mailboxed.source();

            // Checking that program, origin replies to, is not terminated.
            ensure!(Self::is_active(destination), Error::<T>::InactiveProgram);

            let reply_id = MessageId::generate_reply(mailboxed.id());

            // Set zero gas limit if reply deposit exists.
            let gas_limit = if GasHandlerOf::<T>::exists_and_deposit(reply_id) {
                0
            } else {
                gas_limit
            };

            GearBank::<T>::deposit_value(&origin, value, keep_alive)?;

            // If voucher or any other prepaid mechanism is not used,
            // gas limit is taken from user's account.
            let gas_sponsor = gas_sponsor.unwrap_or_else(|| origin.clone());
            GearBank::<T>::deposit_gas(&gas_sponsor, gas_limit, keep_alive)?;
            Self::create(gas_sponsor, reply_id, gas_limit, true);

            // Creating reply message.
            let message = ReplyMessage::from_packet(
                reply_id,
                ReplyPacket::new_with_gas(payload, gas_limit, value.unique_saturated_into()),
            );

            // Converting reply message into appropriate type for queueing.
            let dispatch =
                message.into_stored_dispatch(origin.clone().cast(), destination, mailboxed.id());

            // Pre-generating appropriate event to avoid dispatch cloning.
            let event = Event::MessageQueued {
                id: dispatch.id(),
                source: origin,
                destination: dispatch.destination(),
                entry: MessageEntry::Reply(mailboxed.id()),
            };

            // Queueing dispatch.
            QueueOf::<T>::queue(dispatch)
                .unwrap_or_else(|e| unreachable!("Message queue corrupted! {:?}", e));

            // Depositing pre-generated event.
            Self::deposit_event(event);

            Ok(().into())
        }

        /// Underlying implementation of `GearPallet::upload_code`.
        pub fn upload_code_impl(
            origin: AccountIdOf<T>,
            code: Vec<u8>,
        ) -> DispatchResultWithPostInfo {
            let code_id =
                Self::set_code_with_metadata(Self::try_new_code(code)?, origin.into_origin())?;

            // TODO: replace this temporary (`None`) value
            // for expiration block number with properly
            // calculated one (issues #646 and #969).
            Self::deposit_event(Event::CodeChanged {
                id: code_id,
                change: CodeChangeKind::Active { expiration: None },
            });

            Ok(().into())
        }
    }

    impl<T: Config> PrepaidCallsDispatcher for Pallet<T>
    where
        T::AccountId: Origin,
    {
        type AccountId = AccountIdOf<T>;
        type Balance = BalanceOf<T>;

        fn weight(call: &PrepaidCall<Self::Balance>) -> Weight {
            match call {
                PrepaidCall::SendMessage { payload, .. } => {
                    <T as Config>::WeightInfo::send_message(payload.len() as u32)
                }
                PrepaidCall::SendReply { payload, .. } => {
                    <T as Config>::WeightInfo::send_reply(payload.len() as u32)
                }
                PrepaidCall::UploadCode { code } => {
                    <T as Config>::WeightInfo::upload_code(code.len() as u32 / 1024)
                }
            }
        }

        fn dispatch(
            account_id: Self::AccountId,
            sponsor_id: Self::AccountId,
            call: PrepaidCall<Self::Balance>,
        ) -> DispatchResultWithPostInfo {
            match call {
                PrepaidCall::SendMessage {
                    destination,
                    payload,
                    gas_limit,
                    value,
                    keep_alive,
                } => Self::send_message_impl(
                    account_id,
                    destination,
                    payload,
                    gas_limit,
                    value,
                    keep_alive,
                    Some(sponsor_id),
                ),
                PrepaidCall::SendReply {
                    reply_to_id,
                    payload,
                    gas_limit,
                    value,
                    keep_alive,
                } => Self::send_reply_impl(
                    account_id,
                    reply_to_id,
                    payload,
                    gas_limit,
                    value,
                    keep_alive,
                    Some(sponsor_id),
                ),
                PrepaidCall::UploadCode { code } => Self::upload_code_impl(account_id, code),
            }
        }
    }

    impl<T: Config> QueueRunner for Pallet<T>
    where
        T::AccountId: Origin,
    {
        type Gas = GasBalanceOf<T>;

        fn run_queue(initial_gas: Self::Gas) -> Self::Gas {
            // Setting adjusted initial gas allowance
            GasAllowanceOf::<T>::put(initial_gas);

            // Ext manager creation.
            // It will be processing messages execution results following its `JournalHandler` trait implementation.
            // It also will handle delayed tasks following `TasksHandler`.
            let mut ext_manager = Default::default();

            // Processing regular and delayed tasks.
            Self::process_tasks(&mut ext_manager);

            // Processing message queue.
            Self::process_queue(ext_manager);

            // Calculating weight burned within the block.
            initial_gas.saturating_sub(GasAllowanceOf::<T>::get())
        }
    }
}
