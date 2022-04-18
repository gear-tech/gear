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
use alloc::{boxed::Box, collections::BTreeMap, vec::Vec};
use gear_backend_common::{ExtInfo, IntoExtInfo};
use gear_core::{
    env::Ext as EnvExt,
    gas::{ChargeResult, GasAllowanceCounter, GasAmount, GasCounter, ValueCounter},
    ids::{CodeId, MessageId, ProgramId},
    memory::{
        wasm_pages_to_pages_set, AllocationsContext, Memory, PageBuf, PageNumber, WasmPageNumber,
    },
    message::{ExitCode, HandlePacket, InitPacket, MessageContext, ReplyPacket},
};

/// Trait to which ext must have to work in processor wasm executor.
/// Currently used only for lazy-pages support.
pub trait ProcessorExt {
    /// Create new
    #[allow(clippy::too_many_arguments)]
    fn new(
        gas_counter: GasCounter,
        gas_allowance_counter: GasAllowanceCounter,
        value_counter: ValueCounter,
        allocations_context: AllocationsContext,
        message_context: MessageContext,
        block_info: BlockInfo,
        config: AllocationsConfig,
        existential_deposit: u128,
        error_explanation: Option<&'static str>,
        exit_argument: Option<ProgramId>,
        origin: ProgramId,
        program_id: ProgramId,
        program_candidates_data: BTreeMap<CodeId, Vec<(ProgramId, MessageId)>>,
    ) -> Self;

    /// Try to enable and initialize lazy pages env
    fn try_to_enable_lazy_pages(
        &mut self,
        program_id: ProgramId,
        memory_pages: &mut BTreeMap<PageNumber, Option<Box<PageBuf>>>,
    ) -> Result<bool, &'static str>;

    /// Protect and save storage keys for pages which has no data
    fn protect_pages_and_init_info(
        memory_pages: &BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        prog_id: ProgramId,
        wasm_mem_begin_addr: u64,
    ) -> Result<(), &'static str>;

    /// Lazy pages contract post execution actions
    fn post_execution_actions(
        memory_pages: &mut BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        wasm_mem_begin_addr: u64,
    ) -> Result<(), &'static str>;

    /// Remove lazy-pages protection, returns wasm memory begin addr
    fn remove_lazy_pages_prot(mem_addr: u64) -> Result<(), &'static str>;

    /// Protect lazy-pages and set new wasm mem addr if it has been changed
    fn protect_lazy_pages_and_update_wasm_mem_addr(
        old_mem_addr: u64,
        new_mem_addr: u64,
    ) -> Result<(), &'static str>;
}

/// Structure providing externalities for running host functions.
pub struct Ext {
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
    /// Any guest code panic explanation, if available.
    pub error_explanation: Option<&'static str>,
    /// Contains argument to the `exit` if it was called.
    pub exit_argument: Option<ProgramId>,
    /// Communication origin
    pub origin: ProgramId,
    /// Current program id
    pub program_id: ProgramId,
    /// Map of code hashes to program ids of future programs, which are planned to be
    /// initialized with the corresponding code (with the same code hash).
    pub program_candidates_data: BTreeMap<CodeId, Vec<(ProgramId, MessageId)>>,
}

/// Empty implementation for non-substrate (and non-lazy-pages) using
impl ProcessorExt for Ext {
    fn new(
        gas_counter: GasCounter,
        gas_allowance_counter: GasAllowanceCounter,
        value_counter: ValueCounter,
        allocations_context: AllocationsContext,
        message_context: MessageContext,
        block_info: BlockInfo,
        config: AllocationsConfig,
        existential_deposit: u128,
        error_explanation: Option<&'static str>,
        exit_argument: Option<ProgramId>,
        origin: ProgramId,
        program_id: ProgramId,
        program_candidates_data: BTreeMap<CodeId, Vec<(ProgramId, MessageId)>>,
    ) -> Self {
        Self {
            gas_counter,
            gas_allowance_counter,
            value_counter,
            allocations_context,
            message_context,
            block_info,
            config,
            existential_deposit,
            error_explanation,
            exit_argument,
            origin,
            program_id,
            program_candidates_data,
        }
    }

    fn try_to_enable_lazy_pages(
        &mut self,
        _program_id: ProgramId,
        _memory_pages: &mut BTreeMap<PageNumber, Option<Box<PageBuf>>>,
    ) -> Result<bool, &'static str> {
        Ok(false)
    }

    fn protect_pages_and_init_info(
        _memory_pages: &BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        _prog_id: ProgramId,
        _wasm_mem_begin_addr: u64,
    ) -> Result<(), &'static str> {
        Ok(())
    }

    fn post_execution_actions(
        _memory_pages: &mut BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        _wasm_mem_begin_addr: u64,
    ) -> Result<(), &'static str> {
        Ok(())
    }

    fn remove_lazy_pages_prot(_mem_addr: u64) -> Result<(), &'static str> {
        Ok(())
    }

    fn protect_lazy_pages_and_update_wasm_mem_addr(
        _old_mem_addr: u64,
        _new_mem_addr: u64,
    ) -> Result<(), &'static str> {
        Ok(())
    }
}

impl IntoExtInfo for Ext {
    fn into_ext_info<F: FnMut(usize, &mut [u8]) -> Result<(), &'static str>>(
        self,
        mut get_page_data: F,
    ) -> Result<ExtInfo, (&'static str, GasAmount)> {
        let pages = wasm_pages_to_pages_set(self.allocations_context.allocations().iter());
        let mut pages_data = BTreeMap::new();
        for page in pages.iter() {
            let mut buf = alloc::vec![0u8; PageNumber::size()];
            if let Err(err) = get_page_data(page.offset(), &mut buf) {
                return Err((err, self.gas_counter.into()));
            }
            pages_data.insert(*page, buf);
        }

        let (outcome, context_store) = self.message_context.drain();
        let (generated_dispatches, awakening) = outcome.drain();

        Ok(ExtInfo {
            gas_amount: self.gas_counter.into(),
            pages,
            pages_data,
            generated_dispatches,
            awakening,
            context_store,
            trap_explanation: self.error_explanation,
            exit_argument: self.exit_argument,
            program_candidates_data: self.program_candidates_data,
        })
    }

    fn into_gas_amount(self) -> GasAmount {
        self.gas_counter.into()
    }
}

impl Ext {
    /// Return result and store error info in field
    pub fn return_and_store_err<T>(
        &mut self,
        result: Result<T, &'static str>,
    ) -> Result<T, &'static str> {
        result.map_err(|err| {
            self.error_explanation = Some(err);
            err
        })
    }
}

impl EnvExt for Ext {
    fn alloc(
        &mut self,
        pages_num: WasmPageNumber,
        mem: &mut dyn Memory,
    ) -> Result<WasmPageNumber, &'static str> {
        // Greedily charge gas for allocations
        self.charge_gas(pages_num.0 * self.config.alloc_cost as u32)?;
        // Greedily charge gas for grow
        self.charge_gas(pages_num.0 * self.config.mem_grow_cost as u32)?;

        let old_mem_size = mem.size();

        let result = self
            .allocations_context
            .alloc(pages_num, mem)
            .map_err(|_e| "Allocation error");

        let page_number = self.return_and_store_err(result)?;

        // Returns back greedily used gas for grow
        let new_mem_size = mem.size();
        let grow_pages_num = new_mem_size - old_mem_size;
        let mut gas_to_return_back =
            self.config.mem_grow_cost * (pages_num - grow_pages_num).0 as u64;

        // Returns back greedily used gas for allocations
        let first_page = page_number;
        let last_page = first_page + pages_num - 1.into();
        let mut new_alloced_pages_num = 0;
        for page in first_page.0..=last_page.0 {
            if !self.allocations_context.is_init_page(page.into()) {
                new_alloced_pages_num += 1;
            }
        }
        gas_to_return_back += self.config.alloc_cost * (pages_num.0 - new_alloced_pages_num) as u64;

        self.refund_gas(gas_to_return_back as u32)?;

        Ok(page_number)
    }

    fn block_height(&self) -> u32 {
        self.block_info.height
    }

    fn block_timestamp(&self) -> u64 {
        self.block_info.timestamp
    }

    fn origin(&self) -> ProgramId {
        self.origin
    }

    fn send_init(&mut self) -> Result<usize, &'static str> {
        let result = self
            .message_context
            .send_init()
            .map_err(|_e| "Message init error");

        self.return_and_store_err(result.map(|v| v as usize))
    }

    fn send_push(&mut self, handle: usize, buffer: &[u8]) -> Result<(), &'static str> {
        let result = self
            .message_context
            .send_push(handle as u32, buffer)
            .map_err(|_e| "Payload push error");

        self.return_and_store_err(result)
    }

    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), &'static str> {
        let result = self
            .message_context
            .reply_push(buffer)
            .map_err(|_e| "Reply payload push error");

        self.return_and_store_err(result)
    }

    fn send_commit(&mut self, handle: usize, msg: HandlePacket) -> Result<MessageId, &'static str> {
        if 0 < msg.value() && msg.value() < self.existential_deposit {
            return self.return_and_store_err(Err(
                "Value of the message is less than existance deposit, but greater than 0",
            ));
        };

        if self.gas_counter.reduce(msg.gas_limit().unwrap_or(0)) != ChargeResult::Enough {
            return self
                .return_and_store_err(Err("Gas limit exceeded while trying to send message"));
        };

        if self.value_counter.reduce(msg.value()) != ChargeResult::Enough {
            return self.return_and_store_err(Err("No value left to reply"));
        };

        let result = self
            .message_context
            .send_commit(handle as u32, msg)
            .map_err(|_e| "Message commit error");

        self.return_and_store_err(result)
    }

    fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, &'static str> {
        if 0 < msg.value() && msg.value() < self.existential_deposit {
            return self.return_and_store_err(Err(
                "Value of the message is less than existance deposit, but greater than 0",
            ));
        };

        if self.value_counter.reduce(msg.value()) != ChargeResult::Enough {
            return self.return_and_store_err(Err("No value left to reply"));
        };

        let result = self
            .message_context
            .reply_commit(msg)
            .map_err(|_e| "Reply commit error");

        self.return_and_store_err(result)
    }

    fn reply_to(&self) -> Option<(MessageId, ExitCode)> {
        self.message_context.current().reply()
    }

    fn source(&mut self) -> ProgramId {
        self.message_context.current().source()
    }

    fn exit(&mut self, value_destination: ProgramId) -> Result<(), &'static str> {
        if self.exit_argument.is_some() {
            Err("Cannot call `exit' twice")
        } else {
            self.exit_argument = Some(value_destination);
            Ok(())
        }
    }

    fn message_id(&mut self) -> MessageId {
        self.message_context.current().id()
    }

    fn program_id(&self) -> ProgramId {
        self.program_id
    }

    fn free(&mut self, page: WasmPageNumber) -> Result<(), &'static str> {
        let result = self
            .allocations_context
            .free(page)
            .map_err(|_e| "Free error");

        // Returns back gas for allocated page if it's new
        if !self.allocations_context.is_init_page(page) {
            self.refund_gas(self.config.alloc_cost as u32)?;
        }

        self.return_and_store_err(result)
    }

    fn debug(&mut self, data: &str) -> Result<(), &'static str> {
        log::debug!(target: "gwasm", "DEBUG: {}", data);

        Ok(())
    }

    fn msg(&mut self) -> &[u8] {
        self.message_context.current().payload()
    }

    fn charge_gas(&mut self, val: u32) -> Result<(), &'static str> {
        use ChargeResult::*;

        let common_charge = self.gas_counter.charge(val as u64);
        let allowance_charge = self.gas_allowance_counter.charge(val as u64);

        let res = match (common_charge, allowance_charge) {
            (NotEnough, _) => Err("Gas limit exceeded"),
            (Enough, NotEnough) => Err(gear_backend_common::GAS_ALLOWANCE_STR),
            (Enough, Enough) => Ok(()),
        };

        self.return_and_store_err(res)
    }

    fn refund_gas(&mut self, val: u32) -> Result<(), &'static str> {
        if self.gas_counter.refund(val as u64) == ChargeResult::Enough {
            self.gas_allowance_counter.refund(val as u64);
            Ok(())
        } else {
            self.return_and_store_err(Err("Too many gas added"))
        }
    }

    fn gas_available(&self) -> u64 {
        self.gas_counter.left()
    }

    fn value(&self) -> u128 {
        self.message_context.current().value()
    }

    fn value_available(&self) -> u128 {
        self.value_counter.left()
    }

    fn leave(&mut self) -> Result<(), &'static str> {
        Ok(())
    }

    fn wait(&mut self) -> Result<(), &'static str> {
        Ok(())
    }

    fn wake(&mut self, waker_id: MessageId) -> Result<(), &'static str> {
        let result = self
            .message_context
            .wake(waker_id)
            .map_err(|_| "Unable to mark the message to be woken");

        self.return_and_store_err(result)
    }

    fn create_program(&mut self, packet: InitPacket) -> Result<ProgramId, &'static str> {
        let code_hash = packet.code_id();

        // Send a message for program creation
        let result = self
            .message_context
            .init_program(packet)
            .map(|(new_prog_id, init_msg_id)| {
                // Save a program candidate for this run
                let entry = self.program_candidates_data.entry(code_hash).or_default();
                entry.push((new_prog_id, init_msg_id));

                new_prog_id
            })
            .map_err(|_| "Duplicate init message for the same id");

        self.return_and_store_err(result)
    }
}
