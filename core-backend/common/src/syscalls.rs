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
pub enum SysCallName {
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
    SendInput,
    SendPushInput,
    SendInputWGas,

    // Sending `handle_reply` message
    Reply,
    ReplyWGas,
    ReplyCommit,
    ReplyCommitWGas,
    ReplyPush,
    ReservationReply,
    ReservationReplyCommit,
    ReplyInput,
    ReplyPushInput,
    ReplyInputWGas,

    // Sending `init` message
    CreateProgram,
    CreateProgramWGas,

    // Message data related
    Read,
    ReplyTo,
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

impl SysCallName {
    pub fn to_str(&self) -> &'static str {
        match self {
            SysCallName::Alloc => "alloc",
            SysCallName::BlockHeight => "gr_block_height",
            SysCallName::BlockTimestamp => "gr_block_timestamp",
            SysCallName::CreateProgram => "gr_create_program",
            SysCallName::CreateProgramWGas => "gr_create_program_wgas",
            SysCallName::Debug => "gr_debug",
            SysCallName::Error => "gr_error",
            SysCallName::Exit => "gr_exit",
            SysCallName::Free => "free",
            SysCallName::GasAvailable => "gr_gas_available",
            SysCallName::Leave => "gr_leave",
            SysCallName::MessageId => "gr_message_id",
            SysCallName::Origin => "gr_origin",
            SysCallName::OutOfAllowance => IMPORT_NAME_OUT_OF_ALLOWANCE,
            SysCallName::OutOfGas => IMPORT_NAME_OUT_OF_GAS,
            SysCallName::ProgramId => "gr_program_id",
            SysCallName::Random => "gr_random",
            SysCallName::Read => "gr_read",
            SysCallName::Reply => "gr_reply",
            SysCallName::ReplyCommit => "gr_reply_commit",
            SysCallName::ReplyCommitWGas => "gr_reply_commit_wgas",
            SysCallName::ReplyPush => "gr_reply_push",
            SysCallName::ReplyTo => "gr_reply_to",
            SysCallName::ReplyWGas => "gr_reply_wgas",
            SysCallName::ReplyInput => "gr_reply_input",
            SysCallName::ReplyPushInput => "gr_reply_push_input",
            SysCallName::ReplyInputWGas => "gr_reply_input_wgas",
            SysCallName::ReservationReply => "gr_reservation_reply",
            SysCallName::ReservationReplyCommit => "gr_reservation_reply_commit",
            SysCallName::ReservationSend => "gr_reservation_send",
            SysCallName::ReservationSendCommit => "gr_reservation_send_commit",
            SysCallName::ReserveGas => "gr_reserve_gas",
            SysCallName::Send => "gr_send",
            SysCallName::SendCommit => "gr_send_commit",
            SysCallName::SendCommitWGas => "gr_send_commit_wgas",
            SysCallName::SendInit => "gr_send_init",
            SysCallName::SendPush => "gr_send_push",
            SysCallName::SendWGas => "gr_send_wgas",
            SysCallName::SendInput => "gr_send_input",
            SysCallName::SendPushInput => "gr_send_push_input",
            SysCallName::SendInputWGas => "gr_send_input_wgas",
            SysCallName::Size => "gr_size",
            SysCallName::Source => "gr_source",
            SysCallName::StatusCode => "gr_status_code",
            SysCallName::SystemReserveGas => "gr_system_reserve_gas",
            SysCallName::UnreserveGas => "gr_unreserve_gas",
            SysCallName::Value => "gr_value",
            SysCallName::ValueAvailable => "gr_value_available",
            SysCallName::Wait => "gr_wait",
            SysCallName::WaitFor => "gr_wait_for",
            SysCallName::WaitUpTo => "gr_wait_up_to",
            SysCallName::Wake => "gr_wake",
        }
    }

    pub fn all() -> impl Iterator<Item = Self> {
        enum_iterator::all()
    }

    pub fn count() -> usize {
        Self::all().count()
    }
}
