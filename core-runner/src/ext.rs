// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use gear_core::{
    env::Ext as EnvExt,
    gas::{ChargeResult, GasCounter},
    memory::{MemoryContext, PageNumber},
    message::{ExitCode, MessageContext, MessageId, OutgoingPacket, ReplyPacket},
    program::ProgramId,
};

use crate::util::BlakeMessageIdGenerator;

use alloc::boxed::Box;

/// Structure providing externalities for running host functions.
pub struct Ext {
    /// Memory context.
    pub memory_context: MemoryContext,
    /// Message context.
    pub messages: MessageContext<BlakeMessageIdGenerator>,
    /// Gas counter.
    pub gas_counter: Box<dyn GasCounter>,
    /// Cost per allocation.
    pub alloc_cost: u64,
    /// Cost per gmemory grow.
    pub mem_grow_cost: u64,
    /// Any guest code panic explanation, if available.
    pub last_error_returned: Option<&'static str>,
    /// Current block height.
    pub block_height: u32,
}

impl Ext {
    fn return_with_tracing<T>(
        &mut self,
        result: Result<T, &'static str>,
    ) -> Result<T, &'static str> {
        match result {
            Ok(result) => Ok(result),
            Err(error_string) => {
                self.last_error_returned = Some(error_string);
                Err(error_string)
            }
        }
    }
}

impl EnvExt for Ext {
    fn alloc(&mut self, pages_num: PageNumber) -> Result<PageNumber, &'static str> {
        // Greedily charge gas for allocations
        self.gas(pages_num.raw() * self.alloc_cost as u32)?;
        // Greedily charge gas for grow
        self.gas(pages_num.raw() * self.mem_grow_cost as u32)?;

        let old_mem_size = self.memory_context.memory().size().raw();

        let result = self
            .memory_context
            .alloc(pages_num)
            .map_err(|_e| "Allocation error");

        if result.is_err() {
            return self.return_with_tracing(result);
        }

        // Returns back greedly used gas for grow
        let new_mem_size = self.memory_context.memory().size().raw();
        let grow_pages_num = new_mem_size - old_mem_size;
        let mut gas_to_return_back = self.mem_grow_cost * (pages_num.raw() - grow_pages_num) as u64;

        // Returns back greedly used gas for allocations
        let first_page = result.unwrap().raw();
        let last_page = first_page + pages_num.raw() - 1;
        let mut new_alloced_pages_num = 0;
        for page in first_page..=last_page {
            if !self.memory_context.is_init_page(page.into()) {
                new_alloced_pages_num += 1;
            }
        }
        gas_to_return_back += self.alloc_cost * (pages_num.raw() - new_alloced_pages_num) as u64;

        if self.gas_counter.refund(gas_to_return_back) != ChargeResult::Enough {
            return self.return_with_tracing(Err("Gas limit - add too many gas"));
        }

        self.return_with_tracing(result)
    }

    fn block_height(&self) -> u32 {
        self.block_height
    }

    fn send_init(&mut self) -> Result<usize, &'static str> {
        let result = self.messages.send_init().map_err(|_e| "Message init error");

        self.return_with_tracing(result)
    }

    fn send_push(&mut self, handle: usize, buffer: &[u8]) -> Result<(), &'static str> {
        let result = self
            .messages
            .send_push(handle, buffer)
            .map_err(|_e| "Payload push error");

        self.return_with_tracing(result)
    }

    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), &'static str> {
        let result = self
            .messages
            .reply_push(buffer)
            .map_err(|_e| "Reply payload push error");

        self.return_with_tracing(result)
    }

    fn send_commit(
        &mut self,
        handle: usize,
        msg: OutgoingPacket,
    ) -> Result<MessageId, &'static str> {
        if self.gas_counter.reduce(msg.gas_limit()) != ChargeResult::Enough {
            return self
                .return_with_tracing(Err("Gas limit exceeded while trying to send message"));
        };

        let result = self
            .messages
            .send_commit(handle, msg)
            .map_err(|_e| "Message commit error");

        self.return_with_tracing(result)
    }

    fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, &'static str> {
        if self.gas_counter.reduce(msg.gas_limit()) != ChargeResult::Enough {
            return self.return_with_tracing(Err("Gas limit exceeded while trying to reply"));
        };

        let result = self
            .messages
            .reply_commit(msg)
            .map_err(|_e| "Reply commit error");

        self.return_with_tracing(result)
    }

    fn reply_to(&self) -> Option<(MessageId, ExitCode)> {
        self.messages.current().reply()
    }

    fn source(&mut self) -> ProgramId {
        self.messages.current().source()
    }

    fn message_id(&mut self) -> MessageId {
        self.messages.current().id()
    }

    fn free(&mut self, ptr: PageNumber) -> Result<(), &'static str> {
        let result = self.memory_context.free(ptr).map_err(|_e| "Free error");

        // Returns back gas for allocated page if it's new
        if !self.memory_context.is_init_page(ptr)
            && self.gas_counter.refund(self.alloc_cost) != ChargeResult::Enough
        {
            return self.return_with_tracing(Err("Gas limit - add too many gas"));
        }

        self.return_with_tracing(result)
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
        self.messages.current().payload()
    }

    fn gas(&mut self, val: u32) -> Result<(), &'static str> {
        if self.gas_counter.charge(val as u64) == ChargeResult::Enough {
            Ok(())
        } else {
            self.return_with_tracing(Err("Gas limit exceeded"))
        }
    }

    fn gas_available(&mut self) -> u64 {
        self.gas_counter.left()
    }

    fn value(&self) -> u128 {
        self.messages.current().value()
    }

    fn wait(&mut self) -> Result<(), &'static str> {
        let result = self
            .messages
            .wait()
            .map_err(|_| "Unable to add the message to the wait list");

        self.return_with_tracing(result)
    }

    fn wake(&mut self, waker_id: MessageId, gas_limit: u64) -> Result<(), &'static str> {
        let result = self
            .messages
            .wake(waker_id, gas_limit)
            .map_err(|_| "Unable to mark the message to be woken");

        self.return_with_tracing(result)
    }
}
