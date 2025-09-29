// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
#![allow(clippy::manual_inspect)]
#![allow(clippy::useless_conversion)]
#![doc(html_logo_url = "https://gear-tech.io/logo.png")]
#![doc(html_favicon_url = "https://gear-tech.io/favicon.ico")]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod builtin;
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

pub mod migrations;
pub mod pallet_tests;

pub use crate::{
    builtin::{
        BuiltinDispatcher, BuiltinDispatcherFactory, BuiltinInfo, BuiltinReply, HandleFn, WeightFn,
    },
    manager::{ExtManager, HandleKind},
    pallet::*,
    schedule::{
        DbWeights, InstantiationWeights, InstructionWeights, InstrumentationWeights, Limits,
        MemoryWeights, RentWeights, Schedule, SyscallWeights, TaskWeights,
    },
};
pub use gear_core::rpc::{GasInfo, ReplyInfo};
pub use weights::WeightInfo;

use crate::internal::InheritorForError;
use alloc::{
    format,
    string::{String, ToString},
};
use common::{
    self, BlockLimiter, CodeStorage, GasProvider, GasTree, Origin, Program, ProgramStorage,
    QueueRunner, event::*, gas_provider::GasNodeId, scheduler::*, storage::*,
};
use core::{marker::PhantomData, num::NonZero};
use core_processor::{
    common::{DispatchOutcome as CoreDispatchOutcome, ExecutableActorData, JournalNote},
    configs::{BlockConfig, BlockInfo},
};
#[cfg(feature = "try-runtime")]
use frame_support::storage::{KeyPrefixIterator, storage_prefix};
use frame_support::{
    dispatch::{DispatchResultWithPostInfo, PostDispatchInfo},
    ensure,
    pallet_prelude::*,
    traits::{
        ConstBool, Currency, ExistenceRequirement, Get, LockableCurrency, Randomness,
        StorageVersion, WithdrawReasons, fungible,
        tokens::{Fortitude, Preservation},
    },
    weights::Weight,
};
use frame_system::{
    Pallet as System, RawOrigin,
    pallet_prelude::{BlockNumberFor, *},
};
use gear_core::{
    buffer::*,
    code::{Code, CodeAndId, CodeError, CodeMetadata, InstrumentationStatus, InstrumentedCode},
    env::MessageWaitedType,
    ids::{ActorId, CodeId, MessageId, ReservationId, prelude::*},
    limited::LimitedVecError,
    message::*,
    percent::Percent,
    tasks::VaraScheduledTask,
};
use gear_lazy_pages_common::LazyPagesInterface;
use gear_lazy_pages_interface::LazyPagesRuntimeInterface;
use manager::QueuePostProcessingData;
use pallet_gear_voucher::{PrepaidCall, PrepaidCallsDispatcher, VoucherId, WeightInfo as _};
use primitive_types::H256;
use sp_runtime::{
    DispatchError, SaturatedConversion,
    traits::{Bounded, One, Saturating, UniqueSaturatedInto, Zero},
};
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    convert::TryInto,
    prelude::*,
};

pub type Ext = core_processor::Ext<LazyPagesRuntimeInterface>;

pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
pub(crate) type CurrencyOf<T> = <T as pallet_gear_bank::Config>::Currency;
pub(crate) type BalanceOf<T> = <CurrencyOf<T> as Currency<AccountIdOf<T>>>::Balance;
pub(crate) type SentOf<T> = <<T as Config>::Messenger as Messenger>::Sent;
pub(crate) type DbWeightOf<T> = <T as frame_system::Config>::DbWeight;
pub(crate) type DequeuedOf<T> = <<T as Config>::Messenger as Messenger>::Dequeued;
pub(crate) type PalletInfoOf<T> = <T as frame_system::Config>::PalletInfo;
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

/// Lock for programs on ED.
pub const EXISTENTIAL_DEPOSIT_LOCK_ID: [u8; 8] = *b"glock/ed";

/// The current storage version.
const GEAR_STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use gear_core::code::InstrumentedCodeAndMetadata;

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

        /// The maximum amount of messages that can be produced in during all message executions.
        #[pallet::constant]
        type OutgoingLimit: Get<u32>;

        /// The maximum amount of bytes in outgoing messages during message execution.
        #[pallet::constant]
        type OutgoingBytesLimit: Get<u32>;

        /// Performance multiplier.
        #[pallet::constant]
        type PerformanceMultiplier: Get<Percent>;

        /// Implementation of a storage for program binary codes.
        type CodeStorage: CodeStorage;

        /// Implementation of a storage for programs.
        type ProgramStorage: ProgramStorage<
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
                DelayedDispatch = StoredDelayedDispatch,
                WaitlistFirstKey = ActorId,
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
                Task = VaraScheduledTask<Self::AccountId>,
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

        /// The builtin dispatcher factory.
        type BuiltinDispatcherFactory: BuiltinDispatcherFactory;

        /// The account id of the rent pool if any.
        #[pallet::constant]
        type RentPoolId: Get<Option<AccountIdOf<Self>>>;
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
            destination: ActorId,
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
            state_changes: BTreeSet<ActorId>,
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
            id: ActorId,
            /// Change applied on program with current id.
            ///
            /// NOTE: See more docs about change kinds at `gear_common::event`.
            change: ProgramChangeKind<BlockNumberFor<T>>,
        },

        /// The pseudo-inherent extrinsic that runs queue processing rolled back or not executed.
        QueueNotProcessed,
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
        /// Program is active.
        ActiveProgram,
    }

    #[cfg(feature = "runtime-benchmarks")]
    #[pallet::storage]
    pub(crate) type BenchmarkStorage<T> = StorageMap<_, Identity, u32, Vec<u8>>;

    /// A flag indicating whether the message queue should be processed at the end of a block
    ///
    /// If not set, the inherent extrinsic that processes the queue will keep throwing an error
    /// thereby making the block builder exclude it from the block.
    #[pallet::storage]
    pub(crate) type ExecuteInherent<T> = StorageValue<_, bool, ValueQuery, ConstBool<true>>;

    /// The current block number being processed.
    ///
    /// It shows block number in which queue is processed.
    /// May be less than system pallet block number if panic occurred previously.
    #[pallet::storage]
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
            BlockNumber::<T>::mutate(|bn| {
                *bn = bn.saturating_add(One::one());
            });

            log::debug!(target: "gear::runtime", "⚙️  Initialization of block #{bn:?} (gear #{:?})", Self::block_number());

            T::DbWeight::get().writes(1)
        }

        /// Finalization
        fn on_finalize(bn: BlockNumberFor<T>) {
            // Check if the queue has been processed.
            // If not (while the queue processing enabled), fire an event and revert
            // the Gear internal block number increment made in `on_initialize()`.
            if GearRunInBlock::<T>::take().is_none() && ExecuteInherent::<T>::get() {
                Self::deposit_event(Event::QueueNotProcessed);
                BlockNumber::<T>::mutate(|bn| {
                    *bn = bn.saturating_sub(One::one());
                });
            }

            log::debug!(target: "gear::runtime", "⚙️  Finalization of block #{bn:?} (gear #{:?})", Self::block_number());
        }

        #[cfg(feature = "try-runtime")]
        fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
            use gear_core::code::{ExportError, ImportError, TypeSectionError};

            // Testnet incompatibility codes to be ignored during reinstrumentation check
            #[rustfmt::skip]
            let expected_code_id_errs = [
                // `gr_leave` somehow imported twice
                ("4dd9c141603a668127b98809742cf9f0819d591fe6f44eff63edf2b529a556bd", CodeError::Import(ImportError::DuplicateImport(2))),
                ("90b021503f01db60d0ba00eac970d5d6845f1a757c667232615b5d6c0ff800cc", CodeError::Import(ImportError::DuplicateImport(2))),
                // `handle` entrypoint has invalid signature
                ("e8378f125ec82bb7f399d81b3481d5d62bb5d65749f47fea6cd65f7a48e9c24c", CodeError::Export(ExportError::InvalidExportFnSignature(2))),
                ("d815332c3980386e58d0d191c5161d33824d8a6356a355ccb3528e6428551ab3", CodeError::Export(ExportError::InvalidExportFnSignature(2))),
                // `gr_error` has been removed
                ("10d92d804fc4d42341d5eb2ca04b59e8534fd196621bd3908e1eda0a54f00ab9", CodeError::Import(ImportError::InvalidImportFnSignature(4))),
                ("7ae2b90c96fd65439cd3c72d0c1de985b42400c5ad376d34d1a4cb070191ed2c", CodeError::Import(ImportError::UnknownImport(4))),
                // `delay` argument in `gr_reply` was removed
                ("2477bc4f927a3ae8c3534a824d6c5aec9fa9b0f4747a1f1d4ae5fabbe885b111", CodeError::Import(ImportError::InvalidImportFnSignature(6))),
                ("7daa1b4f3a4891bda3c6b669ca896fa12b83ce4c4e840cf1d88d473a330c35fc", CodeError::Import(ImportError::InvalidImportFnSignature(1))),
                // `gr_pay_program_rent` has been removed
                ("4a0bd89b42de7071a527c13ed52527e941dcda92578585e1139562cdf8a1063e", CodeError::Import(ImportError::UnknownImport(53))),
                ("d483a0e542ad20996b38a2efb1f41e8d863cc1659f1ceb89a79065849fadfeb5", CodeError::Import(ImportError::UnknownImport(53))),
                // `ext_logging_log_version_1` import somehow occurred
                ("75e61ed8f08379ff9ea7f69d542dceabf5f30bfcdf95db55eb6cab77ab3ddb56", CodeError::Import(ImportError::UnknownImport(8))),
                ("164dfe52b1438c7e38d010bc28efc85bd307128859d745e801c9099cbd82bd4f", CodeError::Import(ImportError::UnknownImport(8))),
                ("f92585a339751d7ba9da70a0536936cd8659df29bad777db13e1c7b813c1a301", CodeError::Import(ImportError::UnknownImport(8))),
                // `init` entrypoint has invalid signature
                ("8990159f0730dfed622031af63c453d2bcd5644482cac651796bf229f25d23b6", CodeError::Export(ExportError::InvalidExportFnSignature(0))),
                ("c88b00cfd30d1668ebb50283b4785fd945ac36a4783f8eab39dec2819e06a6c9", CodeError::Import(ImportError::UnknownImport(3))),
                // `init` export directly references `gr_leave` import
                ("ec0cc5d401606415c8ed31bfd347865d19fd277eec7d7bc62c164070eb8c241a", CodeError::Export(ExportError::ExportReferencesToImportFunction(0, 0))),
                // Additional failing codes from latest try-state check
                ("1a52db2c8f26a5a91b887e124fddb0a695e2db1bad36760e7a2a33990ad7829e", CodeError::TypeSection(TypeSectionError::ParametersPerTypeLimitExceeded{limit: 128, actual: 960})),
                ("4372306ca8c3d2d4274852607320eb0f00f0ce61717e30a7ca9ed3d783460072", CodeError::TypeSection(TypeSectionError::ParametersPerTypeLimitExceeded{limit: 128, actual: 256})),
                ("59e2f46b9b24a1514c7a6952ab8926d125b6b939c5bd8d0b27002b652b8b0685", CodeError::TypeSection(TypeSectionError::ParametersPerTypeLimitExceeded{limit: 128, actual: 1000})),
                ("f4f752e6bc9aea8597a1658ed317af546810c7e6d143096919a6d93fd6448340", CodeError::TypeSection(TypeSectionError::ParametersPerTypeLimitExceeded{limit: 128, actual: 896})),
            ]
            .into_iter()
            .map(|(codeid, error)| {
                let mut arr = [0u8; 32];
                hex::decode_to_slice(codeid, &mut arr).unwrap();
                (CodeId::from(arr), error)
            })
            .collect::<BTreeMap<_, _>>();

            // Check that all codes can be instrumented with the current schedule
            let prefix = storage_prefix(b"GearProgram", b"OriginalCodeStorage");
            let schedule = T::Schedule::get();

            let mut total_checked = 0;
            let mut failed_codes: Vec<(CodeId, String)> = Vec::new();
            let mut ignored_count = 0;

            log::info!("Starting try-state code compatibility check");

            for code_id in KeyPrefixIterator::<gear_core::ids::CodeId>::new(
                prefix.to_vec(),
                prefix.to_vec(),
                |key: &[u8]| {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(key);
                    Ok(arr.into())
                },
            ) {
                if let Some(original_code) = T::CodeStorage::get_original_code(code_id) {
                    total_checked += 1;

                    let original_code_len = original_code.len() as u32;
                    if original_code_len > schedule.limits.code_len {
                        let error_message = format!(
                            "original code length {original_code_len} exceeds limit {}",
                            schedule.limits.code_len
                        );
                        log::error!("Code {code_id} instrumentation failed: {error_message}");
                        failed_codes.push((code_id, error_message));
                        continue;
                    }

                    // Try to instrument the code with the current schedule without updating storage
                    if let Err(e) = gear_core::code::Code::try_new(
                        original_code,
                        schedule.instruction_weights.version,
                        |module| schedule.rules(module),
                        schedule.limits.stack_height,
                        schedule.limits.data_segments_amount.into(),
                        schedule.limits.type_section_len.into(),
                        schedule.limits.parameters.into(),
                    ) {
                        if let Some(expected_error) = expected_code_id_errs.get(&code_id) {
                            if expected_error == &e {
                                log::warn!(
                                    "Ignoring incompatible code {code_id} (testnet legacy): {e}"
                                );
                                ignored_count += 1;
                            } else {
                                let error_message = format!("{e} (expected: {expected_error})");
                                log::error!(
                                    "Code {code_id} instrumentation failed with unexpected error: {error_message}"
                                );
                                failed_codes.push((code_id, error_message));
                            }
                        } else {
                            let error_message = e.to_string();
                            log::error!("Code {code_id} instrumentation failed: {error_message}");
                            failed_codes.push((code_id, error_message));
                        }
                    }
                }
            }

            log::info!(
                "Try-state check completed: {total_checked} codes checked, {} failed, {ignored_count} ignored",
                failed_codes.len()
            );

            if !failed_codes.is_empty() {
                log::error!("Failed codes with errors:");
                for (code_id, error) in &failed_codes {
                    log::error!("  Code {code_id}: {error}");
                }
                return Err(sp_runtime::TryRuntimeError::from(
                    "Some codes are not compatible with the current schedule",
                ));
            }

            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        /// Getter for [`BlockNumberFor<T>`] (BlockNumberFor)
        pub(crate) fn block_number() -> BlockNumberFor<T> {
            BlockNumber::<T>::get()
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
            use gear_core::{code::TryNewCodeConfig, gas_metering::CustomConstantCostRules};

            let who = ensure_signed(origin)?;

            let code = Code::try_new_mock_with_rules(
                code,
                |_| CustomConstantCostRules::new(0, 0, 0),
                TryNewCodeConfig {
                    // actual version to avoid re-instrumentation
                    version: T::Schedule::get().instruction_weights.version,
                    // some benchmarks have data in user stack memory
                    // TODO: consider to remove checking data section and stack overlap #3875
                    check_data_section: false,
                    ..Default::default()
                },
            )
            .map_err(|e| {
                log::debug!("Code failed to load: {e:?}");
                Error::<T>::ProgramConstructionFailed
            })?;

            let code_and_id = CodeAndId::new(code);
            let code_id = code_and_id.code_id();

            let packet = InitPacket::new_from_user(
                code_id,
                salt.try_into()
                    .map_err(|err: LimitedVecError| DispatchError::Other(err.as_str()))?,
                init_payload
                    .try_into()
                    .map_err(|err: LimitedVecError| DispatchError::Other(err.as_str()))?,
                gas_limit,
                value.unique_saturated_into(),
            );

            let program_id = packet.destination();
            let (builtins, _) = T::BuiltinDispatcherFactory::create();
            // Make sure there is no program with such id in program storage
            ensure!(
                !Self::program_exists(&builtins, program_id),
                Error::<T>::ProgramAlreadyExists
            );

            let program_account = program_id.cast();
            let ed = CurrencyOf::<T>::minimum_balance();
            CurrencyOf::<T>::transfer(&who, &program_account, ed, ExistenceRequirement::KeepAlive)?;
            CurrencyOf::<T>::set_lock(
                EXISTENTIAL_DEPOSIT_LOCK_ID,
                &program_account,
                ed,
                WithdrawReasons::all(),
            );

            // First we reserve enough funds on the account to pay for `gas_limit`
            // and to transfer declared value.
            GearBank::<T>::deposit_gas(&who, gas_limit, false)?;
            GearBank::<T>::deposit_value(&who, value, false)?;

            // By that call we follow the guarantee that we have in `Self::upload_code` -
            // if there's code in storage, there's also metadata for it.
            if let Ok(code_id) = Self::set_code(code_and_id) {
                // TODO: replace this temporary (`None`) value
                // for expiration block number with properly
                // calculated one (issues #646 and #969).
                Self::deposit_event(Event::CodeChanged {
                    id: code_id,
                    change: CodeChangeKind::Active { expiration: None },
                });
            }

            let origin = who.clone().into_origin();

            let message_id = Self::next_message_id(origin);
            let block_number = Self::block_number();

            ExtManager::<T>::new(builtins).set_program(
                program_id,
                code_id,
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
        pub fn upload_code_raw(code: Vec<u8>) -> DispatchResultWithPostInfo {
            let code = Code::try_new_mock_const_or_no_rules(code, false, Default::default())
                .map_err(|e| {
                    log::debug!("Code failed to load: {e:?}");
                    Error::<T>::ProgramConstructionFailed
                })?;

            let code_id = Self::set_code(CodeAndId::new(code))?;

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
            .map_err(|e| e.into_bytes())
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
            } = Self::run_with_ext_copy(|| calc_gas(BlockGasLimitOf::<T>::get()))?;

            log::debug!("\n--- SECOND TRY ---\n");

            let res = Self::run_with_ext_copy(|| {
                calc_gas(min_limit).map(
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
            });

            log::debug!("\n==============================\n");

            res
        }

        #[cfg(not(test))]
        pub fn calculate_reply_for_handle(
            origin: H256,
            destination: H256,
            payload: Vec<u8>,
            gas_limit: u64,
            value: u128,
            allowance_multiplier: u64,
        ) -> Result<ReplyInfo, Vec<u8>> {
            Self::calculate_reply_for_handle_impl(
                origin,
                destination.cast(),
                payload,
                gas_limit,
                value,
                allowance_multiplier,
            )
            .map_err(|v| v.into_bytes())
        }

        #[cfg(test)]
        pub fn calculate_reply_for_handle(
            origin: AccountIdOf<T>,
            destination: ActorId,
            payload: Vec<u8>,
            gas_limit: u64,
            value: u128,
        ) -> Result<ReplyInfo, String> {
            Self::run_with_ext_copy(|| {
                Self::calculate_reply_for_handle_impl(
                    origin.cast(),
                    destination,
                    payload,
                    gas_limit,
                    value,
                    crate::runtime_api::RUNTIME_API_BLOCK_LIMITS_COUNT,
                )
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

        /// Returns true if a program has been successfully initialized
        pub fn is_initialized(program_id: ActorId) -> bool {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|program| program.is_initialized())
                .unwrap_or(false)
        }

        /// Returns true if `program_id` is that of a in active status or the builtin actor.
        pub fn is_active(builtins: &impl BuiltinDispatcher, program_id: ActorId) -> bool {
            builtins.lookup(&program_id).is_some()
                || ProgramStorageOf::<T>::get_program(program_id)
                    .map(|program| program.is_active())
                    .unwrap_or_default()
        }

        /// Returns true if id is a program and the program has terminated status.
        pub fn is_terminated(program_id: ActorId) -> bool {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|program| program.is_terminated())
                .unwrap_or_default()
        }

        /// Returns true if id is a program and the program has exited status.
        pub fn is_exited(program_id: ActorId) -> bool {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|program| program.is_exited())
                .unwrap_or_default()
        }

        /// Returns true if there is a program with the specified `program_id`` (it may be paused)
        /// or this `program_id` belongs to the built-in actor.
        pub fn program_exists(builtins: &impl BuiltinDispatcher, program_id: ActorId) -> bool {
            builtins.lookup(&program_id).is_some()
                || ProgramStorageOf::<T>::program_exists(program_id)
        }

        /// Returns inheritor of an exited/terminated program.
        pub fn first_inheritor_of(program_id: ActorId) -> Option<ActorId> {
            ProgramStorageOf::<T>::get_program(program_id).and_then(|program| match program {
                Program::Active(_) => None,
                Program::Exited(id) => Some(id),
                Program::Terminated(id) => Some(id),
            })
        }

        /// Returns MessageId for newly created user message.
        pub fn next_message_id(user_id: H256) -> MessageId {
            let nonce = SentOf::<T>::get();
            SentOf::<T>::increase();
            let block_number = System::<T>::block_number().unique_saturated_into();

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
                    TaskPoolOf::<T>::add(bn, task.clone()).unwrap_or_else(|e| {
                        let err_msg = format!(
                            "process_tasks: failed adding not processed last task to task pool. \
                            Bn - {bn:?}, task - {task:?}. Got error - {e:?}"
                        );

                        log::error!("{err_msg}");
                        unreachable!("{err_msg}");
                    });
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
            if !LazyPagesRuntimeInterface::try_to_enable_lazy_pages(prefix) {
                let err_msg =
                    "enable_lazy_pages: By some reasons we cannot run lazy-pages on this machine";

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            }
        }

        pub(crate) fn block_config() -> BlockConfig {
            let block_info = BlockInfo {
                height: Self::block_number().unique_saturated_into(),
                timestamp: <pallet_timestamp::Pallet<T>>::get().unique_saturated_into(),
            };

            let schedule = T::Schedule::get();

            BlockConfig {
                block_info,
                performance_multiplier: T::PerformanceMultiplier::get().into(),
                forbidden_funcs: Default::default(),
                reserve_for: CostsPerBlockOf::<T>::reserve_for().unique_saturated_into(),
                gas_multiplier: <T as pallet_gear_bank::Config>::GasMultiplier::get().into(),
                costs: schedule.process_costs(),
                existential_deposit: CurrencyOf::<T>::minimum_balance().unique_saturated_into(),
                mailbox_threshold: T::MailboxThreshold::get(),
                max_reservations: T::ReservationsLimit::get(),
                max_pages: schedule.limits.memory_pages.into(),
                outgoing_limit: T::OutgoingLimit::get(),
                outgoing_bytes_limit: T::OutgoingBytesLimit::get(),
            }
        }

        /// Sets `code`, if code doesn't exist in storage.
        ///
        /// On success returns Blake256 hash of the `code`. If code already
        /// exists (*so, metadata exists as well*), returns unit `CodeAlreadyExists` error.
        pub(crate) fn set_code(code_and_id: CodeAndId) -> Result<CodeId, Error<T>> {
            let code_id = code_and_id.code_id();

            T::CodeStorage::add_code(code_and_id).map_err(|_| Error::<T>::CodeAlreadyExists)?;

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
            code_metadata: CodeMetadata,
            schedule: &Schedule<T>,
        ) -> Result<InstrumentedCodeAndMetadata, CodeError> {
            // By the invariant set in CodeStorage trait, original code can't exist in storage
            // without the instrumented code
            let original_code = T::CodeStorage::get_original_code(code_id).unwrap_or_else(|| {
                let err_msg = format!(
                    "reinstrument_code: failed to get original code for the existing program. \
                    Code id - '{code_id:?}'."
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            });

            let instrumented_code_and_metadata = match Code::try_new(
                original_code,
                schedule.instruction_weights.version,
                |module| schedule.rules(module),
                schedule.limits.stack_height,
                schedule.limits.data_segments_amount.into(),
                schedule.limits.type_section_len.into(),
                schedule.limits.parameters.into(),
            ) {
                Ok(code) => {
                    let instrumented_code_and_metadata = code.into_instrumented_code_and_metadata();

                    T::CodeStorage::update_instrumented_code_and_metadata(
                        code_id,
                        instrumented_code_and_metadata.clone(),
                    );

                    instrumented_code_and_metadata
                }
                Err(e) => {
                    T::CodeStorage::update_code_metadata(
                        code_id,
                        code_metadata
                            .into_failed_instrumentation(schedule.instruction_weights.version),
                    );

                    return Err(e);
                }
            };

            Ok(instrumented_code_and_metadata)
        }

        pub(crate) fn try_new_code(code: Vec<u8>) -> Result<CodeAndId, DispatchError> {
            let schedule = T::Schedule::get();

            ensure!(
                (code.len() as u32) <= schedule.limits.code_len,
                Error::<T>::CodeTooLarge
            );

            let code = Code::try_new(
                code,
                schedule.instruction_weights.version,
                |module| schedule.rules(module),
                schedule.limits.stack_height,
                schedule.limits.data_segments_amount.into(),
                schedule.limits.type_section_len.into(),
                schedule.limits.parameters.into(),
            )
            .map_err(|e| {
                log::debug!("Code checking or instrumentation failed: {e}");
                Error::<T>::ProgramConstructionFailed
            })?;

            ensure!(
                (code.instrumented_code().bytes().len() as u32) <= schedule.limits.code_len,
                Error::<T>::CodeTooLarge
            );

            Ok(CodeAndId::new(code))
        }

        pub(crate) fn check_gas_limit(gas_limit: u64) -> Result<(), DispatchError> {
            // Checking that applied gas limit doesn't exceed block limit.
            ensure!(
                gas_limit <= BlockGasLimitOf::<T>::get(),
                Error::<T>::GasLimitTooHigh
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
                    .map_err(|err: LimitedVecError| DispatchError::Other(err.as_str()))?,
                init_payload
                    .try_into()
                    .map_err(|err: LimitedVecError| DispatchError::Other(err.as_str()))?,
                gas_limit,
                value.unique_saturated_into(),
            );

            let program_id = packet.destination();
            let (builtins, _) = T::BuiltinDispatcherFactory::create();
            // Make sure there is no program with such id in program storage
            ensure!(
                !Self::program_exists(&builtins, program_id),
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
            code_id: CodeId,
        ) -> Result<(), DispatchError> {
            let origin = who.clone().into_origin();

            let message_id = Self::next_message_id(origin);
            let block_number = Self::block_number();

            let (builtins, _) = T::BuiltinDispatcherFactory::create();
            let ext_manager = ExtManager::<T>::new(builtins);

            let program_id = packet.destination();

            // Before storing the program to `ProgramStorage` we need to make sure that an account
            // can be created for the program.
            // Note: making a transfer outside of the `Ext::set_program()` because here a transfer
            // is allowed to fail (as opposed to creating a program by a program).
            let program_account = program_id.cast();
            let ed = CurrencyOf::<T>::minimum_balance();
            CurrencyOf::<T>::transfer(
                &who,
                &program_account,
                ed,
                ExistenceRequirement::AllowDeath,
            )?;

            // Set lock to avoid accidental account removal by the runtime.
            CurrencyOf::<T>::set_lock(
                EXISTENTIAL_DEPOSIT_LOCK_ID,
                &program_account,
                ed,
                WithdrawReasons::all(),
            );

            ext_manager.set_program(program_id, code_id, message_id, block_number);

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

            QueueOf::<T>::queue(dispatch).unwrap_or_else(|e| {
                let err_msg =
                    format!("do_create_program: failed queuing message. Got error - {e:?}");

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

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
        #[pallet::weight(<T as Config>::WeightInfo::upload_code((code.len() as u32) / 1024))]
        pub fn upload_code(origin: OriginFor<T>, code: Vec<u8>) -> DispatchResultWithPostInfo {
            let _ = ensure_signed(origin)?;

            Self::upload_code_impl(code)
        }

        /// Creates program initialization request (message), that is scheduled to be run in the same block.
        ///
        /// There are no guarantees that initialization message will be run in the same block due to block
        /// gas limit restrictions. For example, when it will be the message's turn, required gas limit for it
        /// could be more than remaining block gas limit. Therefore, the message processing will be postponed
        /// until the next block.
        ///
        /// `ActorId` is computed as Blake256 hash of concatenated bytes of `code` + `salt`. (todo #512 `code_hash` + `salt`)
        /// Such `ActorId` must not exist in the Program Storage at the time of this call.
        ///
        /// There is the same guarantee here as in `upload_code`. That is, future program's
        /// `code` and metadata are stored before message was added to the queue and processed.
        ///
        /// The origin must be Signed and the sender must have sufficient funds to pay
        /// for `gas` and `value` (in case the latter is being transferred).
        ///
        /// Gear runtime guarantees that an active program always has an account to store value.
        /// If the underlying account management platform (e.g. Substrate's System pallet) requires
        /// an existential deposit to keep an account alive, the related overhead is considered an
        /// extra cost related with a program instantiation and is charged to the program's creator
        /// and is released back to the creator when the program is removed.
        /// In context of the above, the `value` parameter represents the so-called `reducible` balance
        /// a program should have at its disposal upon instantiation. It is not used to offset the
        /// existential deposit required for an account creation.
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
            <T as Config>::WeightInfo::upload_program((code.len() as u32) / 1024, salt.len() as u32)
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

            Self::check_gas_limit(gas_limit)?;

            let code_and_id = Self::try_new_code(code)?;
            let code_id = code_and_id.code_id();

            let packet = Self::init_packet(
                who.clone(),
                code_id,
                salt,
                init_payload,
                gas_limit,
                value,
                keep_alive,
            )?;

            if !T::CodeStorage::original_code_exists(code_id) {
                // By that call we follow the guarantee that we have in `Self::upload_code` -
                // if there's code in storage, there's also metadata for it.
                let code_hash = Self::set_code(code_and_id)?;

                // TODO: replace this temporary (`None`) value
                // for expiration block number with properly
                // calculated one (issues #646 and #969).
                Self::deposit_event(Event::CodeChanged {
                    id: code_hash,
                    change: CodeChangeKind::Active { expiration: None },
                });
            }

            Self::do_create_program(who, packet, code_id)?;

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
            ensure!(
                T::CodeStorage::original_code_exists(code_id),
                Error::<T>::CodeDoesntExist
            );

            // Check `gas_limit`
            Self::check_gas_limit(gas_limit)?;

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

            Self::do_create_program(who, packet, code_id)?;
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
            destination: ActorId,
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
                .map_err(|_| Error::<T>::MessageNotFound)?;

            let (builtins, _) = T::BuiltinDispatcherFactory::create();
            if Self::is_active(&builtins, mailboxed.source()) {
                // Creating reply message.
                let message = ReplyMessage::auto(mailboxed.id());

                Self::create(origin.clone(), message.id(), 0, true);

                // Converting reply message into appropriate type for queueing.
                let dispatch =
                    message.into_stored_dispatch(origin.cast(), mailboxed.source(), mailboxed.id());

                // Queueing dispatch.
                QueueOf::<T>::queue(dispatch).unwrap_or_else(|e| {
                    let err_msg = format!("claim_value: failed queuing message. Got error - {e:?}");

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });
            }

            Ok(().into())
        }

        /// Process message queue
        #[pallet::call_index(6)]
        #[pallet::weight((
            <T as frame_system::Config>::BlockWeights::get().max_block,
            DispatchClass::Mandatory,
        ))]
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
            let weight_used = System::<T>::block_weight()
                .total()
                .saturating_sub(max_weight);
            let remaining_weight = max_weight.saturating_sub(weight_used);

            // Remaining weight may exceed the minimum block gas limit set by the Limiter trait.
            let mut adjusted_gas = GasAllowanceOf::<T>::get().max(remaining_weight.ref_time());
            // Gas for queue processing can never exceed the hard limit, if the latter is provided.
            if let Some(max_gas) = max_gas {
                adjusted_gas = adjusted_gas.min(max_gas);
            }

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

            log::debug!(target: "gear::runtime", "⚙️  Set ExecuteInherent flag to {value}");
            ExecuteInherent::<T>::put(value);

            Ok(())
        }

        /// Transfers value from chain of terminated or exited programs to its final inheritor.
        ///
        /// `depth` parameter is how far to traverse to inheritor.
        /// A value of 10 is sufficient for most cases.
        ///
        /// # Example of chain
        ///
        /// - Program #1 exits (e.g `gr_exit syscall) with argument pointing to user.
        /// Balance of program #1 has been sent to user.
        /// - Program #2 exits with inheritor pointing to program #1.
        /// Balance of program #2 has been sent to exited program #1.
        /// - Program #3 exits with inheritor pointing to program #2
        /// Balance of program #1 has been sent to exited program #2.
        ///
        /// So chain of inheritors looks like: Program #3 -> Program #2 -> Program #1 -> User.
        ///
        /// We have programs #1 and #2 with stuck value on their balances.
        /// The balances should've been transferred to user (final inheritor) according to the chain.
        /// But protocol doesn't traverse the chain automatically, so user have to call this extrinsic.
        #[pallet::call_index(8)]
        #[pallet::weight(<T as Config>::WeightInfo::claim_value_to_inheritor(depth.get()))]
        pub fn claim_value_to_inheritor(
            origin: OriginFor<T>,
            program_id: ActorId,
            depth: NonZero<u32>,
        ) -> DispatchResultWithPostInfo {
            ensure_signed(origin)?;

            let depth = depth.try_into().unwrap_or_else(|e| {
                unreachable!("NonZero<u32> to NonZero<usize> conversion must be infallible: {e}")
            });
            let (destination, holders) = match Self::inheritor_for(program_id, depth) {
                Ok(res) => res,
                Err(InheritorForError::Cyclic { holders }) => {
                    // TODO: send value to treasury (#3979)
                    log::debug!("Cyclic inheritor detected for {program_id}");
                    return Ok(Some(<T as Config>::WeightInfo::claim_value_to_inheritor(
                        holders.len() as u32,
                    ))
                    .into());
                }
                Err(InheritorForError::NotFound) => return Err(Error::<T>::ActiveProgram.into()),
            };

            let destination = destination.cast();

            let holders_amount = holders.len();
            for holder in holders {
                // transfer is the same as in `Self::clean_inactive_program` except
                // existential deposit is already unlocked because
                // we work only with terminated/exited programs

                let holder = holder.cast();
                let balance = <CurrencyOf<T> as fungible::Inspect<_>>::reducible_balance(
                    &holder,
                    Preservation::Expendable,
                    Fortitude::Polite,
                );

                if !balance.is_zero() {
                    CurrencyOf::<T>::transfer(
                        &holder,
                        &destination,
                        balance,
                        ExistenceRequirement::AllowDeath,
                    )?;
                }
            }

            Ok(Some(<T as Config>::WeightInfo::claim_value_to_inheritor(
                holders_amount as u32,
            ))
            .into())
        }

        /// A dummy extrinsic with programmatically set weight.
        ///
        /// Used in tests to exhaust block resources.
        ///
        /// Parameters:
        /// - `fraction`: the fraction of the `max_extrinsic` the extrinsic will use.
        #[cfg(feature = "dev")]
        #[pallet::call_index(255)]
        #[pallet::weight({
            if let Some(max) = T::BlockWeights::get().get(DispatchClass::Normal).max_extrinsic {
                *fraction * max
            } else {
                Weight::zero()
            }
        })]
        pub fn exhaust_block_resources(
            origin: OriginFor<T>,
            fraction: sp_runtime::Percent,
        ) -> DispatchResultWithPostInfo {
            let _ = fraction; // We dont need to check the weight witness.
            ensure_root(origin)?;
            Ok(Pays::No.into())
        }
    }

    impl<T: Config> Pallet<T>
    where
        T::AccountId: Origin,
    {
        /// Underlying implementation of `GearPallet::send_message`.
        pub fn send_message_impl(
            origin: AccountIdOf<T>,
            destination: ActorId,
            payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
            keep_alive: bool,
            gas_sponsor: Option<AccountIdOf<T>>,
        ) -> DispatchResultWithPostInfo {
            let payload = payload
                .try_into()
                .map_err(|err: LimitedVecError| DispatchError::Other(err.as_str()))?;

            let who = origin;
            let origin = who.clone().into_origin();

            let message = HandleMessage::from_packet(
                Self::next_message_id(origin),
                HandlePacket::new_with_gas(
                    destination,
                    payload,
                    gas_limit,
                    value.unique_saturated_into(),
                ),
            );

            let (builtins, _) = T::BuiltinDispatcherFactory::create();
            if Self::program_exists(&builtins, destination) {
                ensure!(
                    Self::is_active(&builtins, destination),
                    Error::<T>::InactiveProgram
                );

                Self::check_gas_limit(gas_limit)?;

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

                QueueOf::<T>::queue(message).unwrap_or_else(|e| {
                    let err_msg =
                        format!("send_message_impl: failed queuing message. Got error - {e:?}");

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });
            } else {
                // Take data for the error log
                let message_id = message.id();
                let source = origin.cast::<ActorId>();
                let destination = message.destination();

                let message = message.into_stored(source);
                let message: UserMessage = message
                    .try_into()
                    .unwrap_or_else(|_| {
                        // Signal message sent to user
                        let err_msg = format!(
                            "send_message_impl: failed conversion from stored into user message. \
                            Message id - {message_id}, program id - {source}, destination - {destination}",
                        );

                        log::error!("{err_msg}");
                        unreachable!("{err_msg}")
                    });

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
                .map_err(|err: LimitedVecError| DispatchError::Other(err.as_str()))?;

            // Reason for reading from mailbox.
            let reason = UserMessageReadRuntimeReason::MessageReplied.into_reason();

            // Reading message, if found, or failing extrinsic.
            let mailboxed = Self::read_message(origin.clone(), reply_to_id, reason)
                .map_err(|_| Error::<T>::MessageNotFound)?;

            Self::check_gas_limit(gas_limit)?;

            let destination = mailboxed.source();

            // Checking that program, origin replies to, is not terminated.
            let (builtins, _) = T::BuiltinDispatcherFactory::create();
            ensure!(
                Self::is_active(&builtins, destination),
                Error::<T>::InactiveProgram
            );

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
            QueueOf::<T>::queue(dispatch).unwrap_or_else(|e| {
                let err_msg = format!("send_reply_impl: failed queuing message. Got error - {e:?}");

                log::error!("{err_msg}");
                unreachable!("{err_msg}");
            });

            // Depositing pre-generated event.
            Self::deposit_event(event);

            Ok(().into())
        }

        /// Underlying implementation of `GearPallet::upload_code`.
        pub fn upload_code_impl(code: Vec<u8>) -> DispatchResultWithPostInfo {
            let code_id = Self::set_code(Self::try_new_code(code)?)?;

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

    /// Dispatcher for all types of prepaid calls: gear or gear-voucher pallets.
    pub struct PrepaidCallDispatcher<T: Config + pallet_gear_voucher::Config>(PhantomData<T>);

    impl<T: Config + pallet_gear_voucher::Config> PrepaidCallsDispatcher for PrepaidCallDispatcher<T>
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
                    <T as Config>::WeightInfo::upload_code((code.len() as u32) / 1024)
                }
                PrepaidCall::DeclineVoucher => {
                    <T as pallet_gear_voucher::Config>::WeightInfo::decline()
                }
            }
        }

        fn dispatch(
            account_id: Self::AccountId,
            sponsor_id: Self::AccountId,
            voucher_id: VoucherId,
            call: PrepaidCall<Self::Balance>,
        ) -> DispatchResultWithPostInfo {
            match call {
                PrepaidCall::SendMessage {
                    destination,
                    payload,
                    gas_limit,
                    value,
                    keep_alive,
                } => Pallet::<T>::send_message_impl(
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
                } => Pallet::<T>::send_reply_impl(
                    account_id,
                    reply_to_id,
                    payload,
                    gas_limit,
                    value,
                    keep_alive,
                    Some(sponsor_id),
                ),
                PrepaidCall::UploadCode { code } => Pallet::<T>::upload_code_impl(code),
                PrepaidCall::DeclineVoucher => pallet_gear_voucher::Pallet::<T>::decline(
                    RawOrigin::Signed(account_id).into(),
                    voucher_id,
                ),
            }
        }
    }

    impl<T: Config> QueueRunner for Pallet<T>
    where
        T::AccountId: Origin,
    {
        type Gas = GasBalanceOf<T>;

        fn run_queue(initial_gas: Self::Gas) -> Self::Gas {
            // Create an instance of a builtin dispatcher.
            let (builtin_dispatcher, gas_cost) = T::BuiltinDispatcherFactory::create();

            // Setting initial gas allowance adjusted for builtin dispatcher creation cost.
            GasAllowanceOf::<T>::put(initial_gas.saturating_sub(gas_cost));

            // Ext manager creation.
            // It will be processing messages execution results following its `JournalHandler`
            // trait implementation.
            // It also will handle delayed tasks following `TasksHandler`.
            let mut ext_manager = ExtManager::<T>::new(builtin_dispatcher);

            // Processing regular and delayed tasks.
            Self::process_tasks(&mut ext_manager);

            // Processing message queue.
            Self::process_queue(ext_manager);

            // Calculating weight burned within the block.
            initial_gas.saturating_sub(GasAllowanceOf::<T>::get())
        }
    }
}
