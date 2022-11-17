// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use enum_iterator::{self, Sequence};
use gear_wasm_instrument::{IMPORT_NAME_OUT_OF_ALLOWANCE, IMPORT_NAME_OUT_OF_GAS};

/// All available sys calls.
///
/// The type is mainly used to prevent from skipping sys-call integration test for a newly introduced sys-call.
#[derive(Debug, Clone, Copy, Sequence)]
pub enum SysCalls {
    // Message sending related
    // --
    // Sending `handle` message
    Send,
    SendWGas,
    SendCommit,
    SendCommitWGas,
    SendInit,
    SendPush,
    ReservationSend,
    ReservationSendCommit,

    // Sending `handle_reply` message
    Reply,
    ReplyWGas,
    ReplyCommit,
    ReplyCommitWGas,
    ReplyTo,
    ReplyPush,
    ReservationReply,
    ReservationReplyCommit,
    // Sending `init` message
    CreateProgram,
    CreateProgramWGas,

    // Message data related
    Read,
    Size,
    StatusCode,
    MessageId,
    ProgramId,
    Source,
    Value,

    // Program execution related
    // --
    // Execution environmental data
    BlockHeight,
    BlockTimestamp,
    Origin,
    GasAvailable,
    ValueAvailable,
    // Changing execution path calls
    Exit,
    Leave,
    Wait,
    WaitFor,
    WaitUpTo,
    Wake,

    // Hard under the hood calls, serving proper program execution
    Alloc,
    Free,
    OutOfGas,
    OutOfAllowance,

    // Miscellaneous
    Debug,
    Error,
    Random,
    ReserveGas,
    UnreserveGas,
    SystemReserveGas,
}

impl SysCalls {
    pub fn to_str(&self) -> &'static str {
        match self {
            SysCalls::Send => "gr_send",
            SysCalls::SendWGas => "gr_send_wgas",
            SysCalls::SendCommit => "gr_send_commit",
            SysCalls::SendCommitWGas => "gr_send_commit_wgas",
            SysCalls::SendInit => "gr_send_init",
            SysCalls::SendPush => "gr_send_push",
            SysCalls::Reply => "gr_reply",
            SysCalls::ReplyWGas => "gr_reply_wgas",
            SysCalls::ReplyCommit => "gr_reply_commit",
            SysCalls::ReplyCommitWGas => "gr_reply_commit_wgas",
            SysCalls::ReplyTo => "gr_reply_to",
            SysCalls::ReplyPush => "gr_reply_push",
            SysCalls::CreateProgram => "gr_create_program",
            SysCalls::CreateProgramWGas => "gr_create_program_wgas",
            SysCalls::Read => "gr_read",
            SysCalls::Size => "gr_size",
            SysCalls::StatusCode => "gr_status_code",
            SysCalls::MessageId => "gr_message_id",
            SysCalls::ProgramId => "gr_program_id",
            SysCalls::Source => "gr_source",
            SysCalls::Value => "gr_value",
            SysCalls::BlockHeight => "gr_block_height",
            SysCalls::BlockTimestamp => "gr_block_timestamp",
            SysCalls::Origin => "gr_origin",
            SysCalls::GasAvailable => "gr_gas_available",
            SysCalls::ValueAvailable => "gr_value_available",
            SysCalls::Exit => "gr_exit",
            SysCalls::Leave => "gr_leave",
            SysCalls::Wait => "gr_wait",
            SysCalls::WaitFor => "gr_wait_for",
            SysCalls::WaitUpTo => "gr_wait_up_to",
            SysCalls::Wake => "gr_wake",
            SysCalls::Alloc => "alloc",
            SysCalls::Free => "free",
            SysCalls::OutOfGas => IMPORT_NAME_OUT_OF_GAS,
            SysCalls::OutOfAllowance => IMPORT_NAME_OUT_OF_ALLOWANCE,
            SysCalls::Debug => "gr_debug",
            SysCalls::Error => "gr_error",
            SysCalls::Random => "gr_random",
            SysCalls::ReserveGas => "gr_reserve_gas",
            SysCalls::UnreserveGas => "gr_unreserve_gas",
            SysCalls::ReservationSend => "gr_reservation_send",
            SysCalls::ReservationSendCommit => "gr_reservation_send_commit",
            SysCalls::ReservationReply => "gr_reservation_reply",
            SysCalls::ReservationReplyCommit => "gr_reservation_reply_commit",
            SysCalls::SystemReserveGas => "gr_system_reserve_gas",
        }
    }

    pub fn all() -> impl Iterator<Item = Self> {
        enum_iterator::all()
    }
}
