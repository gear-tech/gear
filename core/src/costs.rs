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

    /// Weight of calling `alloc`.
    pub free: u64,

    /// Weight of calling `gr_reserve_gas`.
    pub gr_reserve_gas: u64,

    /// Weight of calling `gr_unreserve_gas`
    pub gr_unreserve_gas: u64,

    /// Weight of calling `gr_system_reserve_gas`
    pub gr_system_reserve_gas: u64,

    /// Weight of calling `gr_gas_available`.
    pub gr_gas_available: u64,

    /// Weight of calling `gr_message_id`.
    pub gr_message_id: u64,

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

    /// Weight of calling `gr_random`.
    pub gr_random: u64,

    /// Weight of calling `gr_send_init`.
    pub gr_send_init: u64,

    /// Weight of calling `gr_send_push`.
    pub gr_send_push: u64,

    /// Weight per payload byte by `gr_send_push`.
    pub gr_send_push_per_byte: u64,

    /// Weight of calling `gr_send_commit`.
    pub gr_send_commit: u64,

    /// Weight per payload byte by `gr_send_commit`.
    pub gr_send_commit_per_byte: u64,

    /// Weight of calling `gr_reservation_send_commit`.
    pub gr_reservation_send_commit: u64,

    /// Weight per payload byte by `gr_reservation_send_commit`.
    pub gr_reservation_send_commit_per_byte: u64,

    /// Weight of calling `gr_reply_commit`.
    pub gr_reply_commit: u64,

    /// Weight of calling `gr_reservation_reply_commit`.
    pub gr_reservation_reply_commit: u64,

    /// Weight of calling `gr_reply_push`.
    pub gr_reply_push: u64,

    /// Weight per payload byte by `gr_reply_push`.
    pub gr_reply_push_per_byte: u64,

    /// Weight of calling `gr_reply_to`.
    pub gr_reply_to: u64,

    /// Weight of calling `gr_reply_push_input`.
    pub gr_reply_push_input: u64,

    /// Weight per payload byte by `gr_reply_push_input`.
    pub gr_reply_push_input_per_byte: u64,

    /// Weight of calling `gr_send_push_input`.
    pub gr_send_push_input: u64,

    /// Weight per payload byte by `gr_send_push_input`.
    pub gr_send_push_input_per_byte: u64,

    /// Weight of calling `gr_debug`.
    pub gr_debug: u64,

    /// Weight per payload byte by `gr_debug`.
    pub gr_debug_per_byte: u64,

    /// Weight of calling `gr_error`.
    pub gr_error: u64,

    /// Weight of calling `gr_status_code`.
    pub gr_status_code: u64,

    /// Weight of calling `gr_exit`.
    pub gr_exit: u64,

    /// Weight of calling `gr_leave`.
    pub gr_leave: u64,

    /// Weight of calling `gr_wait`.
    pub gr_wait: u64,

    /// Weight of calling `gr_wait_for`.
    pub gr_wait_for: u64,

    /// Weight of calling `gr_wait_up_to`.
    pub gr_wait_up_to: u64,

    /// Weight of calling `gr_wake`.
    pub gr_wake: u64,

    /// Weight of calling `gr_create_program_wgas`.
    pub gr_create_program_wgas: u64,

    /// Weight per payload byte by `gr_create_program_wgas`.
    pub gr_create_program_wgas_payload_per_byte: u64,

    /// Weight per salt byte by `gr_create_program_wgas`.
    pub gr_create_program_wgas_salt_per_byte: u64,

    /// Weight per one gear page read.
    pub lazy_pages_read: u64,

    /// Weight per one gear page write.
    pub lazy_pages_write: u64,

    /// Weight per one write, which is after page read.
    pub lazy_pages_write_after_read: u64,

    /// Weight per one gear page update in storage.
    pub lazy_pages_update: u64,
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
    /// Weight of calling `alloc`.
    Alloc,
    /// Weight of calling `free`.
    Free,
    /// Weight of calling `gr_reserve_gas`.
    ReserveGas,
    /// Weight of calling `gr_unreserve_gas`.
    UnreserveGas,
    /// Weight of calling `gr_system_reserve_gas`.
    SystemReserveGas,
    /// Weight of calling `gr_gas_available`.
    GasAvailable,
    /// Weight of calling `gr_message_id`.
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
    /// Weight of calling `gr_random`.
    Random,
    /// Weight of calling `gr_send_init`.
    SendInit,
    /// Weight of calling `gr_send_push`.
    SendPush(u32),
    /// Weight of calling `gr_send_commit`.
    SendCommit(u32),
    /// Weight of calling `gr_reservation_send_commit`.
    ReservationSendCommit(u32),
    /// Weight of calling `gr_reply_commit`.
    ReplyCommit,
    /// Weight of calling `gr_reservation_reply_commit`.
    ReservationReplyCommit,
    /// Weight of calling `gr_reply_push`.
    ReplyPush(u32),
    /// Weight of calling `gr_reply_to`.
    ReplyTo,
    /// Weight of calling `gr_debug`.
    Debug(u32),
    /// Weight of calling `gr_error`.
    Error,
    /// Weight of calling `gr_status_code`.
    StatusCode,
    /// Weight of calling `gr_exit`.
    Exit,
    /// Weight of calling `gr_leave`.
    Leave,
    /// Weight of calling `gr_wait`.
    Wait,
    /// Weight of calling `gr_wait_for`.
    WaitFor,
    /// Weight of calling `gr_wait_up_to`.
    WaitUpTo,
    /// Weight of calling `gr_wake`.
    Wake,
    /// Weight of calling `gr_create_program_wgas`.
    CreateProgram(u32, u32),
    /// Weight of calling `gr_resend_push`.
    SendPushInput(u32),
    /// Weight of calling `gr_rereply_push`.
    ReplyPushInput(u32),
    /// Weight of read access per one gear page.
    LazyPagesRead,
    /// Weight of write access per one gear page.
    LazyPagesWrite,
    /// Weight of write after read access per one gear page.
    LazyPagesWriteAfterRead,
    /// Weight of page update in storage after modification.
    LazyPagesUpdate,
}

impl RuntimeCosts {
    /// Returns a token with a weight given the parameters from `HostFnWeights`.
    pub fn token(&self, s: &HostFnWeights) -> RuntimeToken {
        use self::RuntimeCosts::*;
        let weight = match *self {
            Alloc => s.alloc,
            Free => s.free,
            ReserveGas => s.gr_reserve_gas,
            UnreserveGas => s.gr_unreserve_gas,
            SystemReserveGas => s.gr_system_reserve_gas,
            GasAvailable => s.gr_gas_available,
            MsgId => s.gr_message_id,
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
            Random => s.gr_random,
            SendInit => s.gr_send_init,
            SendPush(len) => s
                .gr_send_push
                .saturating_add(s.gr_send_push_per_byte.saturating_mul(len.into())),
            SendCommit(len) => s
                .gr_send_commit
                .saturating_add(s.gr_send_commit_per_byte.saturating_mul(len.into())),
            ReservationSendCommit(len) => s.gr_reservation_send_commit.saturating_add(
                s.gr_reservation_send_commit_per_byte
                    .saturating_mul(len.into()),
            ),
            ReplyCommit => s.gr_reply_commit,
            ReservationReplyCommit => s.gr_reservation_reply_commit,
            ReplyPush(len) => s
                .gr_reply_push
                .saturating_add(s.gr_reply_push_per_byte.saturating_mul(len.into())),
            ReplyTo => s.gr_reply_to,
            Debug(len) => s
                .gr_debug
                .saturating_add(s.gr_debug_per_byte.saturating_mul(len.into())),
            Error => s.gr_error,
            StatusCode => s.gr_status_code,
            Exit => s.gr_exit,
            Leave => s.gr_leave,
            Wait => s.gr_wait,
            WaitFor => s.gr_wait_for,
            WaitUpTo => s.gr_wait_up_to,
            Wake => s.gr_wake,
            CreateProgram(payload_len, salt_len) => s
                .gr_create_program_wgas
                .saturating_add(
                    s.gr_create_program_wgas_payload_per_byte
                        .saturating_mul(payload_len.into()),
                )
                .saturating_add(
                    s.gr_create_program_wgas_salt_per_byte
                        .saturating_mul(salt_len.into()),
                ),
            SendPushInput(len) => s
                .gr_send_push_input
                .saturating_add(s.gr_send_push_input_per_byte.saturating_mul(len.into())),
            ReplyPushInput(len) => s
                .gr_reply_push_input
                .saturating_add(s.gr_reply_push_input_per_byte.saturating_mul(len.into())),
            LazyPagesRead => s.lazy_pages_read,
            LazyPagesWrite => s.lazy_pages_write,
            LazyPagesWriteAfterRead => s.lazy_pages_write_after_read,
            LazyPagesUpdate => s.lazy_pages_update,
        };
        RuntimeToken { weight }
    }
}
