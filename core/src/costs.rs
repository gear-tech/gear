// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use crate::gas::Token;

use codec::{Decode, Encode};

/// Describes the weight for each imported function that a program is allowed to call.
#[derive(Clone, Encode, Decode, PartialEq, Eq, Default)]
pub struct HostFnWeights {
    /// Weight of calling `alloc`.
    pub alloc: u64,

    /// Weight of calling `gr_reserve_gas`.
    pub gr_reserve_gas: u64,

    /// Weight of calling `gr_unreserve_gas`
    pub gr_unreserve_gas: u64,

    /// Weight of calling `gr_gas_available`.
    pub gr_gas_available: u64,

    /// Weight of calling `gr_msg_id`.
    pub gr_msg_id: u64,

    /// Weight of calling `gr_origin`.
    pub gr_origin: u64,

    /// Weight of calling `gr_program_id`.
    pub gr_program_id: u64,

    /// Weight of calling `gr_source`.
    pub gr_source: u64,

    /// Weight of calling `gr_value`.
    pub gr_value: u64,

    /// Weight of calling `gr_value_available`.
    pub gr_value_available: u64,

    /// Weight of calling `gr_size`.
    pub gr_size: u64,

    /// Weight of calling `gr_read`.
    pub gr_read: u64,

    /// Weight per payload byte by `gr_read`.
    pub gr_read_per_byte: u64,

    /// Weight of calling `gr_block_height`.
    pub gr_block_height: u64,

    /// Weight of calling `gr_block_timestamp`.
    pub gr_block_timestamp: u64,

    /// Weight of calling `gr_value_available`.
    pub gr_send_init: u64,

    /// Weight of calling `gr_send_push`.
    pub gr_send_push: u64,

    /// Weight per payload byte by `gr_send_push`.
    pub gr_send_push_per_byte: u64,

    /// Weight of calling `gr_send_commit`.
    pub gr_send_commit: u64,

    /// Weight per payload byte by `gr_send_commit`.
    pub gr_send_commit_per_byte: u64,

    /// Weight of calling `gr_reply_commit`.
    pub gr_reply_commit: u64,

    /// Weight per payload byte by `gr_reply_commit`.
    pub gr_reply_commit_per_byte: u64,

    /// Weight of calling `gr_reply_push`.
    pub gr_reply_push: u64,

    /// Weight per payload byte by `gr_reply_push`.
    pub gr_reply_push_per_byte: u64,

    /// Weight of calling `gr_reply_to`.
    pub gr_reply_to: u64,

    /// Weight of calling `gr_debug`.
    pub gr_debug: u64,

    /// Weight of calling `gr_exit_code`.
    pub gr_exit_code: u64,

    /// Weight of calling `gr_exit`.
    pub gr_exit: u64,

    /// Weight of calling `gr_leave`.
    pub gr_leave: u64,

    /// Weight of calling `gr_wait`.
    pub gr_wait: u64,

    /// Weight of calling `gr_wait_for`.
    pub gr_wait_for: u64,

    /// Weight of calling `gr_wait_no_more`.
    pub gr_wait_no_more: u64,

    /// Weight of calling `gr_wake`.
    pub gr_wake: u64,

    /// Weight of calling `gr_create_program_wgas`.
    pub gr_create_program_wgas: u64,

    /// Weight per payload byte by `gr_create_program_wgas`.
    pub gr_create_program_wgas_per_byte: u64,

    /// Weight of calling `gas`.
    pub gas: u64,
}

/// We need this access as a macro because sometimes hiding the lifetimes behind
/// a function won't work out.
#[macro_export]
macro_rules! charge_gas_token {
    ($ext:expr, $costs:expr) => {{
        let token = $costs.token(&$ext.context.host_fn_weights);
        (
            $ext.context.gas_counter.charge_token(token),
            $ext.context.gas_allowance_counter.charge_token(token),
        )
    }};
}

/// Token to consume gas amount.
#[derive(Copy, Clone)]
pub struct RuntimeToken {
    weight: u64,
}

impl Token for RuntimeToken {
    fn weight(&self) -> u64 {
        self.weight
    }
}

/// Enumerates syscalls that can be charged by gas meter.
#[derive(Copy, Clone)]
pub enum RuntimeCosts {
    /// Charge the gas meter with the cost of a metering block. The charged costs are
    /// the supplied cost of the block plus the overhead of the metering itself.
    MeteringBlock(u32),
    /// Weight of calling `alloc`.
    Alloc,
    /// Weight of calling `gr_reserve_gas`
    ReserveGas,
    /// Weight of calling `gr_unreserve_gas`
    UnreserveGas,
    /// Weight of calling `gr_gas_available`.
    GasAvailable,
    /// Weight of calling `gr_msg_id`.
    MsgId,
    /// Weight of calling `gr_origin`.
    Origin,
    /// Weight of calling `gr_program_id`.
    ProgramId,
    /// Weight of calling `gr_source`.
    Source,
    /// Weight of calling `gr_value`.
    Value,
    /// Weight of calling `gr_value_available`.
    ValueAvailable,
    /// Weight of calling `gr_size`.
    Size,
    /// Weight of calling `gr_read`.
    Read(u32),
    /// Weight of calling `gr_block_height`.
    BlockHeight,
    /// Weight of calling `gr_block_timestamp`.
    BlockTimestamp,
    /// Weight of calling `gr_value_available`.
    SendInit,
    /// Weight of calling `gr_send_push`.
    SendPush(u32),
    /// Weight of calling `gr_send_commit`.
    SendCommit(u32),
    /// Weight of calling `gr_reply_commit`.
    ReplyCommit(u32),
    /// Weight of calling `gr_reply_push`.
    ReplyPush(u32),
    /// Weight of calling `gr_reply_to`.
    ReplyTo,
    /// Weight of calling `gr_debug`.
    Debug,
    /// Weight of calling `gr_exit_code`.
    ExitCode,
    /// Weight of calling `gr_exit`.
    Exit,
    /// Weight of calling `gr_leave`.
    Leave,
    /// Weight of calling `gr_wait`.
    Wait,
    /// Weight of calling `gr_wait_for`.
    WaitFor,
    /// Weight of calling `gr_wait_no_more`.
    WaitNoMore,
    /// Weight of calling `gr_wake`.
    Wake,
    /// Weight of calling `gr_create_program_wgas`.
    CreateProgram(u32),
}

impl RuntimeCosts {
    /// Returns a token with a weight given the parameters from `HostFnWeights`.
    pub fn token(&self, s: &HostFnWeights) -> RuntimeToken {
        use self::RuntimeCosts::*;
        let weight = match *self {
            MeteringBlock(amount) => s.gas.saturating_add(amount.into()),
            Alloc => s.alloc,
            ReserveGas => s.gr_reserve_gas,
            UnreserveGas => s.gr_unreserve_gas,
            GasAvailable => s.gr_gas_available,
            MsgId => s.gr_msg_id,
            Origin => s.gr_origin,
            ProgramId => s.gr_program_id,
            Source => s.gr_source,
            Value => s.gr_value,
            ValueAvailable => s.gr_value_available,
            Size => s.gr_size,
            Read(len) => s
                .gr_read
                .saturating_add(s.gr_read_per_byte.saturating_mul(len.into())),
            BlockHeight => s.gr_block_height,
            BlockTimestamp => s.gr_block_timestamp,
            SendInit => s.gr_send_init,
            SendPush(len) => s
                .gr_send_push
                .saturating_add(s.gr_send_push_per_byte.saturating_mul(len.into())),
            SendCommit(len) => s
                .gr_send_commit
                .saturating_add(s.gr_send_commit_per_byte.saturating_mul(len.into())),
            ReplyCommit(len) => s
                .gr_reply_commit
                .saturating_add(s.gr_reply_commit_per_byte.saturating_mul(len.into())),
            ReplyPush(len) => s
                .gr_reply_push
                .saturating_add(s.gr_reply_push_per_byte.saturating_mul(len.into())),
            ReplyTo => s.gr_reply_to,
            Debug => s.gr_debug,
            ExitCode => s.gr_exit_code,
            Exit => s.gr_exit,
            Leave => s.gr_leave,
            Wait => s.gr_wait,
            WaitFor => s.gr_wait_for,
            WaitNoMore => s.gr_wait_no_more,
            Wake => s.gr_wake,
            CreateProgram(len) => s
                .gr_create_program_wgas
                .saturating_add(s.gr_create_program_wgas_per_byte.saturating_mul(len.into())),
        };
        RuntimeToken { weight }
    }
}
