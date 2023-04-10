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
// (issue #2531)
#![allow(deprecated)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(feature = "lazy-pages")]
mod ext;

mod internal;
mod queue;
mod runtime_api;
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

use alloc::{format, string::String};
use common::{
    self, event::*, gas_provider::GasNodeId, scheduler::*, storage::*, BlockLimiter, CodeMetadata,
    CodeStorage, GasPrice, GasProvider, GasTree, Origin, Program, ProgramState, ProgramStorage,
    QueueRunner,
};
use core::marker::PhantomData;
use core_processor::{
    common::{DispatchOutcome as CoreDispatchOutcome, ExecutableActorData, JournalNote},
    configs::{BlockConfig, BlockInfo},
    ContextChargedForInstrumentation,
};
use frame_support::{
    dispatch::{DispatchError, DispatchResultWithPostInfo, PostDispatchInfo},
    ensure,
    pallet_prelude::*,
    traits::{
        BalanceStatus, ConstBool, Currency, ExistenceRequirement, Get, LockableCurrency,
        Randomness, ReservableCurrency, StorageVersion,
    },
    weights::Weight,
};
use frame_system::pallet_prelude::{BlockNumberFor, *};
use gear_core::{
    code::{Code, CodeAndId, InstrumentedCode, InstrumentedCodeAndId},
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    memory::{GearPage, PageBuf},
    message::*,
};
use manager::{CodeInfo, QueuePostProcessingData};
use primitive_types::H256;
use sp_runtime::traits::{One, Saturating, UniqueSaturatedInto, Zero};
use sp_std::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    convert::TryInto,
    prelude::*,
};

#[cfg(feature = "lazy-pages")]
use gear_lazy_pages_common as lazy_pages;

#[cfg(feature = "lazy-pages")]
use ext::LazyPagesExt as Ext;

#[cfg(not(feature = "lazy-pages"))]
use core_processor::Ext;

#[cfg(feature = "std")]
type ExecutionEnvironment<EP = DispatchKind> = gear_backend_wasmi::WasmiEnvironment<Ext, EP>;

#[cfg(not(feature = "std"))]
type ExecutionEnvironment<EP = DispatchKind> = gear_backend_sandbox::SandboxEnvironment<Ext, EP>;

pub(crate) type CurrencyOf<T> = <T as Config>::Currency;
pub(crate) type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
pub(crate) type SentOf<T> = <<T as Config>::Messenger as Messenger>::Sent;
pub(crate) type DbWeightOf<T> = <T as frame_system::Config>::DbWeight;
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
pub(crate) type DispatchStashOf<T> = <<T as Config>::Messenger as Messenger>::DispatchStash;
pub type Authorship<T> = pallet_authorship::Pallet<T>;
pub type GasAllowanceOf<T> = <<T as Config>::BlockLimiter as BlockLimiter>::GasAllowance;
pub type GasHandlerOf<T> = <<T as Config>::GasProvider as GasProvider>::GasTree;
pub type BlockGasLimitOf<T> = <<T as Config>::BlockLimiter as BlockLimiter>::BlockGasLimit;
pub type GasUnitOf<T> = <<T as Config>::BlockLimiter as BlockLimiter>::Balance;
pub type ProgramStorageOf<T> = <T as Config>::ProgramStorage;

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
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
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

    #[pallet::config]
    pub trait Config:
        frame_system::Config + pallet_authorship::Config + pallet_timestamp::Config
    {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The generator used to supply randomness to contracts through `seal_random`
        type Randomness: Randomness<Self::Hash, Self::BlockNumber>;

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

        /// Implementation of a storage for program binary codes.
        type CodeStorage: CodeStorage;

        /// Implementation of a storage for programs.
        type ProgramStorage: ProgramStorage<BlockNumber = Self::BlockNumber>;

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
            DispatchStashKey = MessageId,
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

        /// Message Queue processing routing provider.
        type QueueRunner: QueueRunner<Gas = GasUnitOf<Self>>;
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

        /// Any data related to program codes changed.
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

        /// The pseudo-inherent extrinsic that runs queue processing rolled back or not executed.
        QueueProcessingReverted,
    }

    // Gear pallet error.
    #[pallet::error]
    pub enum Error<T> {
        /// Message wasn't found in the mailbox.
        MessageNotFound,
        /// Not enough balance to reserve.
        ///
        /// Usually occurs when the gas_limit specified is such that the origin account can't afford the message.
        InsufficientBalanceForReserve,
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
        /// Messages storage corrupted.
        MessagesStorageCorrupted,
        /// Message queue processing is disabled.
        MessageQueueProcessingDisabled,
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
    pub(crate) type BlockNumber<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

    impl<T: Config> Get<BlockNumberFor<T>> for Pallet<T> {
        fn get() -> BlockNumberFor<T> {
            Self::block_number()
        }
    }

    /// The Gear block number before processing messages.
    ///
    /// A helper variable that mirrors the `BlockNumber` at the beginning of a block.
    /// Allows to gauge the actual `BlockNumber` progress.
    #[pallet::storage]
    #[pallet::getter(fn last_gear_block_number)]
    pub(crate) type LastGearBlockNumber<T: Config> = StorageValue<_, T::BlockNumber>;

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    where
        T::AccountId: Origin,
    {
        fn on_runtime_upgrade() -> Weight {
            log::debug!(target: "gear::runtime", "⚙️ Runtime upgrade");

            Zero::zero()
        }

        /// Initialization
        fn on_initialize(bn: BlockNumberFor<T>) -> Weight {
            // Incrementing Gear block number
            BlockNumber::<T>::mutate(|bn| *bn = bn.saturating_add(One::one()));

            // Align the last gear block number before inherent execution with current value
            <LastGearBlockNumber<T>>::put(Self::block_number());

            log::debug!(target: "gear::runtime", "⚙️  Initialization of block #{bn:?} (gear #{:?})", Self::block_number());

            // The `LastGearBlockNumber` is killed at the end of a block therefore its value
            // will only exist in overlay and never be committed to storage.
            // The respective write weight can be omitted and not accounted for.
            T::DbWeight::get().writes(1)
        }

        /// Finalization
        fn on_finalize(bn: BlockNumberFor<T>) {
            // Check if the queue has been processed, that is gear block number bumped again.
            // If not (while the queue processing enabled), fire an event.
            if let Some(last_gear_block_number) = <LastGearBlockNumber<T>>::take() {
                if Self::execute_inherent() && Self::block_number() <= last_gear_block_number {
                    Self::deposit_event(Event::QueueProcessingReverted);
                }
            }

            // Undoing Gear block number increment that was done upfront in `on_initialize`
            BlockNumber::<T>::mutate(|bn| *bn = bn.saturating_sub(One::one()));

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
        pub fn set_block_number(bn: u32) {
            use sp_runtime::SaturatedConversion;

            <BlockNumber<T>>::put(bn.saturated_into::<T::BlockNumber>());
        }

        /// Submit program for benchmarks which does not check the code.
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

            let module =
                gear_wasm_instrument::parity_wasm::deserialize_buffer(&code).map_err(|e| {
                    log::debug!("Module failed to load: {:?}", e);
                    Error::<T>::ProgramConstructionFailed
                })?;

            let code = Code::new_raw(
                code,
                schedule.instruction_weights.version,
                Some(module),
                true,
                true,
            )
            .map_err(|e| {
                log::debug!("Code failed to load: {:?}", e);
                Error::<T>::ProgramConstructionFailed
            })?;

            let code_and_id = CodeAndId::new(code);
            let code_info = CodeInfo::from_code_and_id(&code_and_id);

            let packet = InitPacket::new_with_gas(
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
                !ProgramStorageOf::<T>::program_exists(program_id),
                Error::<T>::ProgramAlreadyExists
            );

            let reserve_fee = T::GasPrice::gas_price(gas_limit);

            // First we reserve enough funds on the account to pay for `gas_limit`
            // and to transfer declared value.
            CurrencyOf::<T>::reserve(&who, reserve_fee + value)
                .map_err(|_| Error::<T>::InsufficientBalanceForReserve)?;

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
            let block_number = Self::block_number().unique_saturated_into();

            ExtManager::<T>::default().set_program(
                program_id,
                &code_info,
                message_id,
                block_number,
            );

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

            Self::deposit_event(Event::MessageQueued {
                id: message_id,
                source: who,
                destination: program_id,
                entry: MessageEntry::Init,
            });

            Ok(().into())
        }

        /// Submit code for benchmarks which does not check nor instrument the code.
        #[cfg(feature = "runtime-benchmarks")]
        pub fn upload_code_raw(origin: OriginFor<T>, code: Vec<u8>) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            let schedule = T::Schedule::get();

            let code = Code::new_raw(
                code,
                schedule.instruction_weights.version,
                None,
                false,
                true,
            )
            .map_err(|e| {
                log::debug!("Code failed to load: {:?}", e);
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
            fn_name: Vec<u8>,
            wasm: Vec<u8>,
            argument: Option<Vec<u8>>,
        ) -> Result<Vec<u8>, Vec<u8>> {
            let program_id = ProgramId::from_origin(program_id.into_origin());

            let fn_name = String::from_utf8(fn_name)
                .map_err(|_| "Non-utf8 function name".as_bytes().to_vec())?;

            Self::read_state_using_wasm_impl(program_id, fn_name, wasm, argument)
                .map_err(String::into_bytes)
        }

        pub fn read_state(program_id: H256) -> Result<Vec<u8>, Vec<u8>> {
            let program_id = ProgramId::from_origin(program_id.into_origin());

            Self::read_state_impl(program_id).map_err(String::into_bytes)
        }

        pub fn read_metahash(program_id: H256) -> Result<H256, Vec<u8>> {
            let program_id = ProgramId::from_origin(program_id.into_origin());

            Self::read_metahash_impl(program_id).map_err(String::into_bytes)
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

        /// Returns true if a program has been successfully initialized
        pub fn is_initialized(program_id: ProgramId) -> bool {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|(p, _bn)| p.is_initialized())
                .unwrap_or(false)
        }

        /// Returns true if id is a program and the program has active status.
        pub fn is_active(program_id: ProgramId) -> bool {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|(p, _bn)| p.is_active())
                .unwrap_or_default()
        }

        /// Returns true if id is a program and the program has terminated status.
        pub fn is_terminated(program_id: ProgramId) -> bool {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|(p, _bn)| p.is_terminated())
                .unwrap_or_default()
        }

        /// Returns true if id is a program and the program has exited status.
        pub fn is_exited(program_id: ProgramId) -> bool {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|(p, _bn)| p.is_exited())
                .unwrap_or_default()
        }

        pub fn get_block_number(program_id: ProgramId) -> <T as frame_system::Config>::BlockNumber {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|(_p, bn)| bn)
                .unwrap_or_default()
        }

        /// Returns exit argument of an exited program.
        pub fn exit_inheritor_of(program_id: ProgramId) -> Option<ProgramId> {
            ProgramStorageOf::<T>::get_program(program_id)
                .map(|(p, _bn)| {
                    if let Program::Exited(inheritor) = p {
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
                .map(|(p, _bn)| {
                    if let Program::Terminated(inheritor) = p {
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
            let user_id = ProgramId::from_origin(user_id);

            MessageId::generate_from_user(block_number, user_id, nonce.into())
        }

        /// Delayed tasks processing.
        pub fn process_tasks(ext_manager: &mut ExtManager<T>) {
            // Current block number.
            let bn = Self::block_number();

            // Taking block numbers, where some incomplete tasks held.
            // If there are no such values, we charge for single read, because
            // nothing changing in database, otherwise we delete previous
            // value and charge for single write.
            //
            // We also append current bn to process it together, by iterating
            // over sorted bns set (that's the reason why `BTreeSet` used).
            let (missed_blocks, were_empty) = MissedBlocksOf::<T>::take()
                .map(|mut set| {
                    GasAllowanceOf::<T>::decrease(DbWeightOf::<T>::get().writes(1).ref_time());
                    set.insert(bn);
                    (set, false)
                })
                .unwrap_or_else(|| {
                    GasAllowanceOf::<T>::decrease(DbWeightOf::<T>::get().reads(1).ref_time());
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
                if GasAllowanceOf::<T>::get() <= DbWeightOf::<T>::get().writes(2).ref_time() {
                    stopped_at = Some(*bn);
                    log::debug!("Stopping processing tasks at: {stopped_at:?}");
                    break;
                }

                // Iterating over tasks, scheduled on `bn`.
                for task in tasks {
                    log::debug!("Processing task: {:?}", task);

                    // Decreasing gas allowance due to DB deletion.
                    GasAllowanceOf::<T>::decrease(DbWeightOf::<T>::get().writes(1).ref_time());

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
                    if GasAllowanceOf::<T>::get() <= DbWeightOf::<T>::get().writes(2).ref_time() {
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
                    GasAllowanceOf::<T>::decrease(DbWeightOf::<T>::get().writes(1).ref_time());
                }

                MissedBlocksOf::<T>::put(actual_missed_blocks);
            }
        }

        pub(crate) fn get_and_track_memory_pages(
            manager: &mut ExtManager<T>,
            program_id: ProgramId,
            pages_with_data: &BTreeSet<GearPage>,
            lazy_pages_enabled: bool,
        ) -> Option<BTreeMap<GearPage, PageBuf>> {
            let pages = if lazy_pages_enabled {
                Default::default()
            } else {
                match ProgramStorageOf::<T>::get_program_data_for_pages(
                    program_id,
                    pages_with_data.iter(),
                ) {
                    Ok(data) => data,
                    Err(err) => {
                        log::error!("Cannot get data for program pages: {err:?}");
                        return None;
                    }
                }
            };

            manager.insert_program_id_loaded_pages(program_id);

            Some(pages)
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
        ) -> InstrumentedCode {
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
            )
            .unwrap_or_else(|e| unreachable!("Unexpected re-instrumentation failure: {:?}", e));

            let code_and_id = CodeAndId::from_parts_unchecked(code, code_id);
            let code_and_id = InstrumentedCodeAndId::from(code_and_id);
            T::CodeStorage::update_code(code_and_id.clone());
            let (code, _) = code_and_id.into_parts();

            code
        }

        pub(crate) fn check_code(code: Vec<u8>) -> Result<CodeAndId, DispatchError> {
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
        ) -> Result<InitPacket, DispatchError> {
            let packet = InitPacket::new_with_gas(
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
                !ProgramStorageOf::<T>::program_exists(program_id),
                Error::<T>::ProgramAlreadyExists
            );

            let reserve_fee = T::GasPrice::gas_price(gas_limit);

            // First we reserve enough funds on the account to pay for `gas_limit`
            // and to transfer declared value.
            <T as Config>::Currency::reserve(&who, reserve_fee + value)
                .map_err(|_| Error::<T>::InsufficientBalanceForReserve)?;

            Ok(packet)
        }

        pub(crate) fn do_create_program(
            who: T::AccountId,
            packet: InitPacket,
            code_info: CodeInfo,
        ) -> Result<(), DispatchError> {
            let origin = who.clone().into_origin();

            let message_id = Self::next_message_id(origin);
            let block_number = Self::block_number().unique_saturated_into();

            ExtManager::<T>::default().set_program(
                packet.destination(),
                &code_info,
                message_id,
                block_number,
            );

            // # Safety
            //
            // This is unreachable since the `message_id` is new generated
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

            let event = Event::MessageQueued {
                id: dispatch.id(),
                source: who,
                destination: dispatch.destination(),
                entry: MessageEntry::Init,
            };

            QueueOf::<T>::queue(dispatch).map_err(|_| Error::<T>::MessagesStorageCorrupted)?;

            Self::deposit_event(event);

            Ok(())
        }

        pub fn run_call() -> Call<T> {
            Call::run {}
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
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            Self::check_gas_limit_and_value(gas_limit, value)?;

            let code_and_id = Self::check_code(code)?;
            let code_info = CodeInfo::from_code_and_id(&code_and_id);
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
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;

            // Check if code exists.
            let code = T::CodeStorage::get_code(code_id).ok_or(Error::<T>::CodeDoesntExist)?;

            // Check `gas_limit` and `value`
            Self::check_gas_limit_and_value(gas_limit, value)?;

            // Construct packet.
            let packet =
                Self::init_packet(who.clone(), code_id, salt, init_payload, gas_limit, value)?;

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
        ) -> DispatchResultWithPostInfo {
            let payload = payload
                .try_into()
                .map_err(|err: PayloadSizeError| DispatchError::Other(err.into()))?;
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

            if ProgramStorageOf::<T>::program_exists(destination) {
                ensure!(Self::is_active(destination), Error::<T>::InactiveProgram);

                // Message is not guaranteed to be executed, that's why value is not immediately transferred.
                // That's because destination can fail to be initialized, while this dispatch message is next
                // in the queue.
                CurrencyOf::<T>::reserve(&who, value.unique_saturated_into())
                    .map_err(|_| Error::<T>::InsufficientBalanceForReserve)?;

                let gas_limit_reserve = T::GasPrice::gas_price(gas_limit);

                // First we reserve enough funds on the account to pay for `gas_limit`
                CurrencyOf::<T>::reserve(&who, gas_limit_reserve)
                    .map_err(|_| Error::<T>::InsufficientBalanceForReserve)?;

                // # Safety
                //
                // This is unreachable since the `message_id` is new generated
                // with `Self::next_message_id`.
                GasHandlerOf::<T>::create(who.clone(), message.id(), gas_limit)
                    .unwrap_or_else(|e| unreachable!("GasTree corrupted! {:?}", e));

                let message = message.into_stored_dispatch(ProgramId::from_origin(origin));

                Self::deposit_event(Event::MessageQueued {
                    id: message.id(),
                    source: who,
                    destination: message.destination(),
                    entry: MessageEntry::Handle,
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
                .map_err(|_| Error::<T>::InsufficientBalanceForReserve)?;

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
        #[pallet::call_index(4)]
        #[pallet::weight(<T as Config>::WeightInfo::send_reply(payload.len() as u32))]
        pub fn send_reply(
            origin: OriginFor<T>,
            reply_to_id: MessageId,
            payload: Vec<u8>,
            gas_limit: u64,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let payload = payload
                .try_into()
                .map_err(|err: PayloadSizeError| DispatchError::Other(err.into()))?;

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
                .map_err(|_| Error::<T>::InsufficientBalanceForReserve)?;

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
            // Reason for reading from mailbox.
            let reason = UserMessageReadRuntimeReason::MessageClaimed.into_reason();

            // Reading message, if found, or failing extrinsic.
            Self::read_message(ensure_signed(origin)?, message_id, reason)
                .ok_or(Error::<T>::MessageNotFound)?;

            Ok(().into())
        }

        /// Process message queue
        #[pallet::call_index(6)]
        #[pallet::weight((Weight::zero(), DispatchClass::Mandatory))]
        pub fn run(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            ensure_none(origin)?;

            ensure!(
                ExecuteInherent::<T>::get(),
                Error::<T>::MessageQueueProcessingDisabled
            );

            let weight_used = <frame_system::Pallet<T>>::block_weight();
            let max_weight = <T as frame_system::Config>::BlockWeights::get().max_block;
            let remaining_weight = max_weight.saturating_sub(weight_used.total());

            // Remaining weight may exceed the minimum block gas limit determined by the Limiter trait
            let adjusted_gas = GasAllowanceOf::<T>::get().max(remaining_weight.ref_time());

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

            // Incrementing Gear block number one more time to indicate that the queue processing
            // has been successfully completed in the current block.
            // Caveat: this increment must be done strictly after all extrinsics in the block have
            // been handled to ensure correct block number math.
            BlockNumber::<T>::mutate(|bn| *bn = bn.saturating_add(One::one()));

            Ok(PostDispatchInfo {
                actual_weight: Some(
                    Weight::from_ref_time(actual_weight)
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

    impl<T: Config> QueueRunner for Pallet<T>
    where
        T::AccountId: Origin,
    {
        type Gas = GasUnitOf<T>;

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

            if !leftover.is_zero() {
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
