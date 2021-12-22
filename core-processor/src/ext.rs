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

use crate::{
    configs::{AllocationsConfig, BlockInfo},
    id::BlakeMessageIdGenerator,
};
use gear_core::{
    env::Ext as EnvExt,
    gas::{ChargeResult, GasCounter},
    memory::{MemoryContext, PageNumber},
    message::{ExitCode, MessageContext, MessageId, OutgoingPacket, ReplyPacket},
    program::ProgramId,
};

/// Structure providing externalities for running host functions.
pub struct Ext {
    /// Gas counter.
    pub gas_counter: GasCounter,
    /// Memory context.
    pub memory_context: MemoryContext,
    /// Message context.
    pub message_context: MessageContext<BlakeMessageIdGenerator>,
    // Block info.
    pub block_info: BlockInfo,
    /// Allocations config.
    pub config: AllocationsConfig,
    /// Any guest code panic explanation, if available.
    pub error_explanation: Option<&'static str>,
    /// Flag signaling whether the execution interrupts and goes to the waiting state.
    pub waited: bool,
}

impl Ext {
    fn return_and_store_err<T>(
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

        // Returns back greedly used gas for grow
        let new_mem_size = self.memory_context.memory().size().raw();
        let grow_pages_num = new_mem_size - old_mem_size;
        let mut gas_to_return_back =
            self.config.mem_grow_cost * (pages_num.raw() - grow_pages_num) as u64;

        // Returns back greedly used gas for allocations
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
        if self.gas_counter.reduce(msg.gas_limit()) != ChargeResult::Enough {
            return self
                .return_and_store_err(Err("Gas limit exceeded while trying to send message"));
        };

        let result = self
            .message_context
            .send_commit(handle, msg)
            .map_err(|_e| "Message commit error");

        self.return_and_store_err(result)
    }

    fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, &'static str> {
        if self.gas_counter.reduce(msg.gas_limit()) != ChargeResult::Enough {
            return self.return_and_store_err(Err("Gas limit exceeded while trying to reply"));
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

    fn message_id(&mut self) -> MessageId {
        self.message_context.current().id()
    }

    fn program_id(&mut self) -> ProgramId {
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

    fn gas_available(&mut self) -> u64 {
        self.gas_counter.left()
    }

    fn value(&self) -> u128 {
        self.message_context.current().value()
    }

    fn wait(&mut self) -> Result<(), &'static str> {
        let result = self
            .message_context
            .check_uncommitted()
            .map_err(|_| "There are uncommited messages when passing to waiting state")
            .and_then(|_| {
                if self.waited {
                    Err("Cannot pass to the waiting state twice")
                } else {
                    self.waited = true;
                    Ok(())
                }
            });

        self.return_and_store_err(result)
    }

    fn wake(&mut self, waker_id: MessageId) -> Result<(), &'static str> {
        let result = self
            .message_context
            .wake(waker_id)
            .map_err(|_| "Unable to mark the message to be woken");

        self.return_and_store_err(result)
    }
}
