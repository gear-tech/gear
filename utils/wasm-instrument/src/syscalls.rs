// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

//! Gear syscalls for smart contracts execution signatures.

use crate::parity_wasm::elements::{FunctionType, ValueType};
use alloc::{collections::BTreeSet, vec::Vec};
use enum_iterator::{self, Sequence};

/// All available sys calls.
///
/// The type is mainly used to prevent from skipping sys-call integration test for
/// a newly introduced sys-call or from typo in sys-call name.
#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Sequence, Hash)]
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
    SignalFrom,
    Size,
    ReplyCode,
    SignalCode,
    MessageId,
    ProgramId,
    Source,
    Value,

    // Program execution related
    // --
    // Execution environmental data
    BlockHeight,
    BlockTimestamp,
    CostMultiplier,
    GasAvailable,
    ValueAvailable,

    // Changing execution path calls
    Exit,
    Leave,
    Wait,
    WaitFor,
    WaitUpTo,
    Wake,
    Panic,
    OomPanic,

    // Hard under the hood calls, serving proper program execution
    Alloc,
    Free,
    OutOfGas,

    // Miscellaneous
    ReplyDeposit,
    Debug,
    Random,
    ReserveGas,
    UnreserveGas,
    SystemReserveGas,
    PayProgramRent,
}

impl SysCallName {
    pub fn to_str(&self) -> &'static str {
        match self {
            SysCallName::Alloc => "alloc",
            SysCallName::BlockHeight => "gr_block_height",
            SysCallName::BlockTimestamp => "gr_block_timestamp",
            SysCallName::CostMultiplier => "gr_cost_multiplier",
            SysCallName::CreateProgram => "gr_create_program",
            SysCallName::CreateProgramWGas => "gr_create_program_wgas",
            SysCallName::ReplyDeposit => "gr_reply_deposit",
            SysCallName::Debug => "gr_debug",
            SysCallName::Panic => "gr_panic",
            SysCallName::OomPanic => "gr_oom_panic",
            SysCallName::Exit => "gr_exit",
            SysCallName::Free => "free",
            SysCallName::GasAvailable => "gr_gas_available",
            SysCallName::Leave => "gr_leave",
            SysCallName::MessageId => "gr_message_id",
            SysCallName::OutOfGas => "gr_out_of_gas",
            SysCallName::PayProgramRent => "gr_pay_program_rent",
            SysCallName::ProgramId => "gr_program_id",
            SysCallName::Random => "gr_random",
            SysCallName::Read => "gr_read",
            SysCallName::Reply => "gr_reply",
            SysCallName::ReplyCommit => "gr_reply_commit",
            SysCallName::ReplyCommitWGas => "gr_reply_commit_wgas",
            SysCallName::ReplyPush => "gr_reply_push",
            SysCallName::ReplyTo => "gr_reply_to",
            SysCallName::SignalFrom => "gr_signal_from",
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
            SysCallName::ReplyCode => "gr_reply_code",
            SysCallName::SignalCode => "gr_signal_code",
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

    /// Returns list of all syscall names (actually supported by this module syscalls).
    pub fn instrumentable() -> BTreeSet<Self> {
        [
            Self::Alloc,
            Self::Free,
            Self::Debug,
            Self::Panic,
            Self::OomPanic,
            Self::BlockHeight,
            Self::BlockTimestamp,
            Self::Exit,
            Self::GasAvailable,
            Self::PayProgramRent,
            Self::ProgramId,
            Self::Leave,
            Self::ValueAvailable,
            Self::Wait,
            Self::WaitUpTo,
            Self::WaitFor,
            Self::Wake,
            Self::ReplyCode,
            Self::SignalCode,
            Self::MessageId,
            Self::Read,
            Self::Reply,
            Self::ReplyWGas,
            Self::ReplyInput,
            Self::ReplyInputWGas,
            Self::ReplyCommit,
            Self::ReplyCommitWGas,
            Self::ReservationReply,
            Self::ReservationReplyCommit,
            Self::ReplyPush,
            Self::ReplyPushInput,
            Self::ReplyTo,
            Self::SignalFrom,
            Self::Send,
            Self::SendWGas,
            Self::SendInput,
            Self::SendInputWGas,
            Self::SendCommit,
            Self::SendCommitWGas,
            Self::SendInit,
            Self::SendPush,
            Self::SendPushInput,
            Self::ReservationSend,
            Self::ReservationSendCommit,
            Self::Size,
            Self::Source,
            Self::Value,
            Self::CreateProgram,
            Self::CreateProgramWGas,
            Self::ReplyDeposit,
            Self::ReserveGas,
            Self::UnreserveGas,
            Self::Random,
        ]
        .into()
    }

    /// Returns signature for syscall by name.
    pub fn signature(self) -> SysCallSignature {
        use ParamType::*;
        use ValueType::I32;
        match self {
            Self::Alloc => SysCallSignature::system([Alloc], [I32]),
            Self::Free => SysCallSignature::system([Free], [I32]),
            Self::Debug => SysCallSignature::gr([Ptr(Some(1)), Size]),
            Self::Panic => SysCallSignature::gr([Ptr(Some(1)), Size]),
            Self::OomPanic => SysCallSignature::gr([]),
            Self::BlockHeight => SysCallSignature::gr([Ptr(None)]),
            Self::BlockTimestamp => SysCallSignature::gr([Ptr(None)]),
            Self::CostMultiplier => SysCallSignature::gr([Ptr(None)]),
            Self::Exit => SysCallSignature::gr([Ptr(None)]),
            Self::GasAvailable => SysCallSignature::gr([Ptr(None)]),
            Self::PayProgramRent => SysCallSignature::gr([Ptr(None), Ptr(None)]),
            Self::ProgramId => SysCallSignature::gr([Ptr(None)]),
            Self::Leave => SysCallSignature::gr([]),
            Self::ValueAvailable => SysCallSignature::gr([Ptr(None)]),
            Self::Wait => SysCallSignature::gr([]),
            Self::WaitUpTo => SysCallSignature::gr([Duration]),
            Self::WaitFor => SysCallSignature::gr([Duration]),
            Self::Wake => SysCallSignature::gr([Ptr(None), Delay, Ptr(None)]),
            Self::ReplyCode => SysCallSignature::gr([Ptr(None)]),
            Self::SignalCode => SysCallSignature::gr([Ptr(None)]),
            Self::MessageId => SysCallSignature::gr([Ptr(None)]),
            Self::Read => SysCallSignature::gr([MessagePosition, Size, Ptr(None), Ptr(None)]),
            Self::Reply => SysCallSignature::gr([Ptr(Some(1)), Size, Ptr(None), Ptr(None)]),
            Self::ReplyInput => SysCallSignature::gr([Size, Size, Ptr(None), Ptr(None)]),
            Self::ReplyWGas => {
                SysCallSignature::gr([Ptr(Some(1)), Size, Gas, Ptr(None), Ptr(None)])
            }
            Self::ReplyInputWGas => SysCallSignature::gr([Size, Size, Gas, Ptr(None), Ptr(None)]),
            Self::ReplyCommit => SysCallSignature::gr([Ptr(None), Ptr(None)]),
            Self::ReplyCommitWGas => SysCallSignature::gr([Gas, Ptr(None), Ptr(None)]),
            Self::ReservationReply => {
                SysCallSignature::gr([Ptr(None), Ptr(Some(2)), Size, Ptr(None)])
            }
            Self::ReservationReplyCommit => SysCallSignature::gr([Ptr(None), Ptr(None)]),
            Self::ReplyPush => SysCallSignature::gr([Ptr(Some(1)), Size, Ptr(None)]),
            Self::ReplyPushInput => SysCallSignature::gr([Size, Size, Ptr(None)]),
            Self::ReplyTo => SysCallSignature::gr([Ptr(None)]),
            Self::SignalFrom => SysCallSignature::gr([Ptr(None)]),
            Self::Send => SysCallSignature::gr([Ptr(None), Ptr(Some(2)), Size, Delay, Ptr(None)]),
            Self::SendInput => SysCallSignature::gr([Ptr(None), Size, Size, Delay, Ptr(None)]),
            Self::SendWGas => {
                SysCallSignature::gr([Ptr(None), Ptr(Some(2)), Size, Gas, Delay, Ptr(None)])
            }
            Self::SendInputWGas => {
                SysCallSignature::gr([Ptr(None), Size, Size, Gas, Delay, Ptr(None)])
            }
            Self::SendCommit => SysCallSignature::gr([Handler, Ptr(None), Delay, Ptr(None)]),
            Self::SendCommitWGas => {
                SysCallSignature::gr([Handler, Ptr(None), Gas, Delay, Ptr(None)])
            }
            Self::SendInit => SysCallSignature::gr([Ptr(None)]),
            Self::SendPush => SysCallSignature::gr([Handler, Ptr(Some(2)), Size, Ptr(None)]),
            Self::SendPushInput => SysCallSignature::gr([Handler, Size, Size, Ptr(None)]),
            Self::ReservationSend => {
                SysCallSignature::gr([Ptr(None), Ptr(Some(2)), Size, Delay, Ptr(None)])
            }
            Self::ReservationSendCommit => {
                SysCallSignature::gr([Handler, Ptr(None), Delay, Ptr(None)])
            }
            Self::Size => SysCallSignature::gr([Ptr(None)]),
            Self::Source => SysCallSignature::gr([Ptr(None)]),
            Self::Value => SysCallSignature::gr([Ptr(None)]),
            Self::CreateProgram => SysCallSignature::gr([
                Ptr(None),
                Ptr(Some(2)),
                Size,
                Ptr(Some(4)),
                Size,
                Delay,
                Ptr(None),
            ]),
            Self::CreateProgramWGas => SysCallSignature::gr([
                Ptr(None),
                Ptr(Some(2)),
                Size,
                Ptr(Some(4)),
                Size,
                Gas,
                Delay,
                Ptr(None),
            ]),
            Self::ReplyDeposit => SysCallSignature::gr([Ptr(None), Gas, Ptr(None)]),
            Self::ReserveGas => SysCallSignature::gr([Gas, Duration, Ptr(None)]),
            Self::UnreserveGas => SysCallSignature::gr([Ptr(None), Ptr(None)]),
            Self::SystemReserveGas => SysCallSignature::gr([Gas, Ptr(None)]),
            Self::Random => SysCallSignature::gr([Ptr(None), Ptr(None)]),
            other => panic!("Unknown syscall: '{:?}'", other),
        }
    }

    pub fn to_wgas(self) -> Option<Self> {
        Some(match self {
            Self::Reply => Self::ReplyWGas,
            Self::ReplyInput => Self::ReplyInputWGas,
            Self::ReplyCommit => Self::ReplyCommitWGas,
            Self::Send => Self::SendWGas,
            Self::SendInput => Self::SendInputWGas,
            Self::SendCommit => Self::SendCommitWGas,
            Self::CreateProgram => Self::CreateProgramWGas,
            _ => return None,
        })
    }
}

/// Syscall param type.
///
/// `Ptr` is usually used to point to the beginning of the array in memory.
/// In order to distinguish between pointer to the memory array and pointer
/// to some value, `Ptr` was defined as a tuple-like struct that owns an
/// optional index of the memory array size parameter. So if current sys-call
/// doesn't accept any memory array as an argument, then pointer parameter will
/// be `Ptr(None)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ParamType {
    Size,               // i32 buffers size in memory
    Ptr(Option<usize>), // i32 pointer
    Gas,                // i64 gas amount
    MessagePosition,    // i32 message position
    Duration,           // i32 duration in blocks
    Delay,              // i32 delay in blocks
    Handler,            // i32 handler number
    Alloc,              // i32 alloc pages
    Free,               // i32 free page
}

impl From<ParamType> for ValueType {
    fn from(value: ParamType) -> Self {
        match value {
            ParamType::Gas => ValueType::I64,
            _ => ValueType::I32,
        }
    }
}

/// Syscall signature.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SysCallSignature {
    pub params: Vec<ParamType>,
    pub results: Vec<ValueType>,
}

impl SysCallSignature {
    pub fn gr<const N: usize>(params: [ParamType; N]) -> Self {
        Self {
            params: params.to_vec(),
            results: Default::default(),
        }
    }

    pub fn system<const N: usize, const M: usize>(
        params: [ParamType; N],
        results: [ValueType; M],
    ) -> Self {
        Self {
            params: params.to_vec(),
            results: results.to_vec(),
        }
    }

    pub fn func_type(&self) -> FunctionType {
        FunctionType::new(
            self.params.iter().copied().map(Into::into).collect(),
            self.results.clone(),
        )
    }
}
