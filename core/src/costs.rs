// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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

//! Costs module.

use core::{fmt::Debug, marker::PhantomData};
use scale_info::scale::{Decode, Encode};

/// +_+_+
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct CostPer<T> {
    cost: u64,
    _phantom: PhantomData<T>,
}

impl<T> CostPer<T> {
    /// Const constructor
    pub const fn new(cost: u64) -> Self {
        Self {
            cost,
            _phantom: PhantomData,
        }
    }

    /// Cost for one.
    pub const fn one(&self) -> u64 {
        self.cost
    }

    /// Returns another [`CostPer`] with increased `cost` to `other.cost`.
    pub const fn saturating_add(&self, other: Self) -> Self {
        Self::new(self.cost.saturating_add(other.cost))
    }
}

impl<T: Into<u32>> CostPer<T> {
    /// Calculate cost for `num` amount of `T`.
    pub fn calc(&self, num: T) -> u64 {
        self.cost.saturating_mul(Into::<u32>::into(num).into())
    }
}

impl<T> Debug for CostPer<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("{}", &self.cost))
    }
}

impl<T> From<u64> for CostPer<T> {
    fn from(cost: u64) -> Self {
        CostPer::new(cost)
    }
}

impl<T> From<CostPer<T>> for u64 {
    fn from(value: CostPer<T>) -> Self {
        value.cost
    }
}

impl<T> Default for CostPer<T> {
    fn default() -> Self {
        CostPer::new(0)
    }
}

/// +_+_+
#[derive(Debug, Default, Clone, Copy)]
pub struct Call;

/// +_+_+
#[derive(Debug, Default, Clone, Copy, derive_more::From, derive_more::Into)]
pub struct Bytes(u32);

// +_+_+ comments
/// Enumerates syscalls that can be charged by gas meter.
#[derive(Debug, Copy, Clone)]
pub enum CostToken {
    /// Charge zero gas
    Null,
    /// Charge for calling `alloc`.
    Alloc,
    /// Charge for calling `free`.
    Free,
    /// Charge for calling `free_range`
    FreeRange,
    /// Charge for calling `gr_reserve_gas`.
    ReserveGas,
    /// Charge for calling `gr_unreserve_gas`.
    UnreserveGas,
    /// Charge for calling `gr_system_reserve_gas`.
    SystemReserveGas,
    /// Charge for calling `gr_gas_available`.
    GasAvailable,
    /// Charge for calling `gr_message_id`.
    MsgId,
    /// Charge for calling `gr_program_id`.
    ProgramId,
    /// Charge for calling `gr_source`.
    Source,
    /// Charge for calling `gr_value`.
    Value,
    /// Charge for calling `gr_value_available`.
    ValueAvailable,
    /// Charge for calling `gr_size`.
    Size,
    /// Charge for calling `gr_read`.
    Read,
    /// Charge for calling `gr_env_vars`.
    EnvVars,
    /// Charge for calling `gr_block_height`.
    BlockHeight,
    /// Charge for calling `gr_block_timestamp`.
    BlockTimestamp,
    /// Charge for calling `gr_random`.
    Random,
    /// Charge for calling `gr_reply_deposit`.
    ReplyDeposit,
    /// Charge for calling `gr_send`.
    Send(Bytes),
    /// Charge for calling `gr_send_wgas`.
    SendWGas(Bytes),
    /// Charge for calling `gr_send_init`.
    SendInit,
    /// Charge for calling `gr_send_push`.
    SendPush(Bytes),
    /// Charge for calling `gr_send_commit`.
    SendCommit,
    /// Charge for calling `gr_send_commit_wgas`.
    SendCommitWGas,
    /// Charge for calling `gr_reservation_send`.
    ReservationSend(Bytes),
    /// Charge for calling `gr_reservation_send_commit`.
    ReservationSendCommit,
    /// Charge for calling `gr_send_input`.
    SendInput,
    /// Charge for calling `gr_send_input_wgas`.
    SendInputWGas,
    /// Charge for calling `gr_send_push_input`.
    SendPushInput,
    /// Charge for calling `gr_reply`.
    Reply(Bytes),
    /// Charge for calling `gr_reply_wgas`.
    ReplyWGas(Bytes),
    /// Charge for calling `gr_reply_push`.
    ReplyPush(Bytes),
    /// Charge for calling `gr_reply_commit`.
    ReplyCommit,
    /// Charge for calling `gr_reply_commit_wgas`.
    ReplyCommitWGas,
    /// Charge for calling `gr_reservation_reply`.
    ReservationReply(Bytes),
    /// Charge for calling `gr_reservation_reply_commit`.
    ReservationReplyCommit,
    /// Charge for calling `gr_reply_input`.
    ReplyInput,
    /// Charge for calling `gr_reply_input_wgas`.
    ReplyInputWGas,
    /// Charge for calling `gr_reply_push_input`.
    ReplyPushInput,
    /// Charge for calling `gr_reply_to`.
    ReplyTo,
    /// Charge for calling `gr_signal_code`.
    SignalCode,
    /// Charge for calling `gr_signal_from`.
    SignalFrom,
    /// Charge for calling `gr_debug`.
    Debug(Bytes),
    /// Charge for calling `gr_reply_code`.
    ReplyCode,
    /// Charge for calling `gr_exit`.
    Exit,
    /// Charge for calling `gr_leave`.
    Leave,
    /// Charge for calling `gr_wait`.
    Wait,
    /// Charge for calling `gr_wait_for`.
    WaitFor,
    /// Charge for calling `gr_wait_up_to`.
    WaitUpTo,
    /// Charge for calling `gr_wake`.
    Wake,
    /// Charge for calling `gr_create_program`.
    CreateProgram(Bytes, Bytes),
    /// Charge for calling `gr_create_program_wgas`.
    CreateProgramWGas(Bytes, Bytes),
}
