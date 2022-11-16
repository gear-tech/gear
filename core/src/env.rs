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
    ids::{MessageId, ProgramId, ReservationId},
    memory::{Memory, WasmPageNumber},
    message::{HandlePacket, InitPacket, ReplyPacket, StatusCode},
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
    /// The resulting page number should point to `pages` consecutive memory pages.
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
    fn send_init(&mut self) -> Result<u32, Self::Error>;

    /// Push an extra buffer into message payload by handle.
    fn send_push(&mut self, handle: u32, buffer: &[u8]) -> Result<(), Self::Error>;

    /// Complete message and send it to another program.
    fn send_commit(
        &mut self,
        handle: u32,
        msg: HandlePacket,
        delay: u32,
    ) -> Result<MessageId, Self::Error>;

    /// Send message to another program.
    fn send(&mut self, msg: HandlePacket, delay: u32) -> Result<MessageId, Self::Error> {
        let handle = self.send_init()?;
        self.send_commit(handle, msg, delay)
    }

    /// Complete message and send it to another program using gas from reservation.
    fn reservation_send_commit(
        &mut self,
        id: ReservationId,
        handle: u32,
        msg: HandlePacket,
        delay: u32,
    ) -> Result<MessageId, Self::Error>;

    /// Send message to another program using gas from reservation.
    fn reservation_send(
        &mut self,
        id: ReservationId,
        msg: HandlePacket,
        delay: u32,
    ) -> Result<MessageId, Self::Error> {
        let handle = self.send_init()?;
        self.reservation_send_commit(id, handle, msg, delay)
    }

    /// Push an extra buffer into reply message.
    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Self::Error>;

    /// Complete reply message and send it to source program.
    fn reply_commit(&mut self, msg: ReplyPacket, delay: u32) -> Result<MessageId, Self::Error>;

    /// Complete reply message and send it to source program from reservation.
    fn reservation_reply_commit(
        &mut self,
        id: ReservationId,
        msg: ReplyPacket,
        delay: u32,
    ) -> Result<MessageId, Self::Error>;

    /// Produce reply to the current message.
    fn reply(&mut self, msg: ReplyPacket, delay: u32) -> Result<MessageId, Self::Error> {
        self.reply_commit(msg, delay)
    }

    /// Produce reply to the current message from reservation.
    fn reservation_reply(
        &mut self,
        id: ReservationId,
        msg: ReplyPacket,
        delay: u32,
    ) -> Result<MessageId, Self::Error> {
        self.reservation_reply_commit(id, msg, delay)
    }

    /// Get the message id of the initial message.
    fn reply_to(&mut self) -> Result<MessageId, Self::Error>;

    /// Get the source of the message currently being handled.
    fn source(&mut self) -> Result<ProgramId, Self::Error>;

    /// Terminate the program and transfer all available value to the address.
    fn exit(&mut self) -> Result<(), Self::Error>;

    /// Get the status code of the message being processed.
    fn status_code(&mut self) -> Result<StatusCode, Self::Error>;

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

    /// Emit error.
    fn error(&mut self, data: &[u8]) -> Result<(), Self::Error>;

    /// Interrupt the program, saving it's state.
    fn leave(&mut self) -> Result<(), Self::Error>;

    /// Access currently handled message payload.
    fn read(&mut self) -> Result<&[u8], Self::Error>;

    /// Size of currently handled message payload.
    fn size(&mut self) -> Result<usize, Self::Error>;

    /// Returns a random seed for the current block with message id as a subject, along with the time in the past since when it was determinable by chain observers.
    fn random(&self) -> (&[u8], u32);

    /// Charge some extra gas.
    fn charge_gas(&mut self, amount: u64) -> Result<(), Self::Error>;

    /// Charge gas by `RuntimeCosts` token.
    fn charge_gas_runtime(&mut self, costs: RuntimeCosts) -> Result<(), Self::Error>;

    /// Refund some gas.
    fn refund_gas(&mut self, amount: u64) -> Result<(), Self::Error>;

    /// Reserve some gas for a few blocks.
    fn reserve_gas(&mut self, amount: u64, duration: u32) -> Result<ReservationId, Self::Error>;

    /// Unreserve gas using reservation ID.
    fn unreserve_gas(&mut self, id: ReservationId) -> Result<u64, Self::Error>;

    /// Do system reservation.
    fn system_reserve_gas(&mut self, amount: u64) -> Result<(), Self::Error>;

    /// Tell how much gas is left in running context.
    fn gas_available(&mut self) -> Result<u64, Self::Error>;

    /// Value associated with message.
    fn value(&mut self) -> Result<u128, Self::Error>;

    /// Tell how much value is left in running context.
    fn value_available(&mut self) -> Result<u128, Self::Error>;

    /// Interrupt the program and reschedule execution for maximum.
    fn wait(&mut self) -> Result<(), Self::Error>;

    /// Interrupt the program and reschedule execution in duration.
    fn wait_for(&mut self, duration: u32) -> Result<(), Self::Error>;

    /// Interrupt the program and reschedule execution for maximum,
    /// but not more than duration.
    fn wait_up_to(&mut self, duration: u32) -> Result<bool, Self::Error>;

    /// Wake the waiting message and move it to the processing queue.
    fn wake(&mut self, waker_id: MessageId, delay: u32) -> Result<(), Self::Error>;

    /// Send init message to create a new program
    fn create_program(
        &mut self,
        packet: InitPacket,
        delay: u32,
    ) -> Result<(MessageId, ProgramId), Self::Error>;

    /// Return the set of functions that are forbidden to be called.
    fn forbidden_funcs(&self) -> &BTreeSet<&'static str>;

    /// Return gas and gas allowance left in the counters.
    fn counters(&self) -> (u64, u64);

    /// Update counters with the provided values.
    fn update_counters(&mut self, gas: u64, allowance: u64);

    /// Handler for the case when gas is out.
    fn out_of_gas(&mut self) -> Self::Error;

    /// Handler for the case when gas allowance is out.
    fn out_of_allowance(&mut self) -> Self::Error;
}
