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

//! Environment for running a module.

use crate::{
    buffer::PayloadSlice,
    env_vars::EnvVars,
    ids::{ActorId, MessageId, ReservationId},
    memory::Memory,
    message::{DispatchKind, HandlePacket, InitPacket, MessageContext, ReplyPacket},
    pages::WasmPage,
};
use alloc::{collections::BTreeSet, string::String};
use core::fmt::Display;
use gear_core_errors::{ReplyCode, SignalCode};
use gear_wasm_instrument::syscalls::SyscallName;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

/// External api and data for managing memory and messages,
/// use by an executing program to trigger state transition
/// in runtime.
pub trait Externalities {
    /// An error issued in infallible syscall.
    type UnrecoverableError;

    /// An error issued in fallible syscall.
    type FallibleError;

    /// An error issued during allocation.
    type AllocError: Display;

    /// Allocate number of pages.
    ///
    /// The resulting page number should point to `pages` consecutive memory pages.
    fn alloc<Context>(
        &mut self,
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
        pages_num: u32,
    ) -> Result<WasmPage, Self::AllocError>;

    /// Free specific page.
    fn free(&mut self, page: WasmPage) -> Result<(), Self::AllocError>;

    /// Free specific memory range.
    fn free_range(&mut self, start: WasmPage, end: WasmPage) -> Result<(), Self::AllocError>;

    /// Get environment variables currently set in the system and in the form
    /// corresponded to the requested version.
    fn env_vars(&self, version: u32) -> Result<EnvVars, Self::UnrecoverableError>;

    /// Get the current block height.
    fn block_height(&self) -> Result<u32, Self::UnrecoverableError>;

    /// Get the current block timestamp.
    fn block_timestamp(&self) -> Result<u64, Self::UnrecoverableError>;

    /// Initialize a new incomplete message for another program and return its handle.
    fn send_init(&mut self) -> Result<u32, Self::FallibleError>;

    /// Push an extra buffer into message payload by handle.
    fn send_push(&mut self, handle: u32, buffer: &[u8]) -> Result<(), Self::FallibleError>;

    /// Complete message and send it to another program.
    fn send_commit(
        &mut self,
        handle: u32,
        msg: HandlePacket,
        delay: u32,
    ) -> Result<MessageId, Self::FallibleError>;

    /// Send message to another program.
    fn send(&mut self, msg: HandlePacket, delay: u32) -> Result<MessageId, Self::FallibleError> {
        let handle = self.send_init()?;
        self.send_commit(handle, msg, delay)
    }

    /// Push the incoming message buffer into message payload by handle.
    fn send_push_input(
        &mut self,
        handle: u32,
        offset: u32,
        len: u32,
    ) -> Result<(), Self::FallibleError>;

    /// Complete message and send it to another program using gas from reservation.
    fn reservation_send_commit(
        &mut self,
        id: ReservationId,
        handle: u32,
        msg: HandlePacket,
        delay: u32,
    ) -> Result<MessageId, Self::FallibleError>;

    /// Send message to another program using gas from reservation.
    fn reservation_send(
        &mut self,
        id: ReservationId,
        msg: HandlePacket,
        delay: u32,
    ) -> Result<MessageId, Self::FallibleError> {
        let handle = self.send_init()?;
        self.reservation_send_commit(id, handle, msg, delay)
    }

    /// Push an extra buffer into reply message.
    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Self::FallibleError>;

    /// Complete reply message and send it to source program.
    fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::FallibleError>;

    /// Complete reply message and send it to source program from reservation.
    fn reservation_reply_commit(
        &mut self,
        id: ReservationId,
        msg: ReplyPacket,
    ) -> Result<MessageId, Self::FallibleError>;

    /// Produce reply to the current message.
    fn reply(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::FallibleError> {
        self.reply_commit(msg)
    }

    /// Produce reply to the current message from reservation.
    fn reservation_reply(
        &mut self,
        id: ReservationId,
        msg: ReplyPacket,
    ) -> Result<MessageId, Self::FallibleError> {
        self.reservation_reply_commit(id, msg)
    }

    /// Get the message id of the initial message.
    fn reply_to(&self) -> Result<MessageId, Self::FallibleError>;

    /// Get the message id which signal issues from.
    fn signal_from(&self) -> Result<MessageId, Self::FallibleError>;

    /// Push the incoming message buffer into reply message.
    fn reply_push_input(&mut self, offset: u32, len: u32) -> Result<(), Self::FallibleError>;

    /// Get the source of the message currently being handled.
    fn source(&self) -> Result<ActorId, Self::UnrecoverableError>;

    /// Get the reply code if the message being processed.
    fn reply_code(&self) -> Result<ReplyCode, Self::FallibleError>;

    /// Get the signal code if the message being processed.
    fn signal_code(&self) -> Result<SignalCode, Self::FallibleError>;

    /// Get the id of the message currently being handled.
    fn message_id(&self) -> Result<MessageId, Self::UnrecoverableError>;

    /// Get the id of program itself
    fn program_id(&self) -> Result<ActorId, Self::UnrecoverableError>;

    /// Send debug message.
    ///
    /// This should be no-op in release builds.
    fn debug(&self, data: &str) -> Result<(), Self::UnrecoverableError>;

    /// Get the currently handled message payload slice.
    fn payload_slice(&mut self, at: u32, len: u32) -> Result<PayloadSlice, Self::FallibleError>;

    /// Size of currently handled message payload.
    fn size(&self) -> Result<usize, Self::UnrecoverableError>;

    /// Returns a random seed for the current block with message id as a subject, along with the time in the past since when it was determinable by chain observers.
    fn random(&self) -> Result<(&[u8], u32), Self::UnrecoverableError>;

    /// Reserve some gas for a few blocks.
    fn reserve_gas(
        &mut self,
        amount: u64,
        duration: u32,
    ) -> Result<ReservationId, Self::FallibleError>;

    /// Unreserve gas using reservation ID.
    fn unreserve_gas(&mut self, id: ReservationId) -> Result<u64, Self::FallibleError>;

    /// Do system reservation.
    fn system_reserve_gas(&mut self, amount: u64) -> Result<(), Self::FallibleError>;

    /// Tell how much gas is left in running context.
    fn gas_available(&self) -> Result<u64, Self::UnrecoverableError>;

    /// Value associated with message.
    fn value(&self) -> Result<u128, Self::UnrecoverableError>;

    /// Tell how much value is left in running context.
    fn value_available(&self) -> Result<u128, Self::UnrecoverableError>;

    /// Interrupt the program and reschedule execution for maximum.
    fn wait(&mut self) -> Result<(), Self::UnrecoverableError>;

    /// Interrupt the program and reschedule execution in duration.
    fn wait_for(&mut self, duration: u32) -> Result<(), Self::UnrecoverableError>;

    /// Interrupt the program and reschedule execution for maximum,
    /// but not more than duration.
    fn wait_up_to(&mut self, duration: u32) -> Result<bool, Self::UnrecoverableError>;

    /// Wake the waiting message and move it to the processing queue.
    fn wake(&mut self, waker_id: MessageId, delay: u32) -> Result<(), Self::FallibleError>;

    /// Send init message to create a new program.
    fn create_program(
        &mut self,
        packet: InitPacket,
        delay: u32,
    ) -> Result<(MessageId, ActorId), Self::FallibleError>;

    /// Create deposit to handle reply on given message.
    fn reply_deposit(
        &mut self,
        message_id: MessageId,
        amount: u64,
    ) -> Result<(), Self::FallibleError>;

    /// Return the set of functions that are forbidden to be called.
    fn forbidden_funcs(&self) -> &BTreeSet<SyscallName>;

    /// Return the current message context.
    fn msg_ctx(&self) -> &MessageContext;
}

/// Composite wait type for messages waiting.
#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, PartialOrd, Ord, TypeInfo)]
pub enum MessageWaitedType {
    /// Program called `gr_wait` while executing message.
    Wait,
    /// Program called `gr_wait_for` while executing message.
    WaitFor,
    /// Program called `gr_wait_up_to` with insufficient gas for full
    /// duration while executing message.
    WaitUpTo,
    /// Program called `gr_wait_up_to` with enough gas for full duration
    /// storing while executing message.
    WaitUpToFull,
}

/// Trait defining type could be used as entry point for a wasm module.
pub trait WasmEntryPoint: Sized {
    /// Converting self into entry point name.
    fn as_entry(&self) -> &str;

    /// Converting entry point name into self object, if possible.
    fn try_from_entry(entry: &str) -> Option<Self>;

    /// Tries to convert self into `DispatchKind`.
    fn try_into_kind(&self) -> Option<DispatchKind> {
        <DispatchKind as WasmEntryPoint>::try_from_entry(self.as_entry())
    }
}

impl WasmEntryPoint for String {
    fn as_entry(&self) -> &str {
        self
    }

    fn try_from_entry(entry: &str) -> Option<Self> {
        Some(entry.into())
    }
}

impl WasmEntryPoint for DispatchKind {
    fn as_entry(&self) -> &str {
        match self {
            Self::Init => "init",
            Self::Handle => "handle",
            Self::Reply => "handle_reply",
            Self::Signal => "handle_signal",
        }
    }

    fn try_from_entry(entry: &str) -> Option<Self> {
        let kind = match entry {
            "init" => Self::Init,
            "handle" => Self::Handle,
            "handle_reply" => Self::Reply,
            "handle_signal" => Self::Signal,
            _ => return None,
        };

        Some(kind)
    }
}
