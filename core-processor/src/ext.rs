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

use crate::configs::{AllocationsConfig, BlockInfo};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec::Vec,
};
use codec::{Decode, Encode};
use gear_backend_common::{
    error_processor::IntoExtError, AsTerminationReason, ExtInfo, GetGasAmount, IntoExtInfo,
    TerminationReason, TrapExplanation,
};
use gear_core::{
    charge_gas_token,
    costs::{HostFnWeights, RuntimeCosts},
    env::Ext as EnvExt,
    gas::{ChargeResult, GasAllowanceCounter, GasAmount, GasCounter, ValueCounter},
    ids::{CodeId, MessageId, ProgramId, ReservationId},
    memory::{
        AllocationsContext, GrowHandler, GrowHandlerNothing, Memory, PageBuf, PageNumber,
        WasmPageNumber,
    },
    message::{
        GasLimit, HandlePacket, InitPacket, MessageContext, Packet, ReplyPacket, StatusCode,
    },
    reservation::GasReserver,
};
use gear_core_errors::{CoreError, ExecutionError, ExtError, MemoryError, MessageError, WaitError};

/// Processor context.
pub struct ProcessorContext {
    /// Gas counter.
    pub gas_counter: GasCounter,
    /// Gas allowance counter.
    pub gas_allowance_counter: GasAllowanceCounter,
    /// Reserved gas counter.
    pub gas_reserver: GasReserver,
    /// Do system reservation?
    pub system_reservation: Option<u64>,
    /// Value counter.
    pub value_counter: ValueCounter,
    /// Allocations context.
    pub allocations_context: AllocationsContext,
    /// Message context.
    pub message_context: MessageContext,
    /// Block info.
    pub block_info: BlockInfo,
    /// Allocations config.
    pub config: AllocationsConfig,
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
    pub forbidden_funcs: BTreeSet<&'static str>,
    /// Mailbox threshold.
    pub mailbox_threshold: u64,
    /// Cost for single block waitlist holding.
    pub waitlist_cost: u64,
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
        stack_end: Option<WasmPageNumber>,
    );

    /// Lazy pages contract post execution actions
    fn lazy_pages_post_execution_actions(mem: &mut impl Memory);
}

/// [`Ext`](Ext)'s error
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Display, derive_more::From, Encode, Decode)]
pub enum ProcessorError {
    /// Basic error
    #[display(fmt = "{_0}")]
    Core(ExtError),
    /// Termination reason occurred in a syscall
    #[display(fmt = "Terminated: {_0:?}")]
    Terminated(TerminationReason),
    /// User's code panicked
    #[display(fmt = "Panic occurred: {_0}")]
    Panic(String),
}

impl ProcessorError {
    /// Tries to represent this error as [`ExtError`]
    pub fn as_ext_error(&self) -> Option<&ExtError> {
        match self {
            ProcessorError::Core(err) => Some(err),
            _ => None,
        }
    }

    /// Converts error into [`TrapExplanation`]
    pub fn into_trap_explanation(self) -> Option<TrapExplanation> {
        match self {
            Self::Core(err) => Some(TrapExplanation::Core(err)),
            Self::Panic(msg) => Some(TrapExplanation::Other(msg.into())),
            _ => None,
        }
    }
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

impl From<ExecutionError> for ProcessorError {
    fn from(err: ExecutionError) -> Self {
        Self::Core(ExtError::Execution(err))
    }
}

impl CoreError for ProcessorError {
    fn forbidden_function() -> Self {
        Self::Core(ExtError::forbidden_function())
    }
}

impl IntoExtError for ProcessorError {
    fn into_ext_error(self) -> Result<ExtError, Self> {
        match self {
            ProcessorError::Core(err) => Ok(err),
            err => Err(err),
        }
    }
}

impl AsTerminationReason for ProcessorError {
    fn as_termination_reason(&self) -> Option<&TerminationReason> {
        match self {
            ProcessorError::Terminated(reason) => Some(reason),
            _ => None,
        }
    }
}

/// Structure providing externalities for running host functions.
pub struct Ext {
    /// Processor context.
    pub context: ProcessorContext,
    /// Any guest code panic explanation, if available.
    pub error_explanation: Option<ProcessorError>,
}

/// Empty implementation for non-substrate (and non-lazy-pages) using
impl ProcessorExt for Ext {
    const LAZY_PAGES_ENABLED: bool = false;

    fn new(context: ProcessorContext) -> Self {
        Self {
            context,
            error_explanation: None,
        }
    }

    fn lazy_pages_init_for_program(
        _mem: &mut impl Memory,
        _prog_id: ProgramId,
        _stack_end: Option<WasmPageNumber>,
    ) {
        unreachable!()
    }

    fn lazy_pages_post_execution_actions(_mem: &mut impl Memory) {
        unreachable!()
    }
}

impl IntoExtInfo<<Ext as EnvExt>::Error> for Ext {
    fn into_ext_info(self, memory: &impl Memory) -> Result<ExtInfo, (MemoryError, GasAmount)> {
        let pages_for_data = |static_pages: WasmPageNumber,
                              allocations: &BTreeSet<WasmPageNumber>|
         -> Vec<PageNumber> {
            (0..static_pages.0)
                .map(WasmPageNumber)
                .chain(allocations.iter().copied())
                .flat_map(|p| p.to_gear_pages_iter())
                .collect()
        };

        self.into_ext_info_inner(memory, pages_for_data)
    }

    fn into_gas_amount(self) -> GasAmount {
        self.context.gas_counter.into()
    }

    fn last_error(&self) -> Result<&ExtError, <Ext as EnvExt>::Error> {
        self.error_explanation
            .as_ref()
            .and_then(ProcessorError::as_ext_error)
            .ok_or(ProcessorError::Core(ExtError::SyscallUsage))
    }

    fn trap_explanation(&self) -> Option<TrapExplanation> {
        self.error_explanation
            .clone()
            .and_then(ProcessorError::into_trap_explanation)
    }
}

impl GetGasAmount for Ext {
    fn gas_amount(&self) -> GasAmount {
        let gas_counter = self.context.gas_counter.clone();

        gas_counter.into()
    }
}

impl Ext {
    /// Return result and store error info in field
    pub fn return_and_store_err<T, E>(&mut self, result: Result<T, E>) -> Result<T, ProcessorError>
    where
        E: Into<ProcessorError>,
    {
        result.map_err(Into::into).map_err(|err| {
            self.error_explanation = Some(err.clone());
            err
        })
    }

    fn check_message_value(&mut self, message_value: u128) -> Result<(), ProcessorError> {
        let existential_deposit = self.context.existential_deposit;
        // Sending value should apply the range {0} ∪ [existential_deposit; +inf)
        if 0 < message_value && message_value < existential_deposit {
            self.return_and_store_err(Err(MessageError::InsufficientValue {
                message_value,
                existential_deposit,
            }))
        } else {
            Ok(())
        }
    }

    fn charge_message_gas(&mut self, gas_limit: Option<GasLimit>) -> Result<(), ProcessorError> {
        let mailbox_threshold = self.context.mailbox_threshold;
        let gas_limit = gas_limit.unwrap_or(0);

        if gas_limit != 0 && gas_limit < mailbox_threshold {
            self.return_and_store_err(Err(MessageError::InsufficientGasLimit {
                message_gas_limit: gas_limit,
                mailbox_threshold,
            }))
        } else if self.context.gas_counter.reduce(gas_limit) != ChargeResult::Enough {
            self.return_and_store_err(Err(MessageError::NotEnoughGas))
        } else {
            Ok(())
        }
    }

    fn charge_message_value(&mut self, message_value: u128) -> Result<(), ProcessorError> {
        if self.context.value_counter.reduce(message_value) != ChargeResult::Enough {
            self.return_and_store_err(Err(MessageError::NotEnoughValue {
                message_value,
                value_left: self.context.value_counter.left(),
            }))
        } else {
            Ok(())
        }
    }

    fn charge_expiring_resources<T: Packet>(&mut self, packet: &T) -> Result<(), ProcessorError> {
        self.check_message_value(packet.value())?;
        // Charge for using expiring resources. Charge for calling sys-call was done earlier.
        self.charge_message_gas(packet.gas_limit())?;
        self.charge_message_value(packet.value())?;
        Ok(())
    }

    fn check_forbidden_call(&mut self, id: ProgramId) -> Result<(), ProcessorError> {
        if id == ProgramId::SYSTEM {
            self.return_and_store_err(Err(ExecutionError::ForbiddenFunction))
        } else {
            Ok(())
        }
    }

    fn check_charge_results(
        &mut self,
        common_charge: ChargeResult,
        allowance_charge: ChargeResult,
    ) -> Result<(), ProcessorError> {
        use ChargeResult::*;

        let res: Result<(), ProcessorError> = match (common_charge, allowance_charge) {
            (NotEnough, _) => Err(ExecutionError::GasLimitExceeded.into()),
            (Enough, NotEnough) => Err(TerminationReason::GasAllowanceExceeded.into()),
            (Enough, Enough) => Ok(()),
        };

        self.return_and_store_err(res)
    }
}

impl EnvExt for Ext {
    type Error = ProcessorError;

    fn alloc(
        &mut self,
        pages_num: WasmPageNumber,
        mem: &mut impl Memory,
    ) -> Result<WasmPageNumber, Self::Error> {
        self.alloc_inner::<GrowHandlerNothing>(pages_num, mem)
    }

    fn block_height(&mut self) -> Result<u32, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::BlockHeight)?;
        Ok(self.context.block_info.height)
    }

    fn block_timestamp(&mut self) -> Result<u64, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::BlockTimestamp)?;
        Ok(self.context.block_info.timestamp)
    }

    fn origin(&mut self) -> Result<gear_core::ids::ProgramId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Origin)?;
        Ok(self.context.origin)
    }

    fn send_init(&mut self) -> Result<u32, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::SendInit)?;
        let result = self.context.message_context.send_init();

        self.return_and_store_err(result)
    }

    fn send_push(&mut self, handle: u32, buffer: &[u8]) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::SendPush(buffer.len() as u32))?;
        let result = self.context.message_context.send_push(handle, buffer);

        self.return_and_store_err(result)
    }

    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::ReplyPush(buffer.len() as u32))?;
        let result = self.context.message_context.reply_push(buffer);

        self.return_and_store_err(result)
    }

    fn send_commit(
        &mut self,
        handle: u32,
        msg: HandlePacket,
        delay: u32,
    ) -> Result<MessageId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::SendCommit(msg.payload().len() as u32))?;

        self.check_forbidden_call(msg.destination())?;
        self.charge_expiring_resources(&msg)?;

        if delay == 0 {
            self.charge_gas(self.context.message_context.settings().sending_fee())?;
        } else {
            self.charge_gas(
                self.context
                    .message_context
                    .settings()
                    .scheduled_sending_fee(),
            )?;
        }

        let result = self.context.message_context.send_commit(handle, msg, delay);

        self.return_and_store_err(result)
    }

    fn reply_commit(&mut self, msg: ReplyPacket, delay: u32) -> Result<MessageId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::ReplyCommit)?;

        self.check_forbidden_call(self.context.message_context.reply_destination())?;
        self.charge_expiring_resources(&msg)?;

        if delay == 0 {
            self.charge_gas(self.context.message_context.settings().sending_fee())?;
        } else {
            self.charge_gas(
                self.context
                    .message_context
                    .settings()
                    .scheduled_sending_fee(),
            )?;
        }

        let result = self.context.message_context.reply_commit(msg, delay);

        self.return_and_store_err(result)
    }

    fn reply_to(&mut self) -> Result<MessageId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::ReplyTo)?;

        self.context
            .message_context
            .current()
            .details()
            .and_then(|d| d.to_reply())
            .map(|d| d.into_reply_to())
            .ok_or_else(|| MessageError::NoReplyContext.into())
    }

    fn source(&mut self) -> Result<ProgramId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Source)?;
        Ok(self.context.message_context.current().source())
    }

    fn exit(&mut self) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Exit)?;
        Ok(())
    }

    fn status_code(&mut self) -> Result<StatusCode, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::StatusCode)?;

        self.context
            .message_context
            .current()
            .details()
            .map(|d| d.status_code())
            .ok_or_else(|| MessageError::NoStatusCodeContext.into())
    }

    fn message_id(&mut self) -> Result<MessageId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::MsgId)?;
        Ok(self.context.message_context.current().id())
    }

    fn program_id(&mut self) -> Result<gear_core::ids::ProgramId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::ProgramId)?;
        Ok(self.context.program_id)
    }

    fn free(&mut self, page: WasmPageNumber) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Free)?;

        let result = self.context.allocations_context.free(page);

        // Returns back gas for allocated page if it's new
        if !self.context.allocations_context.is_init_page(page) {
            self.refund_gas(self.context.config.alloc_cost)?;
        }

        self.return_and_store_err(result)
    }

    fn debug(&mut self, data: &str) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Debug(data.len() as u32))?;

        if let Some(data) = data.strip_prefix("panic occurred: ") {
            self.error_explanation = Some(ProcessorError::Panic(data.to_string()));
        }
        log::debug!(target: "gwasm", "DEBUG: {}", data);

        Ok(())
    }

    fn read(&mut self) -> Result<&[u8], Self::Error> {
        let size = self
            .size()?
            .try_into()
            .map_err(|_| MessageError::IncomingPayloadTooBig)?;

        self.charge_gas_runtime(RuntimeCosts::Read(size))?;

        Ok(self.context.message_context.current().payload())
    }

    fn size(&mut self) -> Result<usize, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Size)?;

        Ok(self.context.message_context.current().payload().len())
    }

    fn charge_gas(&mut self, val: u64) -> Result<(), Self::Error> {
        let common_charge = self.context.gas_counter.charge(val);
        let allowance_charge = self.context.gas_allowance_counter.charge(val);
        self.check_charge_results(common_charge, allowance_charge)
    }

    fn charge_gas_runtime(&mut self, costs: RuntimeCosts) -> Result<(), Self::Error> {
        let (common_charge, allowance_charge) = charge_gas_token!(self, costs);
        self.check_charge_results(common_charge, allowance_charge)
    }

    fn refund_gas(&mut self, val: u64) -> Result<(), Self::Error> {
        if self.context.gas_counter.refund(val) == ChargeResult::Enough {
            self.context.gas_allowance_counter.refund(val);
            Ok(())
        } else {
            self.return_and_store_err(Err(ExecutionError::TooManyGasAdded))
        }
    }

    fn reserve_gas(&mut self, amount: u64, duration: u32) -> Result<ReservationId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::ReserveGas)?;

        let common_charge = self.context.gas_counter.reduce(amount);
        if common_charge == ChargeResult::NotEnough {
            return Err(ExecutionError::InsufficientGasForReservation.into());
        }

        let id = self.context.gas_reserver.reserve(amount, duration)?;

        Ok(id)
    }

    fn unreserve_gas(&mut self, id: ReservationId) -> Result<u64, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::UnreserveGas)?;

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
        self.charge_gas_runtime(RuntimeCosts::SystemReserveGas)?;

        if self.context.gas_counter.reduce(amount) == ChargeResult::NotEnough {
            return Err(ExecutionError::InsufficientGasForReservation.into());
        }

        self.context.system_reservation = Some(amount);

        Ok(())
    }

    fn gas_available(&mut self) -> Result<u64, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::GasAvailable)?;
        Ok(self.context.gas_counter.left())
    }

    fn value(&mut self) -> Result<u128, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Value)?;
        Ok(self.context.message_context.current().value())
    }

    fn value_available(&mut self) -> Result<u128, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::ValueAvailable)?;
        Ok(self.context.value_counter.left())
    }

    fn leave(&mut self) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Leave)?;
        Ok(())
    }

    fn wait(&mut self) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Wait)?;
        self.charge_gas(self.context.message_context.settings().waiting_fee())?;

        let reserve = u64::from(self.context.reserve_for.saturating_add(1))
            .saturating_mul(self.context.waitlist_cost);

        if self.context.gas_counter.reduce(reserve) != ChargeResult::Enough {
            return self.return_and_store_err(Err(WaitError::NotEnoughGas));
        }

        Ok(())
    }

    fn wait_for(&mut self, duration: u32) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::WaitFor)?;
        self.charge_gas(self.context.message_context.settings().waiting_fee())?;

        if duration == 0 {
            return self.return_and_store_err(Err(WaitError::InvalidArgument));
        }

        let reserve = u64::from(self.context.reserve_for.saturating_add(duration))
            .saturating_mul(self.context.waitlist_cost);

        if self.context.gas_counter.reduce(reserve) != ChargeResult::Enough {
            return self.return_and_store_err(Err(WaitError::NotEnoughGas));
        }

        Ok(())
    }

    fn wait_up_to(&mut self, duration: u32) -> Result<bool, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::WaitUpTo)?;
        self.charge_gas(self.context.message_context.settings().waiting_fee())?;

        if duration == 0 {
            return self.return_and_store_err(Err(WaitError::InvalidArgument));
        }

        let reserve = u64::from(self.context.reserve_for.saturating_add(1))
            .saturating_mul(self.context.waitlist_cost);

        if self.context.gas_counter.reduce(reserve) != ChargeResult::Enough {
            return self.return_and_store_err(Err(WaitError::NotEnoughGas));
        }

        let reserve_full = u64::from(self.context.reserve_for.saturating_add(duration))
            .saturating_mul(self.context.waitlist_cost);
        let reserve_diff = reserve_full - reserve;

        Ok(self.context.gas_counter.reduce(reserve_diff) == ChargeResult::Enough)
    }

    fn wake(&mut self, waker_id: MessageId, delay: u32) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Wake)?;
        self.charge_gas(self.context.message_context.settings().waking_fee())?;

        let result = self.context.message_context.wake(waker_id, delay);

        self.return_and_store_err(result)
    }

    fn create_program(
        &mut self,
        packet: InitPacket,
        delay: u32,
    ) -> Result<(MessageId, ProgramId), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::CreateProgram(
            packet.payload().len() as u32,
            packet.salt().len() as u32,
        ))?;

        self.charge_expiring_resources(&packet)?;

        let code_hash = packet.code_id();

        // Send a message for program creation
        let result = self
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
            });

        self.return_and_store_err(result)
    }

    fn random(&self) -> (&[u8], u32) {
        (&self.context.random_data.0, self.context.random_data.1)
    }

    fn forbidden_funcs(&self) -> &BTreeSet<&'static str> {
        &self.context.forbidden_funcs
    }

    fn counters(&self) -> (u64, u64) {
        (
            self.context.gas_counter.left(),
            self.context.gas_allowance_counter.left(),
        )
    }

    fn update_counters(&mut self, gas: u64, allowance: u64) {
        let gas_left = self.context.gas_counter.left();
        if gas_left > gas {
            self.context.gas_counter.charge(gas_left - gas);
        } else {
            self.context.gas_counter.refund(gas - gas_left);
        }

        let allowance_left = self.context.gas_allowance_counter.left();
        if allowance_left > allowance {
            self.context
                .gas_allowance_counter
                .charge(allowance_left - allowance);
        } else {
            self.context
                .gas_allowance_counter
                .refund(allowance - allowance_left);
        }
    }

    fn out_of_gas(&mut self) -> Self::Error {
        self.error_explanation = Some(ExecutionError::GasLimitExceeded.into());
        ExecutionError::GasLimitExceeded.into()
    }

    fn out_of_allowance(&mut self) -> Self::Error {
        self.error_explanation = Some(TerminationReason::GasAllowanceExceeded.into());
        TerminationReason::GasAllowanceExceeded.into()
    }
}

impl Ext {
    /// Inner alloc realization.
    pub fn alloc_inner<G: GrowHandler>(
        &mut self,
        pages_num: WasmPageNumber,
        mem: &mut impl Memory,
    ) -> Result<WasmPageNumber, ProcessorError> {
        self.charge_gas_runtime(RuntimeCosts::Alloc)?;

        // Charge gas for allocations
        self.charge_gas((pages_num.0 as u64).saturating_mul(self.context.config.alloc_cost))?;
        // Greedily charge gas for grow
        self.charge_gas((pages_num.0 as u64).saturating_mul(self.context.config.mem_grow_cost))?;

        let old_mem_size = mem.size();

        let result = self.context.allocations_context.alloc::<G>(pages_num, mem);

        let page_number = self.return_and_store_err(result)?;

        // Returns back greedily used gas for grow
        let new_mem_size = mem.size();
        let grow_pages_num = new_mem_size - old_mem_size;
        let mut gas_to_return_back = self
            .context
            .config
            .mem_grow_cost
            .saturating_mul((pages_num - grow_pages_num).0 as u64);

        // Returns back greedily used gas for allocations
        let mut new_allocated_pages_num = 0;
        for page in page_number.0..page_number.0 + pages_num.0 {
            if !self.context.allocations_context.is_init_page(page.into()) {
                new_allocated_pages_num += 1;
            }
        }
        gas_to_return_back = gas_to_return_back.saturating_add(
            self.context
                .config
                .alloc_cost
                .saturating_mul((pages_num.0 - new_allocated_pages_num) as u64),
        );

        self.refund_gas(gas_to_return_back)?;

        Ok(page_number)
    }

    /// Into ext info inner impl.
    /// `pages_for_data` returns vector of pages which data will be stored in info.
    pub fn into_ext_info_inner(
        self,
        memory: &impl Memory,
        pages_for_data: impl FnOnce(WasmPageNumber, &BTreeSet<WasmPageNumber>) -> Vec<PageNumber>,
    ) -> Result<ExtInfo, (MemoryError, GasAmount)> {
        let ProcessorContext {
            allocations_context,
            message_context,
            gas_counter,
            gas_reserver,
            program_candidates_data,
            system_reservation,
            ..
        } = self.context;

        let (static_pages, initial_allocations, allocations) = allocations_context.into_parts();
        let mut pages_data = BTreeMap::new();
        for page in pages_for_data(static_pages, &allocations) {
            let mut buf = PageBuf::new_zeroed();
            if let Err(err) = memory.read(page.offset(), buf.as_mut_slice()) {
                return Err((err, gas_counter.into()));
            }
            pages_data.insert(page, buf);
        }

        let (outcome, mut context_store) = message_context.drain();
        let (generated_dispatches, awakening) = outcome.drain();

        context_store.set_reservation_nonce(gas_reserver.nonce());

        let info = ExtInfo {
            gas_amount: gas_counter.into(),
            gas_reserver,
            system_reservation,
            allocations: allocations.ne(&initial_allocations).then_some(allocations),
            pages_data,
            generated_dispatches,
            awakening,
            context_store,
            program_candidates_data,
        };
        Ok(info)
    }
}
