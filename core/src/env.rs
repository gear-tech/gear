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

//! Environment for running a module.

use crate::{
    costs::RuntimeCosts,
    ids::{MessageId, ProgramId},
    memory::{Memory, WasmPageNumber},
    message::{ExitCode, HandlePacket, InitPacket, ReplyPacket},
};
use alloc::collections::BTreeSet;
use codec::{Decode, Encode};
use gear_core_errors::CoreError;

/// Page access rights.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, Copy)]
pub enum PageAction {
    /// Can be read.
    Read,
    /// Can be written.
    Write,
    /// No access.
    None,
}

/// External api for managing memory, messages, allocations and gas-counting.
pub trait Ext {
    /// An error issued in api
    type Error: CoreError;

    /// Allocate number of pages.
    ///
    /// The resulting page number should point to `pages` consecutives memory pages.
    fn alloc(
        &mut self,
        pages: WasmPageNumber,
        mem: &mut impl Memory,
    ) -> Result<WasmPageNumber, Self::Error>;

    /// Get the current block height.
    fn block_height(&mut self) -> Result<u32, Self::Error>;

    /// Get the current block timestamp.
    fn block_timestamp(&mut self) -> Result<u64, Self::Error>;

    /// Get the id of the user who initiated communication with blockchain,
    /// during which, currently processing message was created.
    fn origin(&mut self) -> Result<ProgramId, Self::Error>;

    /// Initialize a new incomplete message for another program and return its handle.
    fn send_init(&mut self) -> Result<usize, Self::Error>;

    /// Push an extra buffer into message payload by handle.
    fn send_push(&mut self, handle: usize, buffer: &[u8]) -> Result<(), Self::Error>;

    /// Complete message and send it to another program.
    fn send_commit(&mut self, handle: usize, msg: HandlePacket) -> Result<MessageId, Self::Error>;

    /// Send message to another program.
    fn send(&mut self, msg: HandlePacket) -> Result<MessageId, Self::Error> {
        let handle = self.send_init()?;
        self.send_commit(handle, msg)
    }

    /// Push an extra buffer into reply message.
    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Self::Error>;

    /// Complete reply message and send it to source program.
    fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::Error>;

    /// Produce reply to the current message.
    fn reply(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::Error> {
        self.reply_commit(msg)
    }

    /// Read the message id, if current message is a reply.
    fn reply_to(&mut self) -> Result<Option<(MessageId, ExitCode)>, Self::Error>;

    /// Get the source of the message currently being handled.
    fn source(&mut self) -> Result<ProgramId, Self::Error>;

    /// Terminate the program and transfer all available value to the address.
    fn exit(&mut self) -> Result<(), Self::Error>;

    /// Get the id of the message currently being handled.
    fn message_id(&mut self) -> Result<MessageId, Self::Error>;

    /// Get the id of program itself
    fn program_id(&mut self) -> Result<ProgramId, Self::Error>;

    /// Free specific memory page.
    ///
    /// Unlike traditional allocator, if multiple pages allocated via `alloc`, all pages
    /// should be `free`-d separately.
    fn free(&mut self, page: WasmPageNumber) -> Result<(), Self::Error>;

    /// Send debug message.
    ///
    /// This should be no-op in release builds.
    fn debug(&mut self, data: &str) -> Result<(), Self::Error>;

    /// Interrupt the program, saving it's state.
    fn leave(&mut self) -> Result<(), Self::Error>;

    /// Access currently handled message payload.
    fn msg(&mut self) -> &[u8];

    /// Default gas host call.
    fn gas(&mut self, amount: u32) -> Result<(), Self::Error>;

    /// Charge some extra gas.
    fn charge_gas(&mut self, amount: u32) -> Result<(), Self::Error>;

    /// Charge gas by `RuntimeCosts` token.
    fn charge_gas_runtime(&mut self, costs: RuntimeCosts) -> Result<(), Self::Error>;

    /// Refund some gas.
    fn refund_gas(&mut self, amount: u32) -> Result<(), Self::Error>;

    /// Tell how much gas is left in running context.
    fn gas_available(&mut self) -> Result<u64, Self::Error>;

    /// Value associated with message.
    fn value(&mut self) -> Result<u128, Self::Error>;

    /// Tell how much value is left in running context.
    fn value_available(&mut self) -> Result<u128, Self::Error>;

    /// Interrupt the program and reschedule execution.
    fn wait(&mut self) -> Result<(), Self::Error>;

    /// Wake the waiting message and move it to the processing queue.
    fn wake(&mut self, waker_id: MessageId) -> Result<(), Self::Error>;

    /// Send init message to create a new program
    fn create_program(&mut self, packet: InitPacket) -> Result<ProgramId, Self::Error>;

    /// Return the set of functions that are forbidden to be called.
    fn forbidden_funcs(&self) -> &BTreeSet<&'static str>;
}

/// # TODO
///
/// remove this trait after the refactor
pub trait ExtHelper<E: Ext> {
    /// Calls fallible fn with inner ext.
    fn with<T, U>(&mut self, f: impl FnOnce(&mut E) -> Result<T, U>) -> Result<T, U>;

    /// Calls fallible fn with inner ext.
    fn with_fallible<T, U>(&mut self, f: impl FnOnce(&mut E) -> Result<T, U>) -> Result<T, U>;
}

impl<E> ExtHelper<E> for E
where
    E: Ext,
{
    /// Calls fallible fn with inner ext.
    fn with<T, U>(&mut self, f: impl FnOnce(&mut E) -> Result<T, U>) -> Result<T, U> {
        f(self)
    }

    /// Calls fallible fn with inner ext.
    fn with_fallible<T, U>(&mut self, f: impl FnOnce(&mut E) -> Result<T, U>) -> Result<T, U> {
        f(self)
    }
}

/// Ext with function context
///
/// Rerseved memory operations for adapting wasmtime
pub trait FunctionContext<E: Ext> {
    /// Memory Error
    type Error;
    /// Memory type
    type Memory;

    /// Get `Ext`
    fn ext(&self) -> &E;

    /// Get mutatable `Ext`
    fn ext_mut(&mut self) -> &mut E;

    /// Read memory into buf
    fn read_memory_into(
        &mut self,
        mem: &Self::Memory,
        offset: usize,
        buffer: &mut [u8],
    ) -> Result<(), Self::Error>;

    /// Read memory into buf
    fn write_into_memory(
        &mut self,
        mem: &Self::Memory,
        offset: usize,
        buffer: &[u8],
    ) -> Result<(), Self::Error>;
}
