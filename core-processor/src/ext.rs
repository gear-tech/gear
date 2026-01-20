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

use crate::{configs::BlockInfo, context::SystemReservationContext};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    vec::Vec,
};
use core::marker::PhantomData;
use gear_core::{
    buffer::PayloadSlice,
    costs::{CostToken, ExtCosts, LazyPagesCosts},
    env::Externalities,
    env_vars::{EnvVars, EnvVarsV1},
    gas::{
        ChargeError, ChargeResult, CounterType, CountersOwner, GasAllowanceCounter, GasAmount,
        GasCounter, GasLeft, ValueCounter,
    },
    ids::{ActorId, CodeId, MessageId, ReservationId, prelude::*},
    memory::{
        AllocError, AllocationsContext, GrowHandler, Memory, MemoryError, MemoryInterval, PageBuf,
    },
    message::{
        ContextOutcomeDrain, ContextStore, Dispatch, DispatchKind, GasLimit, HandlePacket,
        InitPacket, MessageContext, Packet, ReplyPacket,
    },
    pages::{
        GearPage, WasmPage, WasmPagesAmount,
        numerated::{interval::Interval, tree::IntervalsTree},
    },
    program::MemoryInfix,
    reservation::GasReserver,
};
use gear_core_backend::{
    BackendExternalities,
    error::{
        ActorTerminationReason, BackendAllocSyscallError, BackendSyscallError, RunFallibleError,
        TrapExplanation, UndefinedTerminationReason, UnrecoverableExecutionError,
        UnrecoverableExtError as UnrecoverableExtErrorCore, UnrecoverableWaitError,
    },
};
use gear_core_errors::{
    ExecutionError as FallibleExecutionError, ExtError as FallibleExtErrorCore, MessageError,
    ReplyCode, ReservationError, SignalCode,
};
use gear_lazy_pages_common::{GlobalsAccessConfig, LazyPagesInterface, ProcessAccessError, Status};
use gear_wasm_instrument::syscalls::SyscallName;

/// Processor context.
pub struct ProcessorContext {
    /// Gas counter.
    pub gas_counter: GasCounter,
    /// Gas allowance counter.
    pub gas_allowance_counter: GasAllowanceCounter,
    /// Reserved gas counter.
    pub gas_reserver: GasReserver,
    /// System reservation.
    pub system_reservation: Option<u64>,
    /// Value counter.
    pub value_counter: ValueCounter,
    /// Allocations context.
    pub allocations_context: AllocationsContext,
    /// Message context.
    pub message_context: MessageContext,
    /// Block info.
    pub block_info: BlockInfo,
    /// Performance multiplier.
    pub performance_multiplier: gsys::Percent,
    /// Current program id
    pub program_id: ActorId,
    /// Map of code hashes to program ids of future programs, which are planned to be
    /// initialized with the corresponding code (with the same code hash).
    pub program_candidates_data: BTreeMap<CodeId, Vec<(MessageId, ActorId)>>,
    /// Functions forbidden to be called.
    pub forbidden_funcs: BTreeSet<SyscallName>,
    /// Reserve for parameter of scheduling.
    pub reserve_for: u32,
    /// Output from Randomness.
    pub random_data: (Vec<u8>, u32),
    /// Gas multiplier.
    pub gas_multiplier: gsys::GasMultiplier,
    /// Existential deposit.
    pub existential_deposit: u128,
    /// Mailbox threshold.
    pub mailbox_threshold: u64,
    /// Execution externalities costs.
    pub costs: ExtCosts,
}

#[cfg(any(feature = "mock", test))]
impl ProcessorContext {
    /// Create new mock [`ProcessorContext`] for usage in tests.
    pub fn new_mock() -> ProcessorContext {
        use gear_core::message::IncomingDispatch;

        const MAX_RESERVATIONS: u64 = 256;

        let incoming_dispatch = IncomingDispatch::default();

        ProcessorContext {
            gas_counter: GasCounter::new(0),
            gas_allowance_counter: GasAllowanceCounter::new(0),
            gas_reserver: GasReserver::new(
                &incoming_dispatch,
                Default::default(),
                MAX_RESERVATIONS,
            ),
            system_reservation: None,
            value_counter: ValueCounter::new(1_000_000),
            allocations_context: AllocationsContext::try_new(
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            )
            .unwrap(),
            message_context: MessageContext::new(
                incoming_dispatch,
                Default::default(),
                Default::default(),
            ),
            block_info: Default::default(),
            performance_multiplier: gsys::Percent::new(100),
            program_id: Default::default(),
            program_candidates_data: Default::default(),
            forbidden_funcs: Default::default(),
            reserve_for: 0,
            random_data: ([0u8; 32].to_vec(), 0),
            gas_multiplier: gsys::GasMultiplier::from_value_per_gas(100),
            existential_deposit: Default::default(),
            mailbox_threshold: Default::default(),
            costs: Default::default(),
        }
    }
}

#[derive(Debug)]
pub struct ExtInfo {
    pub gas_amount: GasAmount,
    pub gas_reserver: GasReserver,
    pub system_reservation_context: SystemReservationContext,
    pub allocations: Option<IntervalsTree<WasmPage>>,
    pub pages_data: BTreeMap<GearPage, PageBuf>,
    pub generated_dispatches: Vec<(Dispatch, u32, Option<ReservationId>)>,
    pub awakening: Vec<(MessageId, u32)>,
    pub reply_deposits: Vec<(MessageId, u64)>,
    pub program_candidates_data: BTreeMap<CodeId, Vec<(MessageId, ActorId)>>,
    pub context_store: ContextStore,
    pub reply_sent: bool,
}

/// Trait to which ext must have to work in processor wasm executor.
/// Currently used only for lazy-pages support.
pub trait ProcessorExternalities {
    /// Create new
    fn new(context: ProcessorContext) -> Self;

    /// Convert externalities into info.
    fn into_ext_info<Context>(
        self,
        ctx: &mut Context,
        memory: &impl Memory<Context>,
    ) -> Result<ExtInfo, MemoryError>;

    /// Protect and save storage keys for pages which has no data
    fn lazy_pages_init_for_program<Context>(
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
        prog_id: ActorId,
        memory_infix: MemoryInfix,
        stack_end: Option<WasmPage>,
        globals_config: GlobalsAccessConfig,
        lazy_pages_costs: LazyPagesCosts,
    );

    /// Lazy pages program post execution actions
    fn lazy_pages_post_execution_actions<Context>(
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
    );

    /// Returns lazy pages status
    fn lazy_pages_status() -> Status;
}

/// Infallible API error.
#[derive(Debug, Clone, Eq, PartialEq, derive_more::From)]
pub enum UnrecoverableExtError {
    /// Basic error
    Core(UnrecoverableExtErrorCore),
    /// Charge error
    Charge(ChargeError),
}

impl From<UnrecoverableExecutionError> for UnrecoverableExtError {
    fn from(err: UnrecoverableExecutionError) -> UnrecoverableExtError {
        Self::Core(UnrecoverableExtErrorCore::from(err))
    }
}

impl From<UnrecoverableWaitError> for UnrecoverableExtError {
    fn from(err: UnrecoverableWaitError) -> UnrecoverableExtError {
        Self::Core(UnrecoverableExtErrorCore::from(err))
    }
}

impl BackendSyscallError for UnrecoverableExtError {
    fn into_termination_reason(self) -> UndefinedTerminationReason {
        match self {
            UnrecoverableExtError::Core(err) => {
                ActorTerminationReason::Trap(TrapExplanation::UnrecoverableExt(err)).into()
            }
            UnrecoverableExtError::Charge(err) => err.into(),
        }
    }

    fn into_run_fallible_error(self) -> RunFallibleError {
        RunFallibleError::UndefinedTerminationReason(self.into_termination_reason())
    }
}

/// Fallible API error.
#[derive(Debug, Clone, Eq, PartialEq, derive_more::From)]
pub enum FallibleExtError {
    /// Basic error
    Core(FallibleExtErrorCore),
    /// An error occurs in attempt to call forbidden syscall.
    ForbiddenFunction,
    /// Charge error
    Charge(ChargeError),
}

impl From<MessageError> for FallibleExtError {
    fn from(err: MessageError) -> Self {
        Self::Core(FallibleExtErrorCore::Message(err))
    }
}

impl From<FallibleExecutionError> for FallibleExtError {
    fn from(err: FallibleExecutionError) -> Self {
        Self::Core(FallibleExtErrorCore::Execution(err))
    }
}

impl From<ReservationError> for FallibleExtError {
    fn from(err: ReservationError) -> Self {
        Self::Core(FallibleExtErrorCore::Reservation(err))
    }
}

impl From<FallibleExtError> for RunFallibleError {
    fn from(err: FallibleExtError) -> Self {
        match err {
            FallibleExtError::Core(err) => RunFallibleError::FallibleExt(err),
            FallibleExtError::ForbiddenFunction => {
                RunFallibleError::UndefinedTerminationReason(UndefinedTerminationReason::Actor(
                    ActorTerminationReason::Trap(TrapExplanation::ForbiddenFunction),
                ))
            }
            FallibleExtError::Charge(err) => {
                RunFallibleError::UndefinedTerminationReason(UndefinedTerminationReason::from(err))
            }
        }
    }
}

/// [`Ext`](Ext)'s memory management (calls to allocate and free) error.
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Display, derive_more::From)]
pub enum AllocExtError {
    /// Charge error
    Charge(ChargeError),
    /// Allocation error
    Alloc(AllocError),
}

impl BackendAllocSyscallError for AllocExtError {
    type ExtError = UnrecoverableExtError;

    fn into_backend_error(self) -> Result<Self::ExtError, Self> {
        match self {
            Self::Charge(err) => Ok(err.into()),
            err => Err(err),
        }
    }
}

struct LazyGrowHandler<LP: LazyPagesInterface> {
    old_mem_addr: Option<u64>,
    old_mem_size: WasmPagesAmount,
    _phantom: PhantomData<LP>,
}

impl<Context, LP: LazyPagesInterface> GrowHandler<Context> for LazyGrowHandler<LP> {
    fn before_grow_action(ctx: &mut Context, mem: &mut impl Memory<Context>) -> Self {
        // New pages allocation may change wasm memory buffer location.
        // So we remove protections from lazy-pages
        // and then in `after_grow_action` we set protection back for new wasm memory buffer.
        let old_mem_addr = mem.get_buffer_host_addr(ctx);
        LP::remove_lazy_pages_prot(ctx, mem);
        Self {
            old_mem_addr,
            old_mem_size: mem.size(ctx),
            _phantom: PhantomData,
        }
    }

    fn after_grow_action(self, ctx: &mut Context, mem: &mut impl Memory<Context>) {
        // Add new allocations to lazy pages.
        // Protect all lazy pages including new allocations.
        let new_mem_addr = mem.get_buffer_host_addr(ctx).unwrap_or_else(|| {
            let err_msg = format!(
                "LazyGrowHandler::after_grow_action: Memory size cannot be zero after grow is applied for memory. \
                Old memory address - {:?}, old memory size - {:?}",
                self.old_mem_addr, self.old_mem_size
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        });
        LP::update_lazy_pages_and_protect_again(
            ctx,
            mem,
            self.old_mem_addr,
            self.old_mem_size,
            new_mem_addr,
        );
    }
}

/// Used to atomically update `Ext` which prevents some errors
/// when data was updated but operation failed.
///
/// Copies some counters into itself and performs operations on them and
/// incrementally builds list of changes.
///
/// Changes are applied after operation is completed
#[must_use]
struct ExtMutator<'a, LP: LazyPagesInterface> {
    ext: &'a mut Ext<LP>,
    gas_counter: GasCounter,
    gas_allowance_counter: GasAllowanceCounter,
    value_counter: ValueCounter,
    outgoing_gasless: u64,
    reservation_to_mark: Option<ReservationId>,
}

impl<LP: LazyPagesInterface> core::ops::Deref for ExtMutator<'_, LP> {
    type Target = Ext<LP>;

    fn deref(&self) -> &Self::Target {
        self.ext
    }
}

impl<'a, LP: LazyPagesInterface> ExtMutator<'a, LP> {
    fn new(ext: &'a mut Ext<LP>) -> Self {
        // SAFETY: counters are cloned and modified *only* by mutator
        unsafe {
            Self {
                gas_counter: ext.context.gas_counter.clone(),
                gas_allowance_counter: ext.context.gas_allowance_counter.clone(),
                value_counter: ext.context.value_counter.clone(),
                outgoing_gasless: ext.outgoing_gasless,
                reservation_to_mark: None,
                ext,
            }
        }
    }

    fn alloc<Context>(
        &mut self,
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
        pages: WasmPagesAmount,
    ) -> Result<WasmPage, AllocError> {
        // can't access context inside `alloc` so move here
        let gas_for_call = self.context.costs.mem_grow.cost_for_one();
        let gas_for_pages = self.context.costs.mem_grow_per_page;
        self.ext
            .context
            .allocations_context
            .alloc::<Context, LazyGrowHandler<LP>>(ctx, mem, pages, |pages| {
                let cost = gas_for_call.saturating_add(gas_for_pages.cost_for(pages));
                // Inline charge_gas_if_enough because otherwise we have borrow error due to access to `allocations_context` mutable
                if self.gas_counter.charge_if_enough(cost).is_not_enough() {
                    return Err(ChargeError::GasLimitExceeded);
                }

                if self
                    .gas_allowance_counter
                    .charge_if_enough(cost)
                    .is_not_enough()
                {
                    return Err(ChargeError::GasAllowanceExceeded);
                }
                Ok(())
            })
    }

    fn reduce_gas(&mut self, limit: GasLimit) -> Result<(), FallibleExtError> {
        if self.gas_counter.reduce(limit).is_not_enough() {
            return Err(FallibleExecutionError::NotEnoughGas.into());
        }

        Ok(())
    }

    fn charge_message_value(&mut self, value: u128) -> Result<(), FallibleExtError> {
        if self.value_counter.reduce(value).is_not_enough() {
            return Err(FallibleExecutionError::NotEnoughValue.into());
        }

        Ok(())
    }

    fn mark_reservation_used(
        &mut self,
        reservation_id: ReservationId,
    ) -> Result<(), ReservationError> {
        let _ = self
            .ext
            .context
            .gas_reserver
            .check_not_used(reservation_id)?;
        self.reservation_to_mark = Some(reservation_id);
        Ok(())
    }

    fn charge_gas_if_enough(&mut self, gas: u64) -> Result<(), ChargeError> {
        if self.gas_counter.charge_if_enough(gas).is_not_enough() {
            return Err(ChargeError::GasLimitExceeded);
        }

        if self
            .gas_allowance_counter
            .charge_if_enough(gas)
            .is_not_enough()
        {
            return Err(ChargeError::GasAllowanceExceeded);
        }
        Ok(())
    }

    fn charge_expiring_resources<T: Packet>(&mut self, packet: &T) -> Result<(), FallibleExtError> {
        let reducing_gas_limit = self.get_reducing_gas_limit(packet)?;

        self.reduce_gas(reducing_gas_limit)?;
        self.charge_message_value(packet.value())
    }

    fn get_reducing_gas_limit<T: Packet>(&self, packet: &T) -> Result<u64, FallibleExtError> {
        match T::kind() {
            DispatchKind::Handle => {
                // Any "handle" gasless and gasful *non zero* message must
                // cover mailbox threshold. That's because destination
                // of the message is unknown, so it could be a user,
                // and if gasless message is sent, there must be a
                // guaranteed gas to cover mailbox.
                let mailbox_threshold = self.context.mailbox_threshold;
                let gas_limit = packet.gas_limit().unwrap_or(mailbox_threshold);

                // Zero gasful message is a special case.
                if gas_limit != 0 && gas_limit < mailbox_threshold {
                    return Err(MessageError::InsufficientGasLimit.into());
                }

                Ok(gas_limit)
            }
            DispatchKind::Init | DispatchKind::Reply => {
                // Init and reply messages never go to mailbox.
                //
                // For init case, even if there's no code with a provided
                // code id, the init message still goes to queue and then is handled as non
                // executable, as there is no code for the destination actor.
                //
                // Also no reply to user messages go to mailbox, they all are emitted
                // within events.

                Ok(packet.gas_limit().unwrap_or(0))
            }
            DispatchKind::Signal => unreachable!("Signals can't be sent as a syscall"),
        }
    }

    fn charge_sending_fee(&mut self, delay: u32) -> Result<(), ChargeError> {
        if delay == 0 {
            self.charge_gas_if_enough(self.context.message_context.settings().sending_fee)
        } else {
            self.charge_gas_if_enough(
                self.context
                    .message_context
                    .settings()
                    .scheduled_sending_fee,
            )
        }
    }

    fn charge_for_dispatch_stash_hold(&mut self, delay: u32) -> Result<(), FallibleExtError> {
        if delay != 0 {
            let waiting_reserve = self
                .context
                .costs
                .rent
                .dispatch_stash
                .cost_for(self.context.reserve_for.saturating_add(delay).into());

            // Reduce gas for block waiting in dispatch stash.
            return self
                .reduce_gas(waiting_reserve)
                .map_err(|_| MessageError::InsufficientGasForDelayedSending.into());
        }

        Ok(())
    }

    fn apply(mut self) {
        if let Some(reservation) = self.reservation_to_mark.take() {
            let result = self.ext.context.gas_reserver.mark_used(reservation);
            debug_assert!(result.is_ok());
        }

        self.ext.context.gas_counter = self.gas_counter;
        self.ext.context.value_counter = self.value_counter;
        self.ext.context.gas_allowance_counter = self.gas_allowance_counter;
        self.ext.outgoing_gasless = self.outgoing_gasless;
    }
}

/// Structure providing externalities for running host functions.
pub struct Ext<LP: LazyPagesInterface> {
    /// Processor context.
    pub context: ProcessorContext,
    /// Actual gas counter type within wasm module's global.
    pub current_counter: CounterType,
    // Counter of outgoing gasless messages.
    //
    // It's temporary field, used to solve `core-audit/issue#22`.
    outgoing_gasless: u64,
    _phantom: PhantomData<LP>,
}

/// Empty implementation for non-substrate (and non-lazy-pages) using
impl<LP: LazyPagesInterface> ProcessorExternalities for Ext<LP> {
    fn new(context: ProcessorContext) -> Self {
        let current_counter = if context.gas_counter.left() <= context.gas_allowance_counter.left()
        {
            CounterType::GasLimit
        } else {
            CounterType::GasAllowance
        };

        Self {
            context,
            current_counter,
            outgoing_gasless: 0,
            _phantom: PhantomData,
        }
    }

    fn into_ext_info<Context>(
        self,
        ctx: &mut Context,
        memory: &impl Memory<Context>,
    ) -> Result<ExtInfo, MemoryError> {
        let ProcessorContext {
            allocations_context,
            message_context,
            gas_counter,
            gas_reserver,
            system_reservation,
            program_candidates_data,
            ..
        } = self.context;

        let (static_pages, allocations, allocations_changed) = allocations_context.into_parts();

        // Accessed pages are all pages, that had been released and are in allocations set or static.
        let mut accessed_pages = LP::get_write_accessed_pages();
        accessed_pages.retain(|p| {
            let wasm_page: WasmPage = p.to_page();
            wasm_page < static_pages || allocations.contains(wasm_page)
        });
        log::trace!("accessed pages numbers = {accessed_pages:?}");

        let mut pages_data = BTreeMap::new();
        for page in accessed_pages {
            let mut buf = PageBuf::new_zeroed();
            memory.read(ctx, page.offset(), &mut buf)?;
            pages_data.insert(page, buf);
        }

        let (outcome, mut context_store) = message_context.drain();
        let ContextOutcomeDrain {
            outgoing_dispatches: generated_dispatches,
            awakening,
            reply_deposits,
            reply_sent,
        } = outcome.drain();

        let system_reservation_context = SystemReservationContext {
            current_reservation: system_reservation,
            previous_reservation: context_store.system_reservation(),
        };

        context_store.set_reservation_nonce(&gas_reserver);
        if let Some(reservation) = system_reservation {
            context_store.add_system_reservation(reservation);
        }

        let info = ExtInfo {
            gas_amount: gas_counter.to_amount(),
            gas_reserver,
            system_reservation_context,
            // `allocations_changed` can be some times `true` event if final state of allocations is the same as before execution
            allocations: allocations_changed.then_some(allocations),
            pages_data,
            generated_dispatches,
            awakening,
            reply_deposits,
            context_store,
            program_candidates_data,
            reply_sent,
        };
        Ok(info)
    }

    fn lazy_pages_init_for_program<Context>(
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
        prog_id: ActorId,
        memory_infix: MemoryInfix,
        stack_end: Option<WasmPage>,
        globals_config: GlobalsAccessConfig,
        lazy_pages_costs: LazyPagesCosts,
    ) {
        LP::init_for_program(
            ctx,
            mem,
            prog_id,
            memory_infix,
            stack_end,
            globals_config,
            lazy_pages_costs,
        );
    }

    fn lazy_pages_post_execution_actions<Context>(
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
    ) {
        LP::remove_lazy_pages_prot(ctx, mem);
    }

    fn lazy_pages_status() -> Status {
        LP::get_status()
    }
}

impl<LP: LazyPagesInterface> BackendExternalities for Ext<LP> {
    fn gas_amount(&self) -> GasAmount {
        self.context.gas_counter.to_amount()
    }

    fn pre_process_memory_accesses(
        &mut self,
        reads: &[MemoryInterval],
        writes: &[MemoryInterval],
        gas_counter: &mut u64,
    ) -> Result<(), ProcessAccessError> {
        LP::pre_process_memory_accesses(reads, writes, gas_counter)
    }
}

impl<LP: LazyPagesInterface> Ext<LP> {
    fn with_changes<F, R, E>(&mut self, callback: F) -> Result<R, E>
    where
        F: FnOnce(&mut ExtMutator<LP>) -> Result<R, E>,
    {
        let mut mutator = ExtMutator::new(self);
        let result = callback(&mut mutator)?;
        mutator.apply();
        Ok(result)
    }

    /// Checking that reservation could be charged for
    /// dispatch stash with given delay.
    fn check_reservation_gas_limit_for_delayed_sending(
        &self,
        reservation_id: &ReservationId,
        delay: u32,
    ) -> Result<(), FallibleExtError> {
        if delay != 0 {
            let limit = self
                .context
                .gas_reserver
                .limit_of(reservation_id)
                .ok_or(ReservationError::InvalidReservationId)?;

            let waiting_reserve = self
                .context
                .costs
                .rent
                .dispatch_stash
                .cost_for(self.context.reserve_for.saturating_add(delay).into());

            // Gas reservation is known for covering mailbox threshold, as reservation
            // is created after passing a check for that.
            // By this check we guarantee that reservation is enough both for delay
            // and for mailbox threshold.
            if limit < waiting_reserve.saturating_add(self.context.mailbox_threshold) {
                return Err(MessageError::InsufficientGasForDelayedSending.into());
            }
        }

        Ok(())
    }

    fn check_forbidden_destination(&self, id: ActorId) -> Result<(), FallibleExtError> {
        if id == ActorId::SYSTEM {
            Err(FallibleExtError::ForbiddenFunction)
        } else {
            Ok(())
        }
    }

    fn charge_gas_if_enough(
        gas_counter: &mut GasCounter,
        gas_allowance_counter: &mut GasAllowanceCounter,
        amount: u64,
    ) -> Result<(), ChargeError> {
        if gas_counter.charge_if_enough(amount).is_not_enough() {
            return Err(ChargeError::GasLimitExceeded);
        }
        if gas_allowance_counter
            .charge_if_enough(amount)
            .is_not_enough()
        {
            // Here might be refunds for gas counter, but it's meaningless since
            // on gas allowance exceed we totally roll up the message and give
            // it another try in next block with the same initial resources.
            return Err(ChargeError::GasAllowanceExceeded);
        }
        Ok(())
    }

    fn cost_for_reservation(&self, amount: u64, duration: u32) -> u64 {
        self.context
            .costs
            .rent
            .reservation
            .cost_for(self.context.reserve_for.saturating_add(duration).into())
            .saturating_add(amount)
    }
}

impl<LP: LazyPagesInterface> CountersOwner for Ext<LP> {
    fn charge_gas_for_token(&mut self, token: CostToken) -> Result<(), ChargeError> {
        let amount = self.context.costs.syscalls.cost_for_token(token);
        let common_charge = self.context.gas_counter.charge(amount);
        let allowance_charge = self.context.gas_allowance_counter.charge(amount);
        match (common_charge, allowance_charge) {
            (ChargeResult::NotEnough, _) => Err(ChargeError::GasLimitExceeded),
            (ChargeResult::Enough, ChargeResult::NotEnough) => {
                Err(ChargeError::GasAllowanceExceeded)
            }
            (ChargeResult::Enough, ChargeResult::Enough) => Ok(()),
        }
    }

    fn charge_gas_if_enough(&mut self, amount: u64) -> Result<(), ChargeError> {
        Self::charge_gas_if_enough(
            &mut self.context.gas_counter,
            &mut self.context.gas_allowance_counter,
            amount,
        )
    }

    fn gas_left(&self) -> GasLeft {
        (
            self.context.gas_counter.left(),
            self.context.gas_allowance_counter.left(),
        )
            .into()
    }

    fn current_counter_type(&self) -> CounterType {
        self.current_counter
    }

    fn decrease_current_counter_to(&mut self, amount: u64) {
        // For possible cases of non-atomic charges on backend side when global
        // value is less than appropriate at the backend.
        //
        // Example:
        // * While executing program calls some syscall.
        // * Syscall ends up with unrecoverable error - gas limit exceeded.
        // * We have to charge it so we leave backend and whole execution with 0 inner counter.
        // * Meanwhile global is not zero, so for this case we have to skip decreasing.
        if self.current_counter_value() <= amount {
            log::trace!("Skipped decrease to global value");
            return;
        }

        let GasLeft { gas, allowance } = self.gas_left();

        let diff = match self.current_counter_type() {
            CounterType::GasLimit => gas.checked_sub(amount),
            CounterType::GasAllowance => allowance.checked_sub(amount),
        }
        .unwrap_or_else(|| {
            let err_msg = format!(
                "CounterOwner::decrease_current_counter_to: Checked sub operation overflowed. \
                Message id - {message_id}, program id - {program_id}, current counter type - {current_counter_type:?}, \
                gas - {gas}, allowance - {allowance}, amount - {amount}",
                message_id = self.context.message_context.current().id(), program_id = self.context.program_id, current_counter_type = self.current_counter_type()
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        });

        if self.context.gas_counter.charge(diff).is_not_enough() {
            let err_msg = format!(
                "CounterOwner::decrease_current_counter_to: Tried to set gas limit left bigger than before. \
                Message id - {message_id}, program id - {program_id}, gas counter - {gas_counter:?}, diff - {diff}",
                message_id = self.context.message_context.current().id(),
                program_id = self.context.program_id,
                gas_counter = self.context.gas_counter
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        }

        if self
            .context
            .gas_allowance_counter
            .charge(diff)
            .is_not_enough()
        {
            let err_msg = format!(
                "CounterOwner::decrease_current_counter_to: Tried to set gas allowance left bigger than before. \
                Message id - {message_id}, program id - {program_id}, gas allowance counter - {gas_allowance_counter:?}, diff - {diff}",
                message_id = self.context.message_context.current().id(),
                program_id = self.context.program_id,
                gas_allowance_counter = self.context.gas_allowance_counter,
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        }
    }

    fn define_current_counter(&mut self) -> u64 {
        let GasLeft { gas, allowance } = self.gas_left();

        if gas <= allowance {
            self.current_counter = CounterType::GasLimit;
            gas
        } else {
            self.current_counter = CounterType::GasAllowance;
            allowance
        }
    }
}

impl<LP: LazyPagesInterface> Externalities for Ext<LP> {
    type UnrecoverableError = UnrecoverableExtError;
    type FallibleError = FallibleExtError;
    type AllocError = AllocExtError;

    fn alloc<Context>(
        &mut self,
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
        pages_num: u32,
    ) -> Result<WasmPage, Self::AllocError> {
        let pages = WasmPagesAmount::try_from(pages_num)
            .map_err(|_| AllocError::ProgramAllocOutOfBounds)?;

        self.with_changes(|mutator| {
            mutator
                .alloc::<Context>(ctx, mem, pages)
                .map_err(Into::into)
        })
    }

    fn free(&mut self, page: WasmPage) -> Result<(), Self::AllocError> {
        self.context
            .allocations_context
            .free(page)
            .map_err(Into::into)
    }

    fn free_range(&mut self, start: WasmPage, end: WasmPage) -> Result<(), Self::AllocError> {
        let interval = Interval::try_from(start..=end)
            .map_err(|_| AllocExtError::Alloc(AllocError::InvalidFreeRange(start, end)))?;
        self.with_changes(|mutator| {
            mutator.charge_gas_if_enough(
                mutator
                    .context
                    .costs
                    .syscalls
                    .free_range_per_page
                    .cost_for(interval.len()),
            )?;
            mutator
                .ext
                .context
                .allocations_context
                .free_range(interval)
                .map_err(Into::into)
        })
    }

    fn env_vars(&self, version: u32) -> Result<EnvVars, Self::UnrecoverableError> {
        match version {
            1 => Ok(EnvVars::V1(EnvVarsV1 {
                performance_multiplier: self.context.performance_multiplier,
                existential_deposit: self.context.existential_deposit,
                mailbox_threshold: self.context.mailbox_threshold,
                gas_multiplier: self.context.gas_multiplier,
            })),
            _ => Err(UnrecoverableExecutionError::UnsupportedEnvVarsVersion.into()),
        }
    }

    fn block_height(&self) -> Result<u32, Self::UnrecoverableError> {
        Ok(self.context.block_info.height)
    }

    fn block_timestamp(&self) -> Result<u64, Self::UnrecoverableError> {
        Ok(self.context.block_info.timestamp)
    }

    fn send_init(&mut self) -> Result<u32, Self::FallibleError> {
        let handle = self.context.message_context.send_init()?;
        Ok(handle)
    }

    fn send_push(&mut self, handle: u32, buffer: &[u8]) -> Result<(), Self::FallibleError> {
        self.context.message_context.send_push(handle, buffer)?;
        Ok(())
    }

    fn send_push_input(
        &mut self,
        handle: u32,
        offset: u32,
        len: u32,
    ) -> Result<(), Self::FallibleError> {
        let range = self
            .context
            .message_context
            .check_input_range(offset, len)?;

        self.with_changes(|mutator| {
            mutator.charge_gas_if_enough(
                mutator
                    .context
                    .costs
                    .syscalls
                    .gr_send_push_input_per_byte
                    .cost_for(range.len().into()),
            )?;
            mutator
                .ext
                .context
                .message_context
                .send_push_input(handle, range)
                .map_err(Into::into)
        })
    }

    fn send_commit(
        &mut self,
        handle: u32,
        msg: HandlePacket,
        delay: u32,
    ) -> Result<MessageId, Self::FallibleError> {
        self.with_changes(|mutator| {
            mutator.check_forbidden_destination(msg.destination())?;
            mutator.charge_expiring_resources(&msg)?;
            mutator.charge_sending_fee(delay)?;
            mutator.charge_for_dispatch_stash_hold(delay)?;

            mutator
                .ext
                .context
                .message_context
                .send_commit(handle, msg, delay, None)
                .map_err(Into::into)
        })
    }

    fn reservation_send_commit(
        &mut self,
        id: ReservationId,
        handle: u32,
        msg: HandlePacket,
        delay: u32,
    ) -> Result<MessageId, Self::FallibleError> {
        self.with_changes(|mutator| {
            mutator.check_forbidden_destination(msg.destination())?;
            // TODO: unify logic around different source of gas (may be origin msg,
            // or reservation) in order to implement #1828.
            mutator.check_reservation_gas_limit_for_delayed_sending(&id, delay)?;
            // TODO: gasful sending (#1828)
            mutator.charge_message_value(msg.value())?;
            mutator.charge_sending_fee(delay)?;

            mutator.mark_reservation_used(id)?;

            mutator
                .ext
                .context
                .message_context
                .send_commit(handle, msg, delay, Some(id))
                .map_err(Into::into)
        })
    }

    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Self::FallibleError> {
        self.context.message_context.reply_push(buffer)?;
        Ok(())
    }

    // TODO: Consider per byte charge (issue #2255).
    fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::FallibleError> {
        self.with_changes(|mutator| {
            mutator
                .check_forbidden_destination(mutator.context.message_context.reply_destination())?;
            mutator.charge_expiring_resources(&msg)?;
            mutator.charge_sending_fee(0)?;

            mutator
                .ext
                .context
                .message_context
                .reply_commit(msg, None)
                .map_err(Into::into)
        })
    }

    fn reservation_reply_commit(
        &mut self,
        id: ReservationId,
        msg: ReplyPacket,
    ) -> Result<MessageId, Self::FallibleError> {
        self.with_changes(|mutator| {
            mutator
                .check_forbidden_destination(mutator.context.message_context.reply_destination())?;
            // TODO: gasful sending (#1828)
            mutator.charge_message_value(msg.value())?;
            mutator.charge_sending_fee(0)?;

            mutator.mark_reservation_used(id)?;

            mutator
                .ext
                .context
                .message_context
                .reply_commit(msg, Some(id))
                .map_err(Into::into)
        })
    }

    fn reply_to(&self) -> Result<MessageId, Self::FallibleError> {
        self.context
            .message_context
            .current()
            .details()
            .and_then(|d| d.to_reply_details().map(|d| d.to_message_id()))
            .ok_or_else(|| FallibleExecutionError::NoReplyContext.into())
    }

    fn signal_from(&self) -> Result<MessageId, Self::FallibleError> {
        self.context
            .message_context
            .current()
            .details()
            .and_then(|d| d.to_signal_details().map(|d| d.to_message_id()))
            .ok_or_else(|| FallibleExecutionError::NoSignalContext.into())
    }

    fn reply_push_input(&mut self, offset: u32, len: u32) -> Result<(), Self::FallibleError> {
        self.with_changes(|mutator| {
            let range = mutator
                .context
                .message_context
                .check_input_range(offset, len)?;
            mutator.charge_gas_if_enough(
                mutator
                    .context
                    .costs
                    .syscalls
                    .gr_reply_push_input_per_byte
                    .cost_for(range.len().into()),
            )?;
            mutator
                .ext
                .context
                .message_context
                .reply_push_input(range)
                .map_err(Into::into)
        })
    }

    fn source(&self) -> Result<ActorId, Self::UnrecoverableError> {
        Ok(self.context.message_context.current().source())
    }

    fn reply_code(&self) -> Result<ReplyCode, Self::FallibleError> {
        self.context
            .message_context
            .current()
            .details()
            .and_then(|d| d.to_reply_details().map(|d| d.to_reply_code()))
            .ok_or_else(|| FallibleExecutionError::NoReplyContext.into())
    }

    fn signal_code(&self) -> Result<SignalCode, Self::FallibleError> {
        self.context
            .message_context
            .current()
            .details()
            .and_then(|d| d.to_signal_details().map(|d| d.to_signal_code()))
            .ok_or_else(|| FallibleExecutionError::NoSignalContext.into())
    }

    fn message_id(&self) -> Result<MessageId, Self::UnrecoverableError> {
        Ok(self.context.message_context.current().id())
    }

    fn program_id(&self) -> Result<ActorId, Self::UnrecoverableError> {
        Ok(self.context.program_id)
    }

    fn debug(&self, data: &str) -> Result<(), Self::UnrecoverableError> {
        let program_id = self.program_id()?;
        let message_id = self.message_id()?;

        log::debug!(target: "gwasm", "[handle({message_id:.2?})] {program_id:.2?}: {data}");

        Ok(())
    }

    fn payload_slice(&mut self, at: u32, len: u32) -> Result<PayloadSlice, Self::FallibleError> {
        let end = at
            .checked_add(len)
            .ok_or(FallibleExecutionError::TooBigReadLen)?;

        self.with_changes(|mutator| {
            mutator.charge_gas_if_enough(
                mutator
                    .context
                    .costs
                    .syscalls
                    .gr_read_per_byte
                    .cost_for(len.into()),
            )?;

            PayloadSlice::try_new(at, end, mutator.context.message_context.current().payload())
                .ok_or_else(|| FallibleExecutionError::ReadWrongRange.into())
        })
    }

    fn size(&self) -> Result<usize, Self::UnrecoverableError> {
        Ok(self.context.message_context.current().payload().len())
    }

    fn reserve_gas(
        &mut self,
        amount: u64,
        duration: u32,
    ) -> Result<ReservationId, Self::FallibleError> {
        self.with_changes(|mutator| {
            mutator
                .charge_gas_if_enough(mutator.context.message_context.settings().reservation_fee)?;

            if duration == 0 {
                return Err(ReservationError::ZeroReservationDuration.into());
            }

            if amount < mutator.context.mailbox_threshold {
                return Err(ReservationError::ReservationBelowMailboxThreshold.into());
            }

            let reduce_amount = mutator.cost_for_reservation(amount, duration);

            mutator
                .reduce_gas(reduce_amount)
                .map_err(|_| FallibleExecutionError::NotEnoughGas)?;

            mutator
                .ext
                .context
                .gas_reserver
                .reserve(amount, duration)
                .map_err(Into::into)
        })
    }

    #[allow(clippy::obfuscated_if_else)]
    fn unreserve_gas(&mut self, id: ReservationId) -> Result<u64, Self::FallibleError> {
        let (amount, reimburse) = self.context.gas_reserver.unreserve(id)?;

        if let Some(reimbursement) = reimburse {
            let current_gas_amount = self.gas_amount();

            // Basically amount of the reseravtion and the cost for the hold duration.
            let reimbursement_amount = self.cost_for_reservation(amount, reimbursement.duration());
            self.context
                .gas_counter
                .increase(reimbursement_amount, reimbursement)
                .then_some(())
                .unwrap_or_else(|| {
                    let err_msg = format!(
                        "Ext::unreserve_gas: failed to reimburse unreserved gas to left counter. \
                        Current gas amount - {}, reimburse amount - {}",
                        current_gas_amount.left(),
                        amount,
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}");
                });
        }

        Ok(amount)
    }

    fn system_reserve_gas(&mut self, amount: u64) -> Result<(), Self::FallibleError> {
        // TODO: use `NonZero<u64>` after issue #1838 is fixed
        if amount == 0 {
            return Err(ReservationError::ZeroReservationAmount.into());
        }

        if self.context.gas_counter.reduce(amount).is_not_enough() {
            return Err(FallibleExecutionError::NotEnoughGas.into());
        }

        let reservation = &mut self.context.system_reservation;
        *reservation = reservation
            .map(|reservation| reservation.saturating_add(amount))
            .or(Some(amount));

        Ok(())
    }

    fn gas_available(&self) -> Result<u64, Self::UnrecoverableError> {
        Ok(self.context.gas_counter.left())
    }

    fn value(&self) -> Result<u128, Self::UnrecoverableError> {
        Ok(self.context.message_context.current().value())
    }

    fn value_available(&self) -> Result<u128, Self::UnrecoverableError> {
        Ok(self.context.value_counter.left())
    }

    fn wait(&mut self) -> Result<(), Self::UnrecoverableError> {
        self.with_changes(|mutator| {
            mutator.charge_gas_if_enough(mutator.context.message_context.settings().waiting_fee)?;

            if mutator.context.message_context.reply_sent() {
                return Err(UnrecoverableWaitError::WaitAfterReply.into());
            }

            let reserve = mutator
                .context
                .costs
                .rent
                .waitlist
                .cost_for(mutator.context.reserve_for.saturating_add(1).into());

            mutator
                .reduce_gas(reserve)
                .map_err(|_| UnrecoverableExecutionError::NotEnoughGas)?;

            Ok(())
        })
    }

    fn wait_for(&mut self, duration: u32) -> Result<(), Self::UnrecoverableError> {
        self.with_changes(|mutator| {
            mutator.charge_gas_if_enough(mutator.context.message_context.settings().waiting_fee)?;

            if mutator.context.message_context.reply_sent() {
                return Err(UnrecoverableWaitError::WaitAfterReply.into());
            }

            if duration == 0 {
                return Err(UnrecoverableWaitError::ZeroDuration.into());
            }

            let reserve = mutator
                .context
                .costs
                .rent
                .waitlist
                .cost_for(mutator.context.reserve_for.saturating_add(duration).into());

            if mutator.gas_counter.reduce(reserve).is_not_enough() {
                return Err(UnrecoverableExecutionError::NotEnoughGas.into());
            }

            Ok(())
        })
    }

    fn wait_up_to(&mut self, duration: u32) -> Result<bool, Self::UnrecoverableError> {
        self.with_changes(|mutator| {
            mutator.charge_gas_if_enough(mutator.context.message_context.settings().waiting_fee)?;

            if mutator.context.message_context.reply_sent() {
                return Err(UnrecoverableWaitError::WaitAfterReply.into());
            }

            if duration == 0 {
                return Err(UnrecoverableWaitError::ZeroDuration.into());
            }

            let reserve = mutator
                .context
                .costs
                .rent
                .waitlist
                .cost_for(mutator.context.reserve_for.saturating_add(1).into());

            if mutator.gas_counter.reduce(reserve).is_not_enough() {
                return Err(UnrecoverableExecutionError::NotEnoughGas.into());
            }

            let reserve_full = mutator
                .context
                .costs
                .rent
                .waitlist
                .cost_for(mutator.context.reserve_for.saturating_add(duration).into());

            let reserve_diff = reserve_full - reserve;

            Ok(mutator.gas_counter.reduce(reserve_diff).is_enough())
        })
    }

    fn wake(&mut self, waker_id: MessageId, delay: u32) -> Result<(), Self::FallibleError> {
        self.with_changes(|mutator| {
            mutator.charge_gas_if_enough(mutator.context.message_context.settings().waking_fee)?;

            mutator
                .ext
                .context
                .message_context
                .wake(waker_id, delay)
                .map_err(Into::into)
        })
    }

    fn create_program(
        &mut self,
        packet: InitPacket,
        delay: u32,
    ) -> Result<(MessageId, ActorId), Self::FallibleError> {
        let ed = self.context.existential_deposit;
        self.with_changes(|mutator| {
            // We don't check for forbidden destination here, since dest is always unique
            // and almost impossible to match SYSTEM_ID

            mutator.charge_expiring_resources(&packet)?;
            mutator.charge_sending_fee(delay)?;
            mutator.charge_for_dispatch_stash_hold(delay)?;

            // Charge ED to value_counter
            mutator.charge_message_value(ed)?;

            let code_hash = packet.code_id();

            // Send a message for program creation

            mutator
                .ext
                .context
                .message_context
                .init_program(packet, delay)
                .map(|(init_msg_id, new_prog_id)| {
                    let entry = mutator
                        .ext
                        .context
                        .program_candidates_data
                        .entry(code_hash)
                        .or_default();
                    entry.push((init_msg_id, new_prog_id));
                    (init_msg_id, new_prog_id)
                })
                .map_err(Into::into)
        })
    }

    fn reply_deposit(
        &mut self,
        message_id: MessageId,
        amount: u64,
    ) -> Result<(), Self::FallibleError> {
        self.with_changes(|mutator| {
            mutator.reduce_gas(amount)?;
            mutator
                .ext
                .context
                .message_context
                .reply_deposit(message_id, amount)
                .map_err(Into::into)
        })
    }

    fn random(&self) -> Result<(&[u8], u32), Self::UnrecoverableError> {
        Ok((&self.context.random_data.0, self.context.random_data.1))
    }

    fn forbidden_funcs(&self) -> &BTreeSet<SyscallName> {
        &self.context.forbidden_funcs
    }

    fn msg_ctx(&self) -> &MessageContext {
        &self.context.message_context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use gear_core::{
        buffer::{MAX_PAYLOAD_SIZE, Payload},
        costs::{CostOf, RentCosts, SyscallCosts},
        message::{ContextSettings, IncomingDispatch, IncomingMessage},
        reservation::{GasReservationMap, GasReservationSlot, GasReservationState},
    };

    struct MessageContextBuilder {
        incoming_dispatch: IncomingDispatch,
        program_id: ActorId,
        context_settings: ContextSettings,
    }

    type Ext = super::Ext<()>;

    impl MessageContextBuilder {
        fn new() -> Self {
            Self {
                incoming_dispatch: Default::default(),
                program_id: Default::default(),
                context_settings: ContextSettings::with_outgoing_limits(u32::MAX, u32::MAX),
            }
        }

        fn build(self) -> MessageContext {
            MessageContext::new(
                self.incoming_dispatch,
                self.program_id,
                self.context_settings,
            )
        }

        fn with_payload(mut self, payload: Vec<u8>) -> Self {
            self.incoming_dispatch = IncomingDispatch::new(
                Default::default(),
                IncomingMessage::new(
                    Default::default(),
                    Default::default(),
                    payload.try_into().unwrap(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                ),
                Default::default(),
            );

            self
        }

        fn with_outgoing_limit(mut self, outgoing_limit: u32) -> Self {
            self.context_settings.outgoing_limit = outgoing_limit;

            self
        }
    }

    struct ProcessorContextBuilder(ProcessorContext);

    impl ProcessorContextBuilder {
        fn new() -> Self {
            Self(ProcessorContext::new_mock())
        }

        fn build(self) -> ProcessorContext {
            self.0
        }

        fn with_message_context(mut self, context: MessageContext) -> Self {
            self.0.message_context = context;

            self
        }

        fn with_gas(mut self, gas_counter: GasCounter) -> Self {
            self.0.gas_counter = gas_counter;

            self
        }

        fn with_allowance(mut self, gas_allowance_counter: GasAllowanceCounter) -> Self {
            self.0.gas_allowance_counter = gas_allowance_counter;

            self
        }

        fn with_costs(mut self, costs: ExtCosts) -> Self {
            self.0.costs = costs;

            self
        }

        fn with_allocation_context(mut self, ctx: AllocationsContext) -> Self {
            self.0.allocations_context = ctx;

            self
        }

        fn with_existential_deposit(mut self, ed: u128) -> Self {
            self.0.existential_deposit = ed;

            self
        }

        fn with_value(mut self, value: u128) -> Self {
            self.0.value_counter = ValueCounter::new(value);

            self
        }

        fn with_reservations_map(mut self, map: GasReservationMap) -> Self {
            self.0.gas_reserver = GasReserver::new(&Default::default(), map, 256);

            self
        }
    }

    // Invariant: Refund never occurs in `free` call.
    #[test]
    fn free_no_refund() {
        // Set initial Ext state
        let initial_gas = 100;
        let initial_allowance = 10000;

        let gas_left = (initial_gas, initial_allowance).into();

        let existing_page = 99.into();
        let non_existing_page = 100.into();

        let allocations_context = AllocationsContext::try_new(
            512.into(),
            [existing_page].into_iter().collect(),
            1.into(),
            None,
            512.into(),
        )
        .unwrap();

        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_gas(GasCounter::new(initial_gas))
                .with_allowance(GasAllowanceCounter::new(initial_allowance))
                .with_allocation_context(allocations_context)
                .build(),
        );

        // Freeing existing page.
        // Counters shouldn't be changed.
        assert!(ext.free(existing_page).is_ok());
        assert_eq!(ext.gas_left(), gas_left);

        // Freeing non existing page.
        // Counters still shouldn't be changed.
        assert_eq!(
            ext.free(non_existing_page),
            Err(AllocExtError::Alloc(AllocError::InvalidFree(
                non_existing_page
            )))
        );
        assert_eq!(ext.gas_left(), gas_left);
    }

    #[test]
    fn test_counter_zeroes() {
        // Set initial Ext state
        let free_cost = 1000;
        let ext_costs = ExtCosts {
            syscalls: SyscallCosts {
                free: free_cost.into(),
                ..Default::default()
            },
            ..Default::default()
        };

        let initial_gas = free_cost - 1;
        let initial_allowance = free_cost + 1;

        let mut lack_gas_ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_gas(GasCounter::new(initial_gas))
                .with_allowance(GasAllowanceCounter::new(initial_allowance))
                .with_costs(ext_costs.clone())
                .build(),
        );

        assert_eq!(
            lack_gas_ext.charge_gas_for_token(CostToken::Free),
            Err(ChargeError::GasLimitExceeded),
        );

        let gas_amount = lack_gas_ext.gas_amount();
        let allowance = lack_gas_ext.context.gas_allowance_counter.left();
        // there was lack of gas
        assert_eq!(0, gas_amount.left());
        assert_eq!(initial_gas, gas_amount.burned());
        assert_eq!(initial_allowance - free_cost, allowance);

        let initial_gas = free_cost;
        let initial_allowance = free_cost - 1;

        let mut lack_allowance_ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_gas(GasCounter::new(initial_gas))
                .with_allowance(GasAllowanceCounter::new(initial_allowance))
                .with_costs(ext_costs)
                .build(),
        );

        assert_eq!(
            lack_allowance_ext.charge_gas_for_token(CostToken::Free),
            Err(ChargeError::GasAllowanceExceeded),
        );

        let gas_amount = lack_allowance_ext.gas_amount();
        let allowance = lack_allowance_ext.context.gas_allowance_counter.left();
        assert_eq!(initial_gas - free_cost, gas_amount.left());
        assert_eq!(initial_gas, gas_amount.burned());
        // there was lack of allowance
        assert_eq!(0, allowance);
    }

    #[test]
    // This function tests:
    //
    // - `send_commit` on valid handle
    // - `send_commit` on invalid handle
    // - `send_commit` on used handle
    // - `send_init` after limit is exceeded
    fn test_send_commit() {
        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_message_context(MessageContextBuilder::new().with_outgoing_limit(1).build())
                .build(),
        );

        let data = HandlePacket::default();

        let fake_handle = 0;

        let msg = ext.send_commit(fake_handle, data.clone(), 0);
        assert_eq!(
            msg.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(MessageError::OutOfBounds))
        );

        let handle = ext.send_init().expect("Outgoing limit is 1");

        let msg = ext.send_commit(handle, data.clone(), 0);
        assert!(msg.is_ok());

        let msg = ext.send_commit(handle, data, 0);
        assert_eq!(
            msg.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(MessageError::LateAccess))
        );

        let handle = ext.send_init();
        assert_eq!(
            handle.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(
                MessageError::OutgoingMessagesAmountLimitExceeded
            ))
        );
    }

    #[test]
    // This function tests:
    //
    // - `send_push` on non-existent handle
    // - `send_push` on valid handle
    // - `send_push` on used handle
    // - `send_push` with too large payload
    // - `send_push` data is added to buffer
    fn test_send_push() {
        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_message_context(MessageContextBuilder::new().build())
                .build(),
        );

        let data = HandlePacket::default();

        let fake_handle = 0;

        let res = ext.send_push(fake_handle, &[0, 0, 0]);
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(MessageError::OutOfBounds))
        );

        let handle = ext.send_init().expect("Outgoing limit is u32::MAX");

        let res = ext.send_push(handle, &[1, 2, 3]);
        assert!(res.is_ok());

        let res = ext.send_push(handle, &[4, 5, 6]);
        assert!(res.is_ok());

        let large_payload = vec![0u8; MAX_PAYLOAD_SIZE + 1];

        let res = ext.send_push(handle, &large_payload);
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(
                MessageError::MaxMessageSizeExceed
            ))
        );

        let msg = ext.send_commit(handle, data, 0);
        assert!(msg.is_ok());

        let res = ext.send_push(handle, &[7, 8, 9]);
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(MessageError::LateAccess))
        );

        let (outcome, _) = ext.context.message_context.drain();
        let ContextOutcomeDrain {
            mut outgoing_dispatches,
            ..
        } = outcome.drain();
        let dispatch = outgoing_dispatches
            .pop()
            .map(|(dispatch, _, _)| dispatch)
            .expect("Send commit was ok");

        assert_eq!(dispatch.message().payload_bytes(), &[1, 2, 3, 4, 5, 6]);
    }

    #[test]
    // This function tests:
    //
    // - `send_push_input` on non-existent handle
    // - `send_push_input` on valid handle
    // - `send_push_input` on used handle
    // - `send_push_input` data is added to buffer
    fn test_send_push_input() {
        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_message_context(
                    MessageContextBuilder::new()
                        .with_payload(vec![1, 2, 3, 4, 5, 6])
                        .build(),
                )
                .build(),
        );

        let fake_handle = 0;

        let res = ext.send_push_input(fake_handle, 0, 1);
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(MessageError::OutOfBounds))
        );

        let handle = ext.send_init().expect("Outgoing limit is u32::MAX");

        let res = ext.send_push_input(handle, 2, 3);
        assert!(res.is_ok());
        let res = ext.send_push_input(handle, 5, 1);
        assert!(res.is_ok());

        // Len too big
        let res = ext.send_push_input(handle, 0, 7);
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(
                MessageError::OutOfBoundsInputSliceLength
            ))
        );
        let res = ext.send_push_input(handle, 5, 2);
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(
                MessageError::OutOfBoundsInputSliceLength
            ))
        );

        // Too big offset
        let res = ext.send_push_input(handle, 6, 0);
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(
                MessageError::OutOfBoundsInputSliceOffset
            ))
        );

        let data = HandlePacket::default();
        let msg = ext.send_commit(handle, data, 0);
        assert!(msg.is_ok());

        let res = ext.send_push_input(handle, 0, 1);
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(MessageError::LateAccess))
        );

        let (outcome, _) = ext.context.message_context.drain();
        let ContextOutcomeDrain {
            mut outgoing_dispatches,
            ..
        } = outcome.drain();
        let dispatch = outgoing_dispatches
            .pop()
            .map(|(dispatch, _, _)| dispatch)
            .expect("Send commit was ok");

        assert_eq!(dispatch.message().payload_bytes(), &[3, 4, 5, 6]);
    }

    #[test]
    // This function requires `reply_push` to work to add extra data.
    // This function tests:
    //
    // - `reply_commit` with too much data
    // - `reply_commit` with valid data
    // - `reply_commit` duplicate reply
    fn test_reply_commit() {
        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_gas(GasCounter::new(u64::MAX))
                .with_message_context(MessageContextBuilder::new().build())
                .build(),
        );

        let res = ext.reply_push(&[0]);
        assert!(res.is_ok());

        let res = ext.reply_commit(ReplyPacket::new(Payload::repeat(0), 0));
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(
                MessageError::MaxMessageSizeExceed
            ))
        );

        let res = ext.reply_commit(ReplyPacket::auto());
        assert!(res.is_ok());

        let res = ext.reply_commit(ReplyPacket::auto());
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(MessageError::DuplicateReply))
        );
    }

    #[test]
    // This function requires `reply_push` to work to add extra data.
    // This function tests:
    //
    // - `reply_push` with valid data
    // - `reply_push` with too much data
    // - `reply_push` after `reply_commit`
    // - `reply_push` data is added to buffer
    fn test_reply_push() {
        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_gas(GasCounter::new(u64::MAX))
                .with_message_context(MessageContextBuilder::new().build())
                .build(),
        );

        let res = ext.reply_push(&[1, 2, 3]);
        assert!(res.is_ok());

        let res = ext.reply_push(&[4, 5, 6]);
        assert!(res.is_ok());

        let large_payload = vec![0u8; MAX_PAYLOAD_SIZE + 1];

        let res = ext.reply_push(&large_payload);
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(
                MessageError::MaxMessageSizeExceed
            ))
        );

        let res = ext.reply_commit(ReplyPacket::auto());
        assert!(res.is_ok());

        let res = ext.reply_push(&[7, 8, 9]);
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(MessageError::LateAccess))
        );

        let (outcome, _) = ext.context.message_context.drain();
        let ContextOutcomeDrain {
            mut outgoing_dispatches,
            ..
        } = outcome.drain();
        let dispatch = outgoing_dispatches
            .pop()
            .map(|(dispatch, _, _)| dispatch)
            .expect("Send commit was ok");

        assert_eq!(dispatch.message().payload_bytes(), &[1, 2, 3, 4, 5, 6]);
    }

    #[test]
    // This function tests:
    //
    // - `reply_push_input` with valid data
    // - `reply_push_input` after `reply_commit`
    // - `reply_push_input` data is added to buffer
    fn test_reply_push_input() {
        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_message_context(
                    MessageContextBuilder::new()
                        .with_payload(vec![1, 2, 3, 4, 5, 6])
                        .build(),
                )
                .build(),
        );

        let res = ext.reply_push_input(2, 3);
        assert!(res.is_ok());
        let res = ext.reply_push_input(5, 1);
        assert!(res.is_ok());

        // Len too big
        let res = ext.reply_push_input(0, 7);
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(
                MessageError::OutOfBoundsInputSliceLength
            ))
        );
        let res = ext.reply_push_input(5, 2);
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(
                MessageError::OutOfBoundsInputSliceLength
            ))
        );

        // Too big offset
        let res = ext.reply_push_input(6, 0);
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(
                MessageError::OutOfBoundsInputSliceOffset
            ))
        );

        let msg = ext.reply_commit(ReplyPacket::auto());
        assert!(msg.is_ok());

        let res = ext.reply_push_input(0, 1);
        assert_eq!(
            res.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Message(MessageError::LateAccess))
        );

        let (outcome, _) = ext.context.message_context.drain();
        let ContextOutcomeDrain {
            mut outgoing_dispatches,
            ..
        } = outcome.drain();
        let dispatch = outgoing_dispatches
            .pop()
            .map(|(dispatch, _, _)| dispatch)
            .expect("Send commit was ok");

        assert_eq!(dispatch.message().payload_bytes(), &[3, 4, 5, 6]);
    }

    // TODO: fix me (issue #3881)
    #[test]
    fn gas_has_gone_on_err() {
        const INIT_GAS: u64 = 1_000_000_000;

        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_message_context(
                    MessageContextBuilder::new()
                        .with_outgoing_limit(u32::MAX)
                        .build(),
                )
                .with_gas(GasCounter::new(INIT_GAS))
                .build(),
        );

        // initializing send message
        let i = ext.send_init().expect("Shouldn't fail");

        // this one fails due to lack of value, BUT [bug] gas for sending already
        // gone and no longer could be used within the execution.
        assert_eq!(
            ext.send_commit(
                i,
                HandlePacket::new_with_gas(
                    Default::default(),
                    Default::default(),
                    INIT_GAS,
                    u128::MAX
                ),
                0
            )
            .unwrap_err(),
            FallibleExecutionError::NotEnoughValue.into()
        );

        let res = ext.send_commit(
            i,
            HandlePacket::new_with_gas(Default::default(), Default::default(), INIT_GAS, 0),
            0,
        );
        assert!(res.is_ok());
    }

    // TODO: fix me (issue #3881)
    #[test]
    fn reservation_used_on_err() {
        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_message_context(
                    MessageContextBuilder::new()
                        .with_outgoing_limit(u32::MAX)
                        .build(),
                )
                .with_gas(GasCounter::new(1_000_000_000))
                .with_allowance(GasAllowanceCounter::new(1_000_000))
                .build(),
        );

        // creating reservation to be used
        let reservation_id = ext.reserve_gas(1_000_000, 1_000).expect("Shouldn't fail");

        let data = HandlePacket::default();

        // this one fails due to absence of init nonce, BUT [bug] marks reservation used,
        // so another `reservation_send_commit` fails due to used reservation.
        assert_eq!(
            ext.reservation_send_commit(reservation_id, u32::MAX, data, 0)
                .unwrap_err(),
            MessageError::OutOfBounds.into()
        );

        // initializing send message
        let i = ext.send_init().expect("Shouldn't fail");

        let data = HandlePacket::default();
        let res = ext.reservation_send_commit(reservation_id, i, data, 0);
        assert!(res.is_ok());
    }

    #[test]
    fn rollback_works() {
        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_message_context(
                    MessageContextBuilder::new()
                        .with_outgoing_limit(u32::MAX)
                        .build(),
                )
                .with_gas(GasCounter::new(1_000_000_000))
                .build(),
        );

        let reservation_id = ext.reserve_gas(1_000_000, 1_000).expect("Shouldn't fail");
        let remaining_gas = ext.context.gas_counter.to_amount();
        let remaining_gas_allowance = ext.context.gas_allowance_counter.left();
        let remaining_value_counter = ext.context.value_counter.left();
        let result = ext.with_changes::<_, (), _>(|mutator| {
            mutator.reduce_gas(42)?;
            mutator.charge_gas_if_enough(84)?; // changes gas_counter and gas_allowance_counter
            mutator.charge_message_value(128)?;
            mutator.outgoing_gasless = 1;
            mutator.mark_reservation_used(reservation_id)?;
            Err(FallibleExtError::Charge(ChargeError::GasLimitExceeded))
        });

        assert!(result.is_err());
        assert_eq!(ext.context.gas_counter.left(), remaining_gas.left());
        assert_eq!(ext.context.gas_counter.burned(), remaining_gas.burned());
        assert_eq!(
            ext.context.gas_allowance_counter.left(),
            remaining_gas_allowance
        );
        assert_eq!(ext.outgoing_gasless, 0);
        assert_eq!(ext.context.value_counter.left(), remaining_value_counter);
        assert!(matches!(
            ext.context.gas_reserver.states().get(&reservation_id),
            Some(GasReservationState::Created {
                amount: 1_000_000,
                duration: 1_000,
                used: false
            })
        ));
    }

    #[test]
    fn changes_do_apply() {
        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_message_context(
                    MessageContextBuilder::new()
                        .with_outgoing_limit(u32::MAX)
                        .build(),
                )
                .with_gas(GasCounter::new(1_000_000_000))
                .with_allowance(GasAllowanceCounter::new(1_000_000))
                .build(),
        );

        let reservation_id = ext.reserve_gas(1_000_000, 1_000).expect("Shouldn't fail");
        let remaining_gas = ext.context.gas_counter.to_amount();
        let remaining_gas_allowance = ext.context.gas_allowance_counter.left();
        let remaining_value_counter = ext.context.value_counter.left();
        let result = ext.with_changes::<_, (), FallibleExtError>(|mutator| {
            mutator.reduce_gas(42)?;
            mutator.charge_gas_if_enough(84)?; // changes gas_counter and gas_allowance_counter
            mutator.charge_message_value(128)?;
            mutator.outgoing_gasless = 1;
            mutator.mark_reservation_used(reservation_id)?;
            Ok(())
        });

        assert!(result.is_ok());
        assert_eq!(
            ext.context.gas_counter.left(),
            remaining_gas.left() - 42 - 84
        );
        assert_eq!(ext.outgoing_gasless, 1);
        assert_eq!(
            ext.context.gas_counter.burned(),
            remaining_gas.burned() + 84
        );
        assert_eq!(
            ext.context.gas_allowance_counter.left(),
            remaining_gas_allowance - 84
        );
        assert_eq!(
            ext.context.value_counter.left(),
            remaining_value_counter - 128
        );
        assert!(matches!(
            ext.context.gas_reserver.states().get(&reservation_id),
            Some(GasReservationState::Created {
                amount: 1_000_000,
                duration: 1_000,
                used: true
            })
        ));
    }

    #[test]
    // This function tests:
    //
    // - `create_program` fails due to lack of value to pay for ED
    // - `create_program` is successful
    fn test_create_program() {
        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_message_context(MessageContextBuilder::new().build())
                .with_existential_deposit(500)
                .with_value(0)
                .build(),
        );

        let data = InitPacket::default();
        let msg = ext.create_program(data.clone(), 0);
        assert_eq!(
            msg.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Execution(
                FallibleExecutionError::NotEnoughValue
            ))
        );

        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_gas(GasCounter::new(u64::MAX))
                .with_message_context(MessageContextBuilder::new().build())
                .with_existential_deposit(500)
                .with_value(1500)
                .build(),
        );

        let msg = ext.create_program(data.clone(), 0);
        assert!(msg.is_ok());
    }

    #[test]
    // This function tests:
    //
    // - `send_commit` with value greater than the ED
    // - `send_commit` with value below the ED
    fn test_send_commit_with_value() {
        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_message_context(
                    MessageContextBuilder::new()
                        .with_outgoing_limit(u32::MAX)
                        .build(),
                )
                .with_existential_deposit(500)
                .with_value(0)
                .build(),
        );

        let data = HandlePacket::new(ActorId::default(), Payload::default(), 1000);

        let handle = ext.send_init().expect("No outgoing limit");

        let msg = ext.send_commit(handle, data.clone(), 0);
        assert_eq!(
            msg.unwrap_err(),
            FallibleExtError::Core(FallibleExtErrorCore::Execution(
                FallibleExecutionError::NotEnoughValue
            ))
        );

        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_message_context(
                    MessageContextBuilder::new()
                        .with_outgoing_limit(u32::MAX)
                        .build(),
                )
                .with_existential_deposit(500)
                .with_value(5000)
                .build(),
        );

        let handle = ext.send_init().expect("No outgoing limit");
        // Sending value greater than ED is ok
        let msg = ext.send_commit(handle, data.clone(), 0);
        assert!(msg.is_ok());

        let data = HandlePacket::new(ActorId::default(), Payload::default(), 100);
        let handle = ext.send_init().expect("No outgoing limit");
        let msg = ext.send_commit(handle, data, 0);
        // Sending value below ED is also fine
        assert!(msg.is_ok());
    }

    #[test]
    fn test_unreserve_no_reimbursement() {
        let costs = ExtCosts {
            rent: RentCosts {
                reservation: CostOf::new(10),
                ..Default::default()
            },
            ..Default::default()
        };

        // Create "pre-reservation".
        let (id, gas_reservation_map) = {
            let mut m = BTreeMap::new();
            let id = ReservationId::generate(MessageId::new([5; 32]), 10);

            m.insert(
                id,
                GasReservationSlot {
                    amount: 1_000_000,
                    start: 0,
                    finish: 10,
                },
            );

            (id, m)
        };
        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_gas(GasCounter::new(u64::MAX))
                .with_message_context(MessageContextBuilder::new().build())
                .with_existential_deposit(500)
                .with_reservations_map(gas_reservation_map)
                .with_costs(costs.clone())
                .build(),
        );

        // Check all the reseravations are in "existing" state.
        assert!(
            ext.context
                .gas_reserver
                .states()
                .iter()
                .all(|(_, state)| matches!(state, GasReservationState::Exists { .. }))
        );

        // Unreserving existing and checking no gas reimbursed.
        let gas_before = ext.gas_amount();
        assert!(ext.unreserve_gas(id).is_ok());
        let gas_after = ext.gas_amount();

        assert_eq!(gas_after.left(), gas_before.left());
    }

    #[test]
    fn test_unreserve_with_reimbursement() {
        let costs = ExtCosts {
            rent: RentCosts {
                reservation: CostOf::new(10),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_gas(GasCounter::new(u64::MAX))
                .with_message_context(MessageContextBuilder::new().build())
                .with_existential_deposit(500)
                .with_costs(costs.clone())
                .build(),
        );

        // Define params
        let reservation_amount = 1_000_000;
        let duration = 10;
        let duration_cost = costs
            .rent
            .reservation
            .cost_for(ext.context.reserve_for.saturating_add(duration).into());
        let reservation_total_cost = reservation_amount + duration_cost;

        let gas_before_reservation = ext.gas_amount();
        assert_eq!(gas_before_reservation.left(), u64::MAX);

        let id = ext
            .reserve_gas(reservation_amount, duration)
            .expect("internal error: failed reservation");

        let gas_after_reservation = ext.gas_amount();
        assert_eq!(
            gas_before_reservation.left(),
            gas_after_reservation.left() + reservation_total_cost
        );

        assert!(ext.unreserve_gas(id).is_ok());

        let gas_after_unreserve = ext.gas_amount();
        assert_eq!(
            gas_after_unreserve.left(),
            gas_after_reservation.left() + reservation_total_cost
        );
        assert_eq!(gas_after_unreserve.left(), gas_before_reservation.left());
    }
}
