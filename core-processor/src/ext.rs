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
use core::fmt;
use gear_backend_common::{
    error_processor::IntoExtError, AsTerminationReason, ExtInfo, IntoExtInfo, TerminationReason,
    TrapExplanation,
};
use gear_core::{
    charge_gas_token,
    costs::{HostFnWeights, RuntimeCosts},
    env::Ext as EnvExt,
    gas::{ChargeResult, GasAllowanceCounter, GasAmount, GasCounter, ValueCounter},
    ids::{CodeId, MessageId, ProgramId},
    memory::{AllocationsContext, Memory, PageBuf, PageNumber, WasmPageNumber},
    message::{
        GasLimit, HandlePacket, InitPacket, MessageContext, Packet, ReplyDetails, ReplyPacket,
    },
};
use gear_core_errors::{CoreError, ExecutionError, ExtError, MemoryError, MessageError};

/// Processor context.
pub struct ProcessorContext {
    /// Gas counter.
    pub gas_counter: GasCounter,
    /// Gas allowance counter.
    pub gas_allowance_counter: GasAllowanceCounter,
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
    pub program_candidates_data: BTreeMap<CodeId, Vec<(ProgramId, MessageId)>>,
    /// Weights of host functions.
    pub host_fn_weights: HostFnWeights,
    /// Functions forbidden to be called.
    pub forbidden_funcs: BTreeSet<&'static str>,
    /// Mailbox threshold
    pub mailbox_threshold: u64,
}

/// Trait to which ext must have to work in processor wasm executor.
/// Currently used only for lazy-pages support.
pub trait ProcessorExt {
    /// An error issues in processor
    type Error: fmt::Display;

    /// Create new
    fn new(context: ProcessorContext) -> Self;

    /// Returns whether this extension works with lazy pages
    fn is_lazy_pages_enabled() -> bool;

    /// If extension support lazy pages, then checks that
    /// environment for lazy pages is initialized.
    fn check_lazy_pages_consistent_state() -> bool;

    /// Protect and save storage keys for pages which has no data
    fn lazy_pages_protect_and_init_info(
        mem: &impl Memory,
        prog_id: ProgramId,
    ) -> Result<(), Self::Error>;

    /// Lazy pages contract post execution actions
    fn lazy_pages_post_execution_actions(
        mem: &impl Memory,
        memory_pages: &mut BTreeMap<PageNumber, PageBuf>,
    ) -> Result<(), Self::Error>;
}

/// [`Ext`](Ext)'s error
#[derive(Debug, Clone, Eq, PartialEq, derive_more::Display, derive_more::From)]
pub enum ProcessorError {
    /// Basic error
    #[display(fmt = "{}", _0)]
    Core(ExtError),
    /// Termination reason occurred in a syscall
    #[display(fmt = "Terminated: {:?}", _0)]
    Terminated(TerminationReason),
    /// User's code panicked
    #[display(fmt = "Panic occurred: {}", _0)]
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

impl From<ExecutionError> for ProcessorError {
    fn from(err: ExecutionError) -> Self {
        Self::Core(ExtError::Execution(err))
    }
}

impl CoreError for ProcessorError {}

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
    type Error = ExtError;

    fn new(context: ProcessorContext) -> Self {
        Self {
            context,
            error_explanation: None,
        }
    }

    fn is_lazy_pages_enabled() -> bool {
        false
    }

    fn check_lazy_pages_consistent_state() -> bool {
        true
    }

    fn lazy_pages_protect_and_init_info(
        _mem: &impl Memory,
        _prog_id: ProgramId,
    ) -> Result<(), Self::Error> {
        unreachable!()
    }

    fn lazy_pages_post_execution_actions(
        _mem: &impl Memory,
        _memory_pages: &mut BTreeMap<PageNumber, PageBuf>,
    ) -> Result<(), Self::Error> {
        unreachable!()
    }
}

impl IntoExtInfo for Ext {
    fn into_ext_info(
        self,
        memory: &impl Memory,
    ) -> Result<(ExtInfo, Option<TrapExplanation>), (MemoryError, GasAmount)> {
        let ProcessorContext {
            allocations_context,
            message_context,
            gas_counter,
            program_candidates_data,
            ..
        } = self.context;

        let static_pages = allocations_context.static_pages();
        let (initial_allocations, wasm_pages) = allocations_context.into_parts();
        let mut pages_data = BTreeMap::new();
        for page in (0..static_pages.0)
            .map(WasmPageNumber)
            .chain(wasm_pages.iter().copied())
            .flat_map(|p| p.to_gear_pages_iter())
        {
            let mut buf = PageBuf::new_zeroed();
            if let Err(err) = memory.read(page.offset(), buf.as_mut_slice()) {
                return Err((err, gas_counter.into()));
            }
            pages_data.insert(page, buf);
        }

        let (outcome, context_store) = message_context.drain();
        let (generated_dispatches, awakening) = outcome.drain();

        let info = ExtInfo {
            gas_amount: gas_counter.into(),
            allocations: wasm_pages.ne(&initial_allocations).then_some(wasm_pages),
            pages_data,
            generated_dispatches,
            awakening,
            context_store,
            program_candidates_data,
        };
        let trap_explanation = self
            .error_explanation
            .and_then(ProcessorError::into_trap_explanation);
        Ok((info, trap_explanation))
    }

    fn into_gas_amount(self) -> GasAmount {
        self.context.gas_counter.into()
    }

    fn last_error(&self) -> Option<&ExtError> {
        self.error_explanation
            .as_ref()
            .and_then(ProcessorError::as_ext_error)
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
}

impl EnvExt for Ext {
    type Error = ProcessorError;

    fn alloc(
        &mut self,
        pages_num: WasmPageNumber,
        mem: &mut impl Memory,
    ) -> Result<WasmPageNumber, Self::Error> {
        // Greedily charge gas for allocations
        self.charge_gas(
            pages_num
                .0
                .saturating_mul(self.context.config.alloc_cost as u32),
        )?;
        // Greedily charge gas for grow
        self.charge_gas(
            pages_num
                .0
                .saturating_mul(self.context.config.mem_grow_cost as u32),
        )?;

        self.charge_gas_runtime(RuntimeCosts::Alloc)?;

        let old_mem_size = mem.size();

        let result = self.context.allocations_context.alloc(pages_num, mem);

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
        let first_page = page_number;
        let last_page = first_page + pages_num - 1.into();
        let mut new_allocated_pages_num = 0;
        for page in first_page.0..=last_page.0 {
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

        self.refund_gas(gas_to_return_back as u32)?;

        Ok(page_number)
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

    fn send_init(&mut self) -> Result<usize, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::SendInit)?;
        let result = self.context.message_context.send_init();

        self.return_and_store_err(result.map(|v| v as usize))
    }

    fn send_push(&mut self, handle: usize, buffer: &[u8]) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::SendPush(buffer.len() as u32))?;
        let result = self
            .context
            .message_context
            .send_push(handle as u32, buffer);

        self.return_and_store_err(result)
    }

    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::ReplyPush(buffer.len() as u32))?;
        let result = self.context.message_context.reply_push(buffer);

        self.return_and_store_err(result)
    }

    fn send_commit(&mut self, handle: usize, msg: HandlePacket) -> Result<MessageId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::SendCommit(msg.payload().len() as u32))?;

        self.charge_expiring_resources(&msg)?;

        let result = self.context.message_context.send_commit(handle as u32, msg);

        self.return_and_store_err(result)
    }

    fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::ReplyCommit(msg.payload().len() as u32))?;

        self.charge_expiring_resources(&msg)?;

        let result = self.context.message_context.reply_commit(msg);

        self.return_and_store_err(result)
    }

    fn reply_details(&mut self) -> Result<Option<ReplyDetails>, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::ReplyTo)?;
        Ok(self.context.message_context.current().reply())
    }

    fn source(&mut self) -> Result<ProgramId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Source)?;
        Ok(self.context.message_context.current().source())
    }

    fn exit(&mut self) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Exit)?;
        Ok(())
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
        let result = self.context.allocations_context.free(page);

        // Returns back gas for allocated page if it's new
        if !self.context.allocations_context.is_init_page(page) {
            self.refund_gas(self.context.config.alloc_cost as u32)?;
        }

        self.return_and_store_err(result)
    }

    fn debug(&mut self, data: &str) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Debug)?;

        if let Some(data) = data.strip_prefix("panic occurred: ") {
            self.error_explanation = Some(ProcessorError::Panic(data.to_string()));
        }
        log::debug!(target: "gwasm", "DEBUG: {}", data);

        Ok(())
    }

    fn msg(&mut self) -> &[u8] {
        self.context.message_context.current().payload()
    }

    fn gas(&mut self, val: u32) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::MeteringBlock(val))
    }

    fn charge_gas(&mut self, val: u32) -> Result<(), Self::Error> {
        use ChargeResult::*;

        let common_charge = self.context.gas_counter.charge(val as u64);
        let allowance_charge = self.context.gas_allowance_counter.charge(val as u64);

        let res: Result<(), ProcessorError> = match (common_charge, allowance_charge) {
            (NotEnough, _) => Err(ExecutionError::GasLimitExceeded.into()),
            (Enough, NotEnough) => Err(TerminationReason::GasAllowanceExceeded.into()),
            (Enough, Enough) => Ok(()),
        };

        self.return_and_store_err(res)
    }

    fn charge_gas_runtime(&mut self, costs: RuntimeCosts) -> Result<(), Self::Error> {
        use ChargeResult::*;
        let (common_charge, allowance_charge) = charge_gas_token!(self, costs);

        let res: Result<(), ProcessorError> = match (common_charge, allowance_charge) {
            (NotEnough, _) => Err(ExecutionError::GasLimitExceeded.into()),
            (Enough, NotEnough) => Err(TerminationReason::GasAllowanceExceeded.into()),
            (Enough, Enough) => Ok(()),
        };

        self.return_and_store_err(res)
    }

    fn refund_gas(&mut self, val: u32) -> Result<(), Self::Error> {
        if self.context.gas_counter.refund(val as u64) == ChargeResult::Enough {
            self.context.gas_allowance_counter.refund(val as u64);
            Ok(())
        } else {
            self.return_and_store_err(Err(ExecutionError::TooManyGasAdded))
        }
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
        Ok(())
    }

    fn wake(&mut self, waker_id: MessageId) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Wake)?;
        let result = self.context.message_context.wake(waker_id);

        self.return_and_store_err(result)
    }

    fn create_program(&mut self, packet: InitPacket) -> Result<ProgramId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::CreateProgram(packet.payload().len() as u32))?;

        self.charge_expiring_resources(&packet)?;

        let code_hash = packet.code_id();

        // Send a message for program creation
        let result =
            self.context
                .message_context
                .init_program(packet)
                .map(|(new_prog_id, init_msg_id)| {
                    // Save a program candidate for this run
                    let entry = self
                        .context
                        .program_candidates_data
                        .entry(code_hash)
                        .or_default();
                    entry.push((new_prog_id, init_msg_id));

                    new_prog_id
                });

        self.return_and_store_err(result)
    }

    fn forbidden_funcs(&self) -> &BTreeSet<&'static str> {
        &self.context.forbidden_funcs
    }
}
