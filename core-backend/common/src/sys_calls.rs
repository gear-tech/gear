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

#[derive(Debug, Clone, Copy, Sequence)]
pub enum SysCallNames {
    // Message sending related
    // --
    // Sending `handle` message
    Send,
    SendWGas,
    SendCommit,
    SendCommitWGas,
    SendInit,
    SendPush,
    // Sending `handle_reply` message
    Reply,
    ReplyWGas,
    ReplyCommit,
    ReplyCommitWGas,
    ReplyTo,
    ReplyPush,
    // Sending `init` message
    CreateProgram,
    CreateProgramWGas,

    // Message data related
    Read,
    Size,
    ExitCode,
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
}

impl SysCallNames {
    pub fn to_str(&self) -> &'static str {
        match self {
            SysCallNames::Send => "gr_send",
            SysCallNames::SendWGas => "gr_send_wgas",
            SysCallNames::SendCommit => "gr_send_commit",
            SysCallNames::SendCommitWGas => "gr_send_commit_wgas",
            SysCallNames::SendInit => "gr_send_init",
            SysCallNames::SendPush => "gr_send_push",
            SysCallNames::Reply => "gr_reply",
            SysCallNames::ReplyWGas => "gr_reply_wgas",
            SysCallNames::ReplyCommit => "gr_reply_commit",
            SysCallNames::ReplyCommitWGas => "gr_reply_commit_wgas",
            SysCallNames::ReplyTo => "gr_reply_to",
            SysCallNames::ReplyPush => "gr_reply_push",
            SysCallNames::CreateProgram => "gr_create_program",
            SysCallNames::CreateProgramWGas => "gr_create_program_wgas",
            SysCallNames::Read => "gr_read",
            SysCallNames::Size => "gr_size",
            SysCallNames::ExitCode => "gr_exit_code",
            SysCallNames::MessageId => "gr_message_id",
            SysCallNames::ProgramId => "gr_program_id",
            SysCallNames::Source => "gr_source",
            SysCallNames::Value => "gr_value",
            SysCallNames::BlockHeight => "gr_block_height",
            SysCallNames::BlockTimestamp => "gr_block_timestamp",
            SysCallNames::Origin => "gr_origin",
            SysCallNames::GasAvailable => "gr_gas_available",
            SysCallNames::ValueAvailable => "gr_value_available",
            SysCallNames::Exit => "gr_exit",
            SysCallNames::Leave => "gr_leave",
            SysCallNames::Wait => "gr_wait",
            SysCallNames::WaitFor => "gr_wait_for",
            SysCallNames::WaitUpTo => "gr_wait_up_to",
            SysCallNames::Wake => "gr_wake",
            SysCallNames::Alloc => "alloc",
            SysCallNames::Free => "free",
            SysCallNames::OutOfGas => IMPORT_NAME_OUT_OF_GAS,
            SysCallNames::OutOfAllowance => IMPORT_NAME_OUT_OF_ALLOWANCE,
            SysCallNames::Debug => "gr_debug",
            SysCallNames::Error => "gr_error",
            SysCallNames::Random => "gr_random",
            SysCallNames::ReserveGas => "gr_reserve_gas",
            SysCallNames::UnreserveGas => "gr_unreserve_gas",
        }
    }

    pub fn all() -> impl Iterator<Item = Self> {
        enum_iterator::all()
    }
}
