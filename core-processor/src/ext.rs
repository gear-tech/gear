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

use crate::{
    configs::{AllocationsConfig, BlockInfo},
    id::BlakeMessageIdGenerator,
};
use alloc::vec;
use alloc::vec::Vec;
use alloc::{boxed::Box, collections::BTreeMap};
use gear_backend_common::ExtInfo;
use gear_core::{
    env::Ext as EnvExt,
    gas::{ChargeResult, GasAmount, GasCounter, ValueCounter},
    memory::{MemoryContext, PageBuf, PageNumber},
    message::{ExitCode, MessageContext, MessageId, MessageState, OutgoingPacket, ReplyPacket},
    program::ProgramId,
};

/// Trait to which ext must have to work in processor wasm executor.
/// Currently used only for lazy-pages support.
pub trait ProcessorExt {
    /// Create new
    #[allow(clippy::too_many_arguments)]
    fn new(
        gas_counter: GasCounter,
        value_counter: ValueCounter,
        memory_context: MemoryContext,
        message_context: MessageContext<BlakeMessageIdGenerator>,
        block_info: BlockInfo,
        config: AllocationsConfig,
        existential_deposit: u128,
        error_explanation: Option<&'static str>,
        exit_argument: Option<ProgramId>,
        initiator: ProgramId,
    ) -> Self;

    /// Try to enable and initialize lazy pages env
    fn try_to_enable_lazy_pages(
        &mut self,
        program_id: ProgramId,
        memory_pages: &mut BTreeMap<PageNumber, Option<Box<PageBuf>>>,
    ) -> bool;

    /// Protect and save storage keys for pages which has no data
    fn protect_pages_and_init_info(
        memory_pages: &BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        prog_id: ProgramId,
        wasm_mem_begin_addr: usize,
    );

    /// Lazy pages contract post execution actions
    fn post_execution_actions(
        memory_pages: &mut BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        wasm_mem_begin_addr: usize,
    );

    /// Remove lazy-pages protection, returns wasm memory begin addr
    fn remove_lazy_pages_prot(mem_addr: usize);

    /// Protect lazy-pages and set new wasm mem addr if it has been changed
    fn protect_lazy_pages_and_update_wasm_mem_addr(old_mem_addr: usize, new_mem_addr: usize);

    /// Returns list of current lazy pages numbers
    fn get_lazy_pages_numbers() -> Vec<u32>;
}

/// Structure providing externalities for running host functions.
pub struct Ext {
    /// Gas counter.
    pub gas_counter: GasCounter,
    /// Value counter.
    pub value_counter: ValueCounter,
    /// Memory context.
    pub memory_context: MemoryContext,
    /// Message context.
    pub message_context: MessageContext<BlakeMessageIdGenerator>,
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
    /// Communication initiator
    pub initiator: ProgramId,
}

/// Empty implementation for non-substrate (and non-lazy-pages) using
impl ProcessorExt for Ext {
    fn new(
        gas_counter: GasCounter,
        value_counter: ValueCounter,
        memory_context: MemoryContext,
        message_context: MessageContext<BlakeMessageIdGenerator>,
        block_info: BlockInfo,
        config: AllocationsConfig,
        existential_deposit: u128,
        error_explanation: Option<&'static str>,
        exit_argument: Option<ProgramId>,
        initiator: ProgramId,
    ) -> Self {
        Self {
            gas_counter,
            value_counter,
            memory_context,
            message_context,
            block_info,
            config,
            existential_deposit,
            error_explanation,
            exit_argument,
            initiator,
        }
    }

    fn try_to_enable_lazy_pages(
        &mut self,
        _program_id: ProgramId,
        _memory_pages: &mut BTreeMap<PageNumber, Option<Box<PageBuf>>>,
    ) -> bool {
        false
    }

    fn protect_pages_and_init_info(
        _memory_pages: &BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        _prog_id: ProgramId,
        _wasm_mem_begin_addr: usize,
    ) {
    }

    fn post_execution_actions(
        _memory_pages: &mut BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        _wasm_mem_begin_addr: usize,
    ) {
    }

    fn remove_lazy_pages_prot(_mem_addr: usize) {}

    fn protect_lazy_pages_and_update_wasm_mem_addr(_old_mem_addr: usize, _new_mem_addr: usize) {}

    fn get_lazy_pages_numbers() -> Vec<u32> {
        Vec::default()
    }
}

impl From<Ext> for ExtInfo {
    fn from(ext: Ext) -> ExtInfo {
        let accessed_pages_numbers = ext.memory_context.allocations().clone();
        let mut accessed_pages = BTreeMap::new();
        for page in accessed_pages_numbers {
            let mut buf = vec![0u8; PageNumber::size()];
            ext.get_mem(page.offset(), &mut buf);
            accessed_pages.insert(page, buf);
        }

        let nonce = ext.message_context.nonce();

        let (
            MessageState {
                outgoing,
                reply,
                awakening,
            },
            store,
        ) = ext.message_context.drain();

        let gas_amount: GasAmount = ext.gas_counter.into();

        let trap_explanation = ext.error_explanation;

        ExtInfo {
            gas_amount,
            pages: ext.memory_context.allocations().clone(),
            accessed_pages,
            outgoing,
            reply,
            awakening,
            nonce,
            payload_store: Some(store),
            trap_explanation,
            exit_argument: ext.exit_argument,
        }
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
    fn alloc(&mut self, pages_num: PageNumber) -> Result<PageNumber, &'static str> {
        // Greedily charge gas for allocations
        self.charge_gas(pages_num.raw() * self.config.alloc_cost as u32)?;
        // Greedily charge gas for grow
        self.charge_gas(pages_num.raw() * self.config.mem_grow_cost as u32)?;

        let old_mem_size = self.memory_context.memory().size().raw();

        let result = self
            .memory_context
            .alloc(pages_num)
            .map_err(|_e| "Allocation error");

        if result.is_err() {
            return self.return_and_store_err(result);
        }

        // Returns back greedily used gas for grow
        let new_mem_size = self.memory_context.memory().size().raw();
        let grow_pages_num = new_mem_size - old_mem_size;
        let mut gas_to_return_back =
            self.config.mem_grow_cost * (pages_num.raw() - grow_pages_num) as u64;

        // Returns back greedily used gas for allocations
        let first_page = result.unwrap().raw();
        let last_page = first_page + pages_num.raw() - 1;
        let mut new_alloced_pages_num = 0;
        for page in first_page..=last_page {
            if !self.memory_context.is_init_page(page.into()) {
                new_alloced_pages_num += 1;
            }
        }
        gas_to_return_back +=
            self.config.alloc_cost * (pages_num.raw() - new_alloced_pages_num) as u64;

        self.refund_gas(gas_to_return_back as u32)?;

        self.return_and_store_err(result)
    }

    fn block_height(&self) -> u32 {
        self.block_info.height
    }

    fn block_timestamp(&self) -> u64 {
        self.block_info.timestamp
    }

    fn initiator(&self) -> ProgramId {
        self.initiator
    }

    fn send_init(&mut self) -> Result<usize, &'static str> {
        let result = self
            .message_context
            .send_init()
            .map_err(|_e| "Message init error");

        self.return_and_store_err(result)
    }

    fn send_push(&mut self, handle: usize, buffer: &[u8]) -> Result<(), &'static str> {
        let result = self
            .message_context
            .send_push(handle, buffer)
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

    fn send_commit(
        &mut self,
        handle: usize,
        msg: OutgoingPacket,
    ) -> Result<MessageId, &'static str> {
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
            .send_commit(handle, msg)
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
        self.memory_context.program_id()
    }

    fn free(&mut self, ptr: PageNumber) -> Result<(), &'static str> {
        let result = self.memory_context.free(ptr).map_err(|_e| "Free error");

        // Returns back gas for allocated page if it's new
        if !self.memory_context.is_init_page(ptr) {
            self.refund_gas(self.config.alloc_cost as u32)?;
        }

        self.return_and_store_err(result)
    }

    fn debug(&mut self, data: &str) -> Result<(), &'static str> {
        log::debug!(target: "gwasm", "DEBUG: {}", data);

        Ok(())
    }

    fn set_mem(&mut self, ptr: usize, val: &[u8]) {
        self.memory_context
            .memory()
            .write(ptr, val)
            // TODO: remove and propagate error, issue #97
            .expect("Memory out of bounds.");
    }

    fn get_mem(&self, ptr: usize, buffer: &mut [u8]) {
        self.memory_context.memory().read(ptr, buffer);
    }

    fn msg(&mut self) -> &[u8] {
        self.message_context.current().payload()
    }

    fn charge_gas(&mut self, val: u32) -> Result<(), &'static str> {
        if self.gas_counter.charge(val as u64) == ChargeResult::Enough {
            Ok(())
        } else {
            self.return_and_store_err(Err("Gas limit exceeded"))
        }
    }

    fn refund_gas(&mut self, val: u32) -> Result<(), &'static str> {
        if self.gas_counter.refund(val as u64) == ChargeResult::Enough {
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

    fn get_wasm_memory_begin_addr(&self) -> usize {
        self.memory_context.memory().get_wasm_memory_begin_addr()
    }
}
