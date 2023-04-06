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

use crate::configs::{BlockInfo, PageCosts};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use gear_backend_common::{
    lazy_pages::{GlobalsAccessConfig, LazyPagesWeights, Status},
    memory::ProcessAccessError,
    ActorTerminationReason, BackendAllocExtError, BackendExt, BackendExtError, ExtInfo,
    SystemReservationContext, TerminationReason, TrapExplanation,
};
use gear_core::{
    costs::{HostFnWeights, RuntimeCosts},
    env::Ext as EnvExt,
    gas::{
        ChargeError, ChargeResult, CountersOwner, GasAllowanceCounter, GasAmount, GasCounter,
        GasLeft, Token, ValueCounter,
    },
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    memory::{
        AllocError, AllocationsContext, GearPage, GrowHandler, Memory, MemoryInterval,
        NoopGrowHandler, PageBuf, PageU32Size, WasmPage,
    },
    message::{
        GasLimit, HandlePacket, InitPacket, MessageContext, Packet, ReplyPacket, StatusCode,
    },
    reservation::GasReserver,
};
use gear_core_errors::{
    ExecutionError, ExtError, MemoryError, MessageError, ReservationError, WaitError,
};
use gear_wasm_instrument::syscalls::SysCallName;

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
    /// Max allowed wasm memory pages.
    pub max_pages: WasmPage,
    /// Allocations config.
    pub page_costs: PageCosts,
    /// Account existential deposit
    pub existential_deposit: u128,
    /// Communication origin
    pub origin: ProgramId,
    /// Current program id
    pub program_id: ProgramId,
    /// Map of code hashes to program ids of future programs, which are planned to be
    /// initialized with the corresponding code (with the same code hash).
    pub program_candidates_data: BTreeMap<CodeId, Vec<(MessageId, ProgramId)>>,
    /// Weights of host functions.
    pub host_fn_weights: HostFnWeights,
    /// Functions forbidden to be called.
    pub forbidden_funcs: BTreeSet<SysCallName>,
    /// Mailbox threshold.
    pub mailbox_threshold: u64,
    /// Cost for single block waitlist holding.
    pub waitlist_cost: u64,
    /// Cost of holding a message in dispatch stash.
    pub dispatch_hold_cost: u64,
    /// Reserve for parameter of scheduling.
    pub reserve_for: u32,
    /// Cost for reservation holding.
    pub reservation: u64,
    /// Output from Randomness.
    pub random_data: (Vec<u8>, u32),
}

/// Trait to which ext must have to work in processor wasm executor.
/// Currently used only for lazy-pages support.
pub trait ProcessorExt {
    /// Whether this extension works with lazy pages.
    const LAZY_PAGES_ENABLED: bool;

    /// Create new
    fn new(context: ProcessorContext) -> Self;

    /// Protect and save storage keys for pages which has no data
    fn lazy_pages_init_for_program(
        mem: &mut impl Memory,
        prog_id: ProgramId,
        stack_end: Option<WasmPage>,
        globals_config: GlobalsAccessConfig,
        lazy_pages_weights: LazyPagesWeights,
    );

    /// Lazy pages contract post execution actions
    fn lazy_pages_post_execution_actions(mem: &mut impl Memory);

    /// Returns lazy pages status
    fn lazy_pages_status() -> Status;
}

/// [`Ext`](Ext)'s error
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Display, derive_more::From)]
pub enum ProcessorError {
    /// Basic error
    #[display(fmt = "{_0}")]
    Core(ExtError),
    /// An error occurs in attempt to call forbidden sys-call.
    #[display(fmt = "Unable to call a forbidden function")]
    ForbiddenFunction,
    /// Charge error
    #[display(fmt = "Charge error: {_0}")]
    Charge(ChargeError),
}

impl From<MessageError> for ProcessorError {
    fn from(err: MessageError) -> Self {
        Self::Core(ExtError::Message(err))
    }
}

impl From<MemoryError> for ProcessorError {
    fn from(err: MemoryError) -> Self {
        Self::Core(ExtError::Memory(err))
    }
}

impl From<WaitError> for ProcessorError {
    fn from(err: WaitError) -> Self {
        Self::Core(ExtError::Wait(err))
    }
}

impl From<ReservationError> for ProcessorError {
    fn from(err: ReservationError) -> Self {
        Self::Core(ExtError::Reservation(err))
    }
}

impl From<ExecutionError> for ProcessorError {
    fn from(err: ExecutionError) -> Self {
        Self::Core(ExtError::Execution(err))
    }
}

impl BackendExtError for ProcessorError {
    fn into_termination_reason(self) -> TerminationReason {
        match self {
            ProcessorError::Core(err) => {
                ActorTerminationReason::Trap(TrapExplanation::Ext(err)).into()
            }
            ProcessorError::ForbiddenFunction => {
                ActorTerminationReason::Trap(TrapExplanation::ForbiddenFunction).into()
            }
            ProcessorError::Charge(err) => err.into(),
        }
    }
}

/// [`Ext`](Ext)'s error
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Display, derive_more::From)]
pub enum ProcessorAllocError {
    /// Charge error
    #[display(fmt = "{_0}")]
    Charge(ChargeError),
    /// Allocation error
    #[display(fmt = "{_0}")]
    Alloc(AllocError),
}

impl BackendAllocExtError for ProcessorAllocError {
    type ExtError = ProcessorError;

    fn into_backend_error(self) -> Result<Self::ExtError, Self> {
        match self {
            Self::Charge(err) => Ok(err.into()),
            err => Err(err),
        }
    }
}
/// Structure providing externalities for running host functions.
pub struct Ext {
    /// Processor context.
    pub context: ProcessorContext,
    // Counter of outgoing gasless messages.
    //
    // It's temporary field, used to solve `core-audit/issue#22`.
    outgoing_gasless: u64,
}

/// Empty implementation for non-substrate (and non-lazy-pages) using
impl ProcessorExt for Ext {
    const LAZY_PAGES_ENABLED: bool = false;

    fn new(context: ProcessorContext) -> Self {
        Self {
            context,
            outgoing_gasless: 0,
        }
    }

    fn lazy_pages_init_for_program(
        _mem: &mut impl Memory,
        _prog_id: ProgramId,
        _stack_end: Option<WasmPage>,
        _globals_config: GlobalsAccessConfig,
        _lazy_pages_weights: LazyPagesWeights,
    ) {
        unreachable!("Must not be called: lazy-pages is unsupported by this ext")
    }

    fn lazy_pages_post_execution_actions(_mem: &mut impl Memory) {
        unreachable!("Must not be called: lazy-pages is unsupported by this ext")
    }

    fn lazy_pages_status() -> Status {
        unreachable!("Must not be called: lazy-pages is unsupported by this ext")
    }
}

impl BackendExt for Ext {
    fn into_ext_info(self, memory: &impl Memory) -> Result<ExtInfo, MemoryError> {
        let pages_for_data =
            |static_pages: WasmPage, allocations: &BTreeSet<WasmPage>| -> Vec<GearPage> {
                static_pages
                    .iter_from_zero()
                    .chain(allocations.iter().copied())
                    .flat_map(|p| p.to_pages_iter())
                    .collect()
            };

        self.into_ext_info_inner(memory, pages_for_data)
    }

    fn gas_amount(&self) -> GasAmount {
        self.context.gas_counter.to_amount()
    }

    fn pre_process_memory_accesses(
        _reads: &[MemoryInterval],
        _writes: &[MemoryInterval],
        _gas_left: &mut GasLeft,
    ) -> Result<(), ProcessAccessError> {
        Ok(())
    }
}

impl Ext {
    fn check_message_value(&mut self, message_value: u128) -> Result<(), ProcessorError> {
        let existential_deposit = self.context.existential_deposit;
        // Sending value should apply the range {0} ∪ [existential_deposit; +inf)
        if message_value != 0 && message_value < existential_deposit {
            Err(MessageError::InsufficientValue {
                message_value,
                existential_deposit,
            }
            .into())
        } else {
            Ok(())
        }
    }

    fn check_gas_limit(&mut self, gas_limit: Option<GasLimit>) -> Result<GasLimit, ProcessorError> {
        let mailbox_threshold = self.context.mailbox_threshold;
        let gas_limit = gas_limit.unwrap_or(0);

        // Sending gas should apply the range {0} ∪ [mailbox_threshold; +inf)
        if gas_limit < mailbox_threshold && gas_limit != 0 {
            Err(MessageError::InsufficientGasLimit {
                message_gas_limit: gas_limit,
                mailbox_threshold,
            }
            .into())
        } else {
            Ok(gas_limit)
        }
    }

    fn reduce_gas(&mut self, gas_limit: GasLimit) -> Result<(), ProcessorError> {
        if self.context.gas_counter.reduce(gas_limit) != ChargeResult::Enough {
            Err(MessageError::NotEnoughGas.into())
        } else {
            Ok(())
        }
    }

    fn charge_message_value(&mut self, message_value: u128) -> Result<(), ProcessorError> {
        if self.context.value_counter.reduce(message_value) != ChargeResult::Enough {
            Err(MessageError::NotEnoughValue {
                message_value,
                value_left: self.context.value_counter.left(),
            }
            .into())
        } else {
            Ok(())
        }
    }

    // It's temporary fn, used to solve `core-audit/issue#22`.
    fn safe_gasfull_sends<T: Packet>(&mut self, packet: &T) -> Result<(), ProcessorError> {
        let outgoing_gasless = self.outgoing_gasless;

        match packet.gas_limit() {
            Some(x) if x != 0 => {
                self.outgoing_gasless = 0;

                let prev_gasless_fee =
                    outgoing_gasless.saturating_mul(self.context.mailbox_threshold);

                self.reduce_gas(prev_gasless_fee)?;
            }
            None => self.outgoing_gasless = outgoing_gasless.saturating_add(1),
            _ => {}
        };

        Ok(())
    }

    fn charge_expiring_resources<T: Packet>(&mut self, packet: &T) -> Result<(), ProcessorError> {
        self.check_message_value(packet.value())?;
        // Charge for using expiring resources. Charge for calling sys-call was done earlier.
        let gas_limit = self.check_gas_limit(packet.gas_limit())?;
        self.reduce_gas(gas_limit)?;
        self.charge_message_value(packet.value())?;
        Ok(())
    }

    fn check_forbidden_destination(&mut self, id: ProgramId) -> Result<(), ProcessorError> {
        if id == ProgramId::SYSTEM {
            Err(ProcessorError::ForbiddenFunction)
        } else {
            Ok(())
        }
    }

    fn charge_sending_fee(&mut self, delay: u32) -> Result<(), ChargeError> {
        if delay == 0 {
            self.charge_gas_if_enough(self.context.message_context.settings().sending_fee())
        } else {
            self.charge_gas_if_enough(
                self.context
                    .message_context
                    .settings()
                    .scheduled_sending_fee(),
            )
        }
    }

    fn charge_for_dispatch_stash_hold(&mut self, delay: u32) -> Result<(), ProcessorError> {
        if delay != 0 {
            // Take delay and get cost of block.
            // reserve = wait_cost * (delay + reserve_for).
            let cost_per_block = self.context.dispatch_hold_cost;
            let waiting_reserve = (self.context.reserve_for as u64)
                .saturating_add(delay as u64)
                .saturating_mul(cost_per_block);

            // Reduce gas for block waiting in dispatch stash.
            if self.context.gas_counter.reduce(waiting_reserve) != ChargeResult::Enough {
                return Err(MessageError::InsufficientGasForDelayedSending.into());
            }
        }
        Ok(())
    }

    fn charge_gas_if_enough(
        gas_counter: &mut GasCounter,
        gas_allowance_counter: &mut GasAllowanceCounter,
        amount: u64,
    ) -> Result<(), ChargeError> {
        if gas_counter.charge_if_enough(amount) != ChargeResult::Enough {
            return Err(ChargeError::GasLimitExceeded);
        }
        if gas_allowance_counter.charge_if_enough(amount) != ChargeResult::Enough {
            if gas_counter.refund(amount) != ChargeResult::Enough {
                // We have just charged `amount` from `self.gas_counter`, so this must be correct.
                unreachable!("Cannot refund {amount} for `gas_counter`");
            }
            return Err(ChargeError::GasAllowanceExceeded);
        }
        Ok(())
    }
}

impl CountersOwner for Ext {
    fn charge_gas_runtime(&mut self, cost: RuntimeCosts) -> Result<(), ChargeError> {
        let token = cost.token(&self.context.host_fn_weights);
        let common_charge = self.context.gas_counter.charge(token);
        let allowance_charge = self.context.gas_allowance_counter.charge(token);
        match (common_charge, allowance_charge) {
            (ChargeResult::NotEnough, _) => Err(ChargeError::GasLimitExceeded),
            (ChargeResult::Enough, ChargeResult::NotEnough) => {
                Err(ChargeError::GasAllowanceExceeded)
            }
            (ChargeResult::Enough, ChargeResult::Enough) => Ok(()),
        }
    }

    fn charge_gas_runtime_if_enough(&mut self, cost: RuntimeCosts) -> Result<(), ChargeError> {
        let amount = cost.token(&self.context.host_fn_weights).weight();
        self.charge_gas_if_enough(amount)
    }

    fn charge_gas_if_enough(&mut self, amount: u64) -> Result<(), ChargeError> {
        Ext::charge_gas_if_enough(
            &mut self.context.gas_counter,
            &mut self.context.gas_allowance_counter,
            amount,
        )
    }

    fn refund_gas(&mut self, amount: u64) -> Result<(), ChargeError> {
        if self.context.gas_counter.refund(amount) == ChargeResult::Enough {
            self.context.gas_allowance_counter.refund(amount);
            Ok(())
        } else {
            Err(ChargeError::TooManyGasAdded)
        }
    }

    fn gas_left(&self) -> GasLeft {
        GasLeft {
            gas: self.context.gas_counter.left(),
            allowance: self.context.gas_allowance_counter.left(),
        }
    }

    fn set_gas_left(&mut self, gas_left: GasLeft) {
        let GasLeft { gas, allowance } = gas_left;

        let gas_left = self.context.gas_counter.left();
        if gas_left > gas {
            if self.context.gas_counter.charge_if_enough(gas_left - gas) != ChargeResult::Enough {
                // We checked above that `gas_left` is bigger than `gas`
                unreachable!("Cannot charge {gas} from `gas_counter`");
            }
        } else {
            self.context.gas_counter.refund(gas - gas_left);
        }

        let allowance_left = self.context.gas_allowance_counter.left();
        if allowance_left > allowance {
            if self
                .context
                .gas_allowance_counter
                .charge_if_enough(allowance_left - allowance)
                != ChargeResult::Enough
            {
                // We checked above that `allowance_left` is bigger than `allowance`
                unreachable!("Cannot charge {allowance} from `gas_allowance_counter`");
            }
        } else {
            self.context
                .gas_allowance_counter
                .refund(allowance - allowance_left);
        }
    }
}

impl EnvExt for Ext {
    type Error = ProcessorError;
    type AllocError = ProcessorAllocError;

    fn alloc(
        &mut self,
        pages_num: WasmPage,
        mem: &mut impl Memory,
    ) -> Result<WasmPage, Self::AllocError> {
        self.alloc_inner::<NoopGrowHandler>(pages_num, mem)
    }

    fn free(&mut self, page: WasmPage) -> Result<(), Self::AllocError> {
        self.context
            .allocations_context
            .free(page)
            .map_err(Into::into)
    }

    fn block_height(&mut self) -> Result<u32, Self::Error> {
        Ok(self.context.block_info.height)
    }

    fn block_timestamp(&mut self) -> Result<u64, Self::Error> {
        Ok(self.context.block_info.timestamp)
    }

    fn origin(&mut self) -> Result<gear_core::ids::ProgramId, Self::Error> {
        Ok(self.context.origin)
    }

    fn send_init(&mut self) -> Result<u32, Self::Error> {
        let handle = self.context.message_context.send_init()?;
        Ok(handle)
    }

    fn send_push(&mut self, handle: u32, buffer: &[u8]) -> Result<(), Self::Error> {
        self.context.message_context.send_push(handle, buffer)?;
        Ok(())
    }

    fn send_push_input(&mut self, handle: u32, offset: u32, len: u32) -> Result<(), Self::Error> {
        let range = self.context.message_context.check_input_range(offset, len);
        self.charge_gas_runtime_if_enough(RuntimeCosts::SendPushInputPerByte(range.len()))?;

        self.context
            .message_context
            .send_push_input(handle, range)?;

        Ok(())
    }

    fn send_commit(
        &mut self,
        handle: u32,
        msg: HandlePacket,
        delay: u32,
    ) -> Result<MessageId, Self::Error> {
        self.check_forbidden_destination(msg.destination())?;
        self.safe_gasfull_sends(&msg)?;
        self.charge_expiring_resources(&msg)?;
        self.charge_sending_fee(delay)?;

        self.charge_for_dispatch_stash_hold(delay)?;

        let msg_id = self
            .context
            .message_context
            .send_commit(handle, msg, delay, None)?;

        Ok(msg_id)
    }

    fn reservation_send_commit(
        &mut self,
        id: ReservationId,
        handle: u32,
        msg: HandlePacket,
        delay: u32,
    ) -> Result<MessageId, Self::Error> {
        self.check_forbidden_destination(msg.destination())?;
        self.check_message_value(msg.value())?;
        self.check_gas_limit(msg.gas_limit())?;
        // TODO: gasful sending (#1828)
        self.charge_message_value(msg.value())?;
        self.charge_sending_fee(delay)?;

        self.charge_for_dispatch_stash_hold(delay)?;

        self.context.gas_reserver.mark_used(id)?;

        let msg_id = self
            .context
            .message_context
            .send_commit(handle, msg, delay, Some(id))?;
        Ok(msg_id)
    }

    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Self::Error> {
        self.context.message_context.reply_push(buffer)?;
        Ok(())
    }

    // TODO: Consider per byte charge (issue #2255).
    fn reply_commit(&mut self, msg: ReplyPacket, delay: u32) -> Result<MessageId, Self::Error> {
        self.check_forbidden_destination(self.context.message_context.reply_destination())?;
        self.safe_gasfull_sends(&msg)?;
        self.charge_expiring_resources(&msg)?;
        self.charge_sending_fee(delay)?;

        self.charge_for_dispatch_stash_hold(delay)?;

        let msg_id = self
            .context
            .message_context
            .reply_commit(msg, delay, None)?;
        Ok(msg_id)
    }

    fn reservation_reply_commit(
        &mut self,
        id: ReservationId,
        msg: ReplyPacket,
        delay: u32,
    ) -> Result<MessageId, Self::Error> {
        self.check_forbidden_destination(self.context.message_context.reply_destination())?;
        self.check_message_value(msg.value())?;
        self.check_gas_limit(msg.gas_limit())?;
        // TODO: gasful sending (#1828)
        self.charge_message_value(msg.value())?;
        self.charge_sending_fee(delay)?;

        self.charge_for_dispatch_stash_hold(delay)?;

        self.context.gas_reserver.mark_used(id)?;

        let msg_id = self
            .context
            .message_context
            .reply_commit(msg, delay, Some(id))?;
        Ok(msg_id)
    }

    fn reply_to(&mut self) -> Result<MessageId, Self::Error> {
        self.context
            .message_context
            .current()
            .details()
            .and_then(|d| d.to_reply_details())
            .map(|d| d.into_reply_to())
            .ok_or_else(|| MessageError::NoReplyContext.into())
    }

    fn signal_from(&mut self) -> Result<MessageId, Self::Error> {
        self.context
            .message_context
            .current()
            .details()
            .and_then(|d| d.to_signal_details())
            .map(|d| d.from())
            .ok_or_else(|| MessageError::NoSignalContext.into())
    }

    fn reply_push_input(&mut self, offset: u32, len: u32) -> Result<(), Self::Error> {
        let range = self.context.message_context.check_input_range(offset, len);
        self.charge_gas_runtime_if_enough(RuntimeCosts::ReplyPushInputPerByte(range.len()))?;

        self.context.message_context.reply_push_input(range)?;

        Ok(())
    }

    fn source(&mut self) -> Result<ProgramId, Self::Error> {
        Ok(self.context.message_context.current().source())
    }

    fn status_code(&mut self) -> Result<StatusCode, Self::Error> {
        self.context
            .message_context
            .current()
            .details()
            .map(|d| d.status_code())
            .ok_or_else(|| MessageError::NoStatusCodeContext.into())
    }

    fn message_id(&mut self) -> Result<MessageId, Self::Error> {
        Ok(self.context.message_context.current().id())
    }

    fn program_id(&mut self) -> Result<ProgramId, Self::Error> {
        Ok(self.context.program_id)
    }

    fn debug(&mut self, data: &str) -> Result<(), Self::Error> {
        log::debug!(target: "gwasm", "DEBUG: {}", data);
        Ok(())
    }

    fn read(&mut self, at: u32, len: u32) -> Result<(&[u8], GasLeft), Self::Error> {
        // Verify read is correct
        let end = at
            .checked_add(len)
            .ok_or(MessageError::TooBigReadLen { at, len })?;
        self.charge_gas_runtime_if_enough(RuntimeCosts::ReadPerByte(len))?;
        let msg = self.context.message_context.current().payload();
        if end as usize > msg.len() {
            return Err(MessageError::ReadWrongRange {
                start: at,
                end,
                msg_len: msg.len() as u32,
            }
            .into());
        }

        Ok((&msg[at as usize..end as usize], self.gas_left()))
    }

    fn size(&mut self) -> Result<usize, Self::Error> {
        Ok(self.context.message_context.current().payload().len())
    }

    fn reserve_gas(&mut self, amount: u64, duration: u32) -> Result<ReservationId, Self::Error> {
        self.charge_gas_if_enough(self.context.message_context.settings().reservation_fee())?;

        if duration == 0 {
            return Err(ReservationError::ZeroReservationDuration.into());
        }

        if amount < self.context.mailbox_threshold {
            return Err(ReservationError::ReservationBelowMailboxThreshold.into());
        }

        let reserve = u64::from(self.context.reserve_for.saturating_add(duration))
            .saturating_mul(self.context.reservation);
        let reduce_amount = amount.saturating_add(reserve);
        if self.context.gas_counter.reduce(reduce_amount) == ChargeResult::NotEnough {
            return Err(ReservationError::InsufficientGasForReservation.into());
        }

        let id = self.context.gas_reserver.reserve(amount, duration)?;

        Ok(id)
    }

    fn unreserve_gas(&mut self, id: ReservationId) -> Result<u64, Self::Error> {
        let amount = self.context.gas_reserver.unreserve(id)?;

        // this statement is like in `Self::refund_gas()` but it won't affect "burned" counter
        // because we don't actually refund we just rise "left" counter during unreservation
        // and it won't affect gas allowance counter because we don't make any actual calculations
        // TODO: uncomment when unreserving in current message features is discussed
        /*if !self.context.gas_counter.increase(amount) {
            return Err(ExecutionError::TooManyGasAdded.into());
        }*/

        Ok(amount)
    }

    fn system_reserve_gas(&mut self, amount: u64) -> Result<(), Self::Error> {
        // TODO: use `NonZeroU64` after issue #1838 is fixed
        if amount == 0 {
            return Err(ReservationError::ZeroReservationAmount.into());
        }

        if self.context.gas_counter.reduce(amount) == ChargeResult::NotEnough {
            return Err(ReservationError::InsufficientGasForReservation.into());
        }

        let reservation = &mut self.context.system_reservation;
        *reservation = reservation
            .map(|reservation| reservation.saturating_add(amount))
            .or(Some(amount));

        Ok(())
    }

    fn gas_available(&mut self) -> Result<u64, Self::Error> {
        Ok(self.context.gas_counter.left())
    }

    fn value(&mut self) -> Result<u128, Self::Error> {
        Ok(self.context.message_context.current().value())
    }

    fn value_available(&mut self) -> Result<u128, Self::Error> {
        Ok(self.context.value_counter.left())
    }

    fn wait(&mut self) -> Result<(), Self::Error> {
        self.charge_gas_if_enough(self.context.message_context.settings().waiting_fee())?;

        let reserve = u64::from(self.context.reserve_for.saturating_add(1))
            .saturating_mul(self.context.waitlist_cost);

        if self.context.gas_counter.reduce(reserve) != ChargeResult::Enough {
            return Err(WaitError::NotEnoughGas.into());
        }

        Ok(())
    }

    fn wait_for(&mut self, duration: u32) -> Result<(), Self::Error> {
        self.charge_gas_if_enough(self.context.message_context.settings().waiting_fee())?;

        if duration == 0 {
            return Err(WaitError::InvalidArgument.into());
        }

        let reserve = u64::from(self.context.reserve_for.saturating_add(duration))
            .saturating_mul(self.context.waitlist_cost);

        if self.context.gas_counter.reduce(reserve) != ChargeResult::Enough {
            return Err(WaitError::NotEnoughGas.into());
        }

        Ok(())
    }

    fn wait_up_to(&mut self, duration: u32) -> Result<bool, Self::Error> {
        self.charge_gas_if_enough(self.context.message_context.settings().waiting_fee())?;

        if duration == 0 {
            return Err(WaitError::InvalidArgument.into());
        }

        let reserve = u64::from(self.context.reserve_for.saturating_add(1))
            .saturating_mul(self.context.waitlist_cost);

        if self.context.gas_counter.reduce(reserve) != ChargeResult::Enough {
            return Err(WaitError::NotEnoughGas.into());
        }

        let reserve_full = u64::from(self.context.reserve_for.saturating_add(duration))
            .saturating_mul(self.context.waitlist_cost);
        let reserve_diff = reserve_full - reserve;

        Ok(self.context.gas_counter.reduce(reserve_diff) == ChargeResult::Enough)
    }

    fn wake(&mut self, waker_id: MessageId, delay: u32) -> Result<(), Self::Error> {
        self.charge_gas_if_enough(self.context.message_context.settings().waking_fee())?;

        self.context.message_context.wake(waker_id, delay)?;
        Ok(())
    }

    fn create_program(
        &mut self,
        packet: InitPacket,
        delay: u32,
    ) -> Result<(MessageId, ProgramId), Self::Error> {
        self.check_forbidden_destination(packet.destination())?;
        self.safe_gasfull_sends(&packet)?;
        self.charge_expiring_resources(&packet)?;
        self.charge_sending_fee(delay)?;

        self.charge_for_dispatch_stash_hold(delay)?;

        let code_hash = packet.code_id();

        // Send a message for program creation
        let (mid, pid) = self
            .context
            .message_context
            .init_program(packet, delay)
            .map(|(init_msg_id, new_prog_id)| {
                // Save a program candidate for this run
                let entry = self
                    .context
                    .program_candidates_data
                    .entry(code_hash)
                    .or_default();
                entry.push((init_msg_id, new_prog_id));

                (init_msg_id, new_prog_id)
            })?;
        Ok((mid, pid))
    }

    fn random(&mut self) -> Result<(&[u8], u32), Self::Error> {
        Ok((&self.context.random_data.0, self.context.random_data.1))
    }

    fn forbidden_funcs(&self) -> &BTreeSet<SysCallName> {
        &self.context.forbidden_funcs
    }
}

impl Ext {
    /// Inner alloc realization.
    pub fn alloc_inner<G: GrowHandler>(
        &mut self,
        pages: WasmPage,
        mem: &mut impl Memory,
    ) -> Result<WasmPage, ProcessorAllocError> {
        self.context
            .allocations_context
            .alloc::<G>(pages, mem, |pages| {
                Ext::charge_gas_if_enough(
                    &mut self.context.gas_counter,
                    &mut self.context.gas_allowance_counter,
                    self.context.page_costs.mem_grow.calc(pages),
                )
            })
            .map_err(Into::into)
    }

    /// Into ext info inner impl.
    /// `pages_for_data` returns vector of pages which data will be stored in info.
    pub fn into_ext_info_inner(
        self,
        memory: &impl Memory,
        pages_for_data: impl FnOnce(WasmPage, &BTreeSet<WasmPage>) -> Vec<GearPage>,
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

        let (static_pages, initial_allocations, allocations) = allocations_context.into_parts();
        let mut pages_data = BTreeMap::new();
        for page in pages_for_data(static_pages, &allocations) {
            let mut buf = PageBuf::new_zeroed();
            memory.read(page.offset(), &mut buf)?;
            pages_data.insert(page, buf);
        }

        let (outcome, mut context_store) = message_context.drain();
        let (generated_dispatches, awakening) = outcome.drain();

        let system_reservation_context = SystemReservationContext {
            current_reservation: system_reservation,
            previous_reservation: context_store.system_reservation(),
        };

        context_store.set_reservation_nonce(gas_reserver.nonce());
        if let Some(reservation) = system_reservation {
            context_store.add_system_reservation(reservation);
        }

        let info = ExtInfo {
            gas_amount: gas_counter.to_amount(),
            gas_reserver,
            system_reservation_context,
            allocations: (allocations != initial_allocations)
                .then_some(allocations)
                .unwrap_or_default(),
            pages_data,
            generated_dispatches,
            awakening,
            context_store,
            program_candidates_data,
        };
        Ok(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gear_core::message::ContextSettings;

    struct ProcessorContextBuilder(ProcessorContext);

    impl ProcessorContextBuilder {
        fn new() -> Self {
            let default_pc = ProcessorContext {
                gas_counter: GasCounter::new(0),
                gas_allowance_counter: GasAllowanceCounter::new(0),
                gas_reserver: GasReserver::new(
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                ),
                system_reservation: None,
                value_counter: ValueCounter::new(0),
                allocations_context: AllocationsContext::new(
                    Default::default(),
                    Default::default(),
                    Default::default(),
                ),
                message_context: MessageContext::new(
                    Default::default(),
                    Default::default(),
                    None,
                    ContextSettings::new(0, 0, 0, 0, 0, 0),
                ),
                block_info: Default::default(),
                max_pages: 512.into(),
                page_costs: PageCosts::new_for_tests(),
                existential_deposit: 0,
                origin: Default::default(),
                program_id: Default::default(),
                program_candidates_data: Default::default(),
                host_fn_weights: Default::default(),
                forbidden_funcs: Default::default(),
                mailbox_threshold: 0,
                waitlist_cost: 0,
                dispatch_hold_cost: 0,
                reserve_for: 0,
                reservation: 0,
                random_data: ([0u8; 32].to_vec(), 0),
            };

            Self(default_pc)
        }

        fn with_gas(mut self, gas_counter: GasCounter) -> Self {
            self.0.gas_counter = gas_counter;

            self
        }

        fn with_allowance(mut self, gas_allowance_counter: GasAllowanceCounter) -> Self {
            self.0.gas_allowance_counter = gas_allowance_counter;

            self
        }

        fn with_weighs(mut self, weights: HostFnWeights) -> Self {
            self.0.host_fn_weights = weights;

            self
        }

        fn with_allocation_context(mut self, ctx: AllocationsContext) -> Self {
            self.0.allocations_context = ctx;

            self
        }

        fn build(self) -> ProcessorContext {
            self.0
        }
    }

    // Invariant: Refund never occurs in `free` call.
    #[test]
    fn free_no_refund() {
        // Set initial Ext state
        let initial_gas = 100;
        let initial_allowance = 10000;

        let gas_left = GasLeft {
            gas: initial_gas,
            allowance: initial_allowance,
        };

        let existing_page = 99.into();
        let non_existing_page = 100.into();

        let allocations_context =
            AllocationsContext::new(BTreeSet::from([existing_page]), 1.into(), 512.into());

        let mut ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_gas(GasCounter::new(initial_gas))
                .with_allowance(GasAllowanceCounter::new(initial_allowance))
                .with_allocation_context(allocations_context)
                .build(),
        );

        // Freeing existing page.
        // Counters still shouldn't be changed.
        assert!(ext.free(existing_page).is_ok());
        assert_eq!(ext.gas_left(), gas_left);

        // Freeing non existing page.
        // Counters shouldn't be changed.
        assert_eq!(
            ext.free(non_existing_page),
            Err(ProcessorAllocError::Alloc(AllocError::InvalidFree(
                non_existing_page.raw()
            )))
        );
        assert_eq!(ext.gas_left(), gas_left);
    }

    #[test]
    fn test_counter_zeroes() {
        // Set initial Ext state
        let free_weight = 1000;
        let host_fn_weights = HostFnWeights {
            free: free_weight,
            ..Default::default()
        };

        let initial_gas = free_weight - 1;
        let initial_allowance = free_weight + 1;

        let mut lack_gas_ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_gas(GasCounter::new(initial_gas))
                .with_allowance(GasAllowanceCounter::new(initial_allowance))
                .with_weighs(host_fn_weights.clone())
                .build(),
        );

        assert_eq!(
            lack_gas_ext.charge_gas_runtime(RuntimeCosts::Free),
            Err(ChargeError::GasLimitExceeded),
        );

        let gas_amount = lack_gas_ext.gas_amount();
        let allowance = lack_gas_ext.context.gas_allowance_counter.left();
        // there was lack of gas
        assert_eq!(0, gas_amount.left());
        assert_eq!(initial_gas, gas_amount.burned());
        assert_eq!(initial_allowance - free_weight, allowance);

        let initial_gas = free_weight;
        let initial_allowance = free_weight - 1;

        let mut lack_allowance_ext = Ext::new(
            ProcessorContextBuilder::new()
                .with_gas(GasCounter::new(initial_gas))
                .with_allowance(GasAllowanceCounter::new(initial_allowance))
                .with_weighs(host_fn_weights)
                .build(),
        );

        assert_eq!(
            lack_allowance_ext.charge_gas_runtime(RuntimeCosts::Free),
            Err(ChargeError::GasAllowanceExceeded),
        );

        let gas_amount = lack_allowance_ext.gas_amount();
        let allowance = lack_allowance_ext.context.gas_allowance_counter.left();
        assert_eq!(initial_gas - free_weight, gas_amount.left());
        assert_eq!(initial_gas, gas_amount.burned());
        // there was lack of allowance
        assert_eq!(0, allowance);
    }

    mod property_tests {
        use super::*;
        use gear_core::memory::{HostPointer, PageError};
        use proptest::{
            arbitrary::any,
            collection::size_range,
            prop_oneof, proptest,
            strategy::{Just, Strategy},
            test_runner::Config as ProptestConfig,
        };

        struct TestMemory(WasmPage);

        impl Memory for TestMemory {
            type GrowError = PageError;

            fn grow(&mut self, pages: WasmPage) -> Result<(), Self::GrowError> {
                self.0 = self.0.add(pages)?;
                Ok(())
            }

            fn size(&self) -> WasmPage {
                self.0
            }

            fn write(&mut self, _offset: u32, _buffer: &[u8]) -> Result<(), MemoryError> {
                unimplemented!()
            }

            fn read(&self, _offset: u32, _buffer: &mut [u8]) -> Result<(), MemoryError> {
                unimplemented!()
            }

            unsafe fn get_buffer_host_addr_unsafe(&mut self) -> HostPointer {
                unimplemented!()
            }
        }

        #[derive(Debug, Clone)]
        enum Action {
            Alloc { pages: WasmPage },
            Free { page: WasmPage },
        }

        fn actions() -> impl Strategy<Value = Vec<Action>> {
            let action = wasm_page_number().prop_flat_map(|page| {
                prop_oneof![
                    Just(Action::Alloc { pages: page }),
                    Just(Action::Free { page })
                ]
            });
            proptest::collection::vec(action, 0..1024)
        }

        fn allocations() -> impl Strategy<Value = BTreeSet<WasmPage>> {
            proptest::collection::btree_set(wasm_page_number(), size_range(0..1024))
        }

        fn wasm_page_number() -> impl Strategy<Value = WasmPage> {
            any::<u16>().prop_map(WasmPage::from)
        }

        fn proptest_config() -> ProptestConfig {
            ProptestConfig {
                cases: 1024,
                ..Default::default()
            }
        }

        #[track_caller]
        fn assert_alloc_error(err: <Ext as EnvExt>::AllocError) {
            match err {
                ProcessorAllocError::Alloc(
                    AllocError::IncorrectAllocationData(_) | AllocError::ProgramAllocOutOfBounds,
                ) => {}
                err => Err(err).unwrap(),
            }
        }

        #[track_caller]
        fn assert_free_error(err: <Ext as EnvExt>::AllocError) {
            match err {
                ProcessorAllocError::Alloc(AllocError::InvalidFree(_)) => {}
                err => Err(err).unwrap(),
            }
        }

        proptest! {
            #![proptest_config(proptest_config())]
            #[test]
            fn alloc(
                static_pages in wasm_page_number(),
                allocations in allocations(),
                max_pages in wasm_page_number(),
                mem_size in wasm_page_number(),
                actions in actions(),
            ) {
                let _ = env_logger::try_init();

                let ctx = AllocationsContext::new(allocations, static_pages, max_pages);
                let ctx = ProcessorContextBuilder::new()
                    .with_gas(GasCounter::new(u64::MAX))
                    .with_allowance(GasAllowanceCounter::new(u64::MAX))
                    .with_allocation_context(ctx)
                    .build();
                let mut ext = Ext::new(ctx);
                let mut mem = TestMemory(mem_size);

                for action in actions {
                    match action {
                        Action::Alloc { pages } => {
                            if let Err(err) = ext.alloc(pages, &mut mem) {
                                assert_alloc_error(err);
                            }
                        }
                        Action::Free { page } => {
                            if let Err(err) = ext.free(page) {
                                assert_free_error(err);
                            }
                        },
                    }
                }
            }
        }
    }
}
