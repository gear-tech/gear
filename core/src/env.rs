// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
    ids::{MessageId, ProgramId, ReservationId},
    memory::{Memory, WasmPage},
    message::{HandlePacket, InitPacket, MessageContext, Payload, ReplyPacket, StatusCode},
};
use alloc::collections::BTreeSet;
use core::{fmt::Display, mem};
use gear_wasm_instrument::syscalls::SysCallName;

/// Holder for the payload of the incoming/currently executing message.
///
/// The type was mainly introduced to establish type safety mechanics
/// for the read of the payload from externalities. To type's purposes
/// see [`Externalities::lend_payload`] docs.
///
/// ### Usage
/// This type gives access to some slice of the currently executing message
/// payload, but doesn't do it directly. It gives to the caller the [`PayloadToSlice`]
/// wrapper, which actually can return the slice of the payload. But this wrapper
/// is instantiated only inside the [`Self::use_with_job`] method.
/// This is actually done to prevent a user of the type from holding payload of the
/// message, which actually moves it, from forgetting to release it back, because
/// if access to the slice buffer was granted directly from the holder, the type user
/// could have written the data to memory and then have dropped the holder. As a result
/// the executing message payload wouldn't have been returned. So [`PayloadSliceHolder::use_with_job`]
/// is a kind of scope-guard for the data and the [`PayloadToSlice`] is a data access guard.
///
/// For more usage info read [`Self::use_with_job`] docs.

pub struct PayloadSliceHolder {
    /// Held payload
    payload: Payload,
    /// Range values indicating slice bounds.
    range: (usize, usize),
}

impl PayloadSliceHolder {
    /// Creates a new [`PayloadSliceHolder`] from the currently executed message context.
    ///
    /// The method checks whether received range (slice) is correct, i.e., the end is lower
    /// than payload's length. If the check goes well, the ownership over payload will be
    /// taken from the message context by [`mem::take`].
    pub fn try_new((start, end): (u32, u32), msg_ctx: &mut MessageContext) -> Result<Self, usize> {
        let payload_len = msg_ctx.payload_mut().inner().len();
        if end as usize > payload_len {
            return Err(payload_len);
        }

        Ok(Self {
            payload: mem::take(msg_ctx.payload_mut()),
            range: (start as usize, end as usize),
        })
    }

    /// Releases back ownership of the held payload to the message context.
    ///
    /// The method actually performs [`mem::swap`] under the hood. It's supposed
    /// to be called from [`Externalities::reclaim_payload`], implementor of which
    /// owns provided message context.
    pub fn release_back(&mut self, msg_ctx: &mut MessageContext) {
        mem::swap(msg_ctx.payload_mut(), &mut self.payload);
    }

    /// Uses the holder in the provided `job` and drops the holder after running it.
    ///
    /// [`PayloadSliceHolder`]'s main purpose is to provide safe access to the payload's
    /// slice and ensure it will be returned back to the message.
    ///
    /// Type docs explain how safe access is designed with [`PayloadToSlice`].
    ///
    /// We ensure that the payload is released back by returning the [`UsePayloadHolderBoundResult`]
    /// from the `job`. This type can actually be instantiated only from tuple of two:
    /// [`ReclaimBoundResult`] and some result with err variant type to be `JobErr`.
    /// The first is returned from [`Externalities::reclaim_payload`], so it means that
    /// that payload was reclaimed by the original owner. The other result stores actual
    /// error of the `Job` as it could have called fallible actions inside it. So,
    /// [`UsePayloadHolderBoundResult`] gives an opportunity to store the actual result of the job,
    /// but also gives guarantee that payload was reclaimed.
    pub fn use_with_job<JobErr, Job>(mut self, mut job: Job) -> UsePayloadHolderBoundResult<JobErr>
    where
        Job: FnMut(PayloadToSlice<'_>) -> UsePayloadHolderBoundResult<JobErr>,
    {
        let held_range = PayloadToSlice(&mut self);
        job(held_range)
    }

    fn in_range(&self) -> &[u8] {
        let (start, end) = self.range;
        // Will not panic as range is checked.
        &self.payload.inner()[start..end]
    }
}

/// A wrapper over mutable reference to [`PayloadSliceHolder`]
/// which can give to the caller the slice of the held payload.
///
/// For more information read [`PayloadSliceHolder`] docs.
pub struct PayloadToSlice<'a>(&'a mut PayloadSliceHolder);

impl<'a> PayloadToSlice<'a> {
    /// Returns slice of the held payload.
    pub fn to_slice(&self) -> &[u8] {
        self.0.in_range()
    }

    /// Converts the wrapper into [`PayloadSliceHolder`].
    pub fn into_holder(self) -> &'a mut PayloadSliceHolder {
        self.0
    }
}

/// Result of calling a `job` within [`PayloadSliceHolder::use_with_job`].
///
/// This is a "bound" type which means it's main purpose is to give
/// some type-level guarantees. More precisely, it gives guarantee
/// that payload value was reclaimed by the owner. Also it stores the error
/// of the `job`, which gives opportunity to handle the actual job's runtime
/// error, but not bound wrappers.
pub struct UsePayloadHolderBoundResult<JobError> {
    job_result: Result<(), JobError>,
}

impl<JobErr> UsePayloadHolderBoundResult<JobErr> {
    /// Convert into inner job of the [`PayloadSliceHolder::use_with_job`] result.
    pub fn into_inner(self) -> Result<(), JobErr> {
        self.job_result
    }
}

impl<JobErr> From<(ReclaimBoundResult, Result<(), JobErr>)>
    for UsePayloadHolderBoundResult<JobErr>
{
    fn from((_token, job_result): (ReclaimBoundResult, Result<(), JobErr>)) -> Self {
        UsePayloadHolderBoundResult { job_result }
    }
}

/// Result of calling [`Externalities::reclaim_payload`].
///
/// This is a "bound" type which means it doesn't store
/// anything, but gives type-level guarantees that [`PayloadSliceHolder`]
/// released the payload back to the message context.
pub struct ReclaimBoundResult(());

impl From<(&mut MessageContext, &mut PayloadSliceHolder)> for ReclaimBoundResult {
    fn from((msg_ctx, payload_holder): (&mut MessageContext, &mut PayloadSliceHolder)) -> Self {
        payload_holder.release_back(msg_ctx);

        ReclaimBoundResult(())
    }
}

/// External api and data for managing memory and messages,
/// use by an executing program to trigger state transition
/// in runtime.
pub trait Externalities {
    /// An error issued in api.
    type Error;
    /// An error issued during allocation.
    type AllocError: Display;

    /// Allocate number of pages.
    ///
    /// The resulting page number should point to `pages` consecutive memory pages.
    fn alloc(
        &mut self,
        pages_num: u32,
        mem: &mut impl Memory,
    ) -> Result<WasmPage, Self::AllocError>;

    /// Free specific memory page.
    ///
    /// Unlike traditional allocator, if multiple pages allocated via `alloc`, all pages
    /// should be `free`-d separately.
    fn free(&mut self, page: WasmPage) -> Result<(), Self::AllocError>;

    /// Get the current block height.
    fn block_height(&self) -> Result<u32, Self::Error>;

    /// Get the current block timestamp.
    fn block_timestamp(&self) -> Result<u64, Self::Error>;

    /// Get the id of the user who initiated communication with blockchain,
    /// during which, currently processing message was created.
    fn origin(&self) -> Result<ProgramId, Self::Error>;

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

    /// Push the incoming message buffer into message payload by handle.
    fn send_push_input(&mut self, handle: u32, offset: u32, len: u32) -> Result<(), Self::Error>;

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
    fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::Error>;

    /// Complete reply message and send it to source program from reservation.
    fn reservation_reply_commit(
        &mut self,
        id: ReservationId,
        msg: ReplyPacket,
    ) -> Result<MessageId, Self::Error>;

    /// Produce reply to the current message.
    fn reply(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::Error> {
        self.reply_commit(msg)
    }

    /// Produce reply to the current message from reservation.
    fn reservation_reply(
        &mut self,
        id: ReservationId,
        msg: ReplyPacket,
    ) -> Result<MessageId, Self::Error> {
        self.reservation_reply_commit(id, msg)
    }

    /// Get the message id of the initial message.
    fn reply_to(&self) -> Result<MessageId, Self::Error>;

    /// Get the message id which signal issues from.
    fn signal_from(&self) -> Result<MessageId, Self::Error>;

    /// Push the incoming message buffer into reply message.
    fn reply_push_input(&mut self, offset: u32, len: u32) -> Result<(), Self::Error>;

    /// Get the source of the message currently being handled.
    fn source(&self) -> Result<ProgramId, Self::Error>;

    /// Get the status code of the message being processed.
    fn status_code(&self) -> Result<StatusCode, Self::Error>;

    /// Get the id of the message currently being handled.
    fn message_id(&self) -> Result<MessageId, Self::Error>;

    /// Pay rent for the specified program.
    fn pay_program_rent(
        &mut self,
        program_id: ProgramId,
        rent: u128,
    ) -> Result<(u128, u32), Self::Error>;

    /// Get the id of program itself
    fn program_id(&self) -> Result<ProgramId, Self::Error>;

    /// Send debug message.
    ///
    /// This should be no-op in release builds.
    fn debug(&self, data: &str) -> Result<(), Self::Error>;

    /// Takes ownership over payload of the executing message and
    /// returns it in the wrapper [`PayloadSliceHolder`].
    ///
    /// Due to details of implementation of the runtime which executes gear
    /// sys-calls inside wasm execution environment (either wasmi or sp_sandbox),
    /// to prevent additional memory allocation on payload read op, we give
    /// ownership over payload to the caller. Giving ownership over payload actually
    /// means, that the payload value in the currently executed message will become
    /// "de-allocated" or just zeroed. To prevent from the risk of payload being
    /// not "returned" back to the message a [`Externalities::reclaim_payload`] is
    /// introduced. For more info, read docs to [`PayloadSliceHolder`], [`UsePayloadHolderBoundResult`],
    /// [`ReclaimBoundResult`], [`PayloadToSlice`] types and their methods.
    fn lend_payload(&mut self, at: u32, len: u32) -> Result<PayloadSliceHolder, Self::Error>;

    /// Reclaims ownership from the payload holder over previously taken payload from the
    /// currently executing message.
    fn reclaim_payload(&mut self, payload_holder: &mut PayloadSliceHolder) -> ReclaimBoundResult;

    /// Size of currently handled message payload.
    fn size(&self) -> Result<usize, Self::Error>;

    /// Returns a random seed for the current block with message id as a subject, along with the time in the past since when it was determinable by chain observers.
    fn random(&self) -> Result<(&[u8], u32), Self::Error>;

    /// Reserve some gas for a few blocks.
    fn reserve_gas(&mut self, amount: u64, duration: u32) -> Result<ReservationId, Self::Error>;

    /// Unreserve gas using reservation ID.
    fn unreserve_gas(&mut self, id: ReservationId) -> Result<u64, Self::Error>;

    /// Do system reservation.
    fn system_reserve_gas(&mut self, amount: u64) -> Result<(), Self::Error>;

    /// Tell how much gas is left in running context.
    fn gas_available(&self) -> Result<u64, Self::Error>;

    /// Value associated with message.
    fn value(&self) -> Result<u128, Self::Error>;

    /// Tell how much value is left in running context.
    fn value_available(&self) -> Result<u128, Self::Error>;

    /// Interrupt the program and reschedule execution for maximum.
    fn wait(&mut self) -> Result<(), Self::Error>;

    /// Interrupt the program and reschedule execution in duration.
    fn wait_for(&mut self, duration: u32) -> Result<(), Self::Error>;

    /// Interrupt the program and reschedule execution for maximum,
    /// but not more than duration.
    fn wait_up_to(&mut self, duration: u32) -> Result<bool, Self::Error>;

    /// Wake the waiting message and move it to the processing queue.
    fn wake(&mut self, waker_id: MessageId, delay: u32) -> Result<(), Self::Error>;

    /// Send init message to create a new program.
    fn create_program(
        &mut self,
        packet: InitPacket,
        delay: u32,
    ) -> Result<(MessageId, ProgramId), Self::Error>;

    /// Create deposit to handle reply on given message.
    fn reply_deposit(&mut self, message_id: MessageId, amount: u64) -> Result<(), Self::Error>;

    /// Return the set of functions that are forbidden to be called.
    fn forbidden_funcs(&self) -> &BTreeSet<SysCallName>;
}
