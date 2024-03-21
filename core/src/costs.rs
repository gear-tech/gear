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

use crate::pages::WasmPage;
use core::{fmt::Debug, marker::PhantomData};
use paste::paste;

/// Gas cost per some type of action or data size.
#[derive(Clone, Copy, PartialEq, Eq)]
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

/// Some actions or calls amount.
#[derive(Debug, Default, Clone, Copy, derive_more::From, derive_more::Into)]
pub struct Calls(u32);

/// Bytes amount.
#[derive(Debug, Default, Clone, Copy, derive_more::From, derive_more::Into)]
pub struct Bytes(u32);

/// Chain blocks amount.
#[derive(Debug, Default, Clone, Copy, derive_more::From, derive_more::Into)]
pub struct Blocks(u32);

/// Program imported function call (syscall) costs.
#[derive(Debug, Clone, Default)]
pub struct SyscallCosts {
    /// Cost of calling `alloc`.
    pub alloc: CostPer<Calls>,

    /// Cost per allocated page for `alloc`.
    pub alloc_per_page: CostPer<WasmPage>,

    /// Cost of calling `free`.
    pub free: CostPer<Calls>,

    /// Cost of calling `free_range`
    pub free_range: CostPer<Calls>,

    /// Cost of calling `free_range` per page
    pub free_range_per_page: CostPer<WasmPage>,

    /// Cost of calling `gr_reserve_gas`.
    pub gr_reserve_gas: CostPer<Calls>,

    /// Cost of calling `gr_unreserve_gas`
    pub gr_unreserve_gas: CostPer<Calls>,

    /// Cost of calling `gr_system_reserve_gas`
    pub gr_system_reserve_gas: CostPer<Calls>,

    /// Cost of calling `gr_gas_available`.
    pub gr_gas_available: CostPer<Calls>,

    /// Cost of calling `gr_message_id`.
    pub gr_message_id: CostPer<Calls>,

    /// Cost of calling `gr_program_id`.
    pub gr_program_id: CostPer<Calls>,

    /// Cost of calling `gr_source`.
    pub gr_source: CostPer<Calls>,

    /// Cost of calling `gr_value`.
    pub gr_value: CostPer<Calls>,

    /// Cost of calling `gr_value_available`.
    pub gr_value_available: CostPer<Calls>,

    /// Cost of calling `gr_size`.
    pub gr_size: CostPer<Calls>,

    /// Cost of calling `gr_read`.
    pub gr_read: CostPer<Calls>,

    /// Cost per payload byte for `gr_read`.
    pub gr_read_per_byte: CostPer<Bytes>,

    /// Cost of calling `gr_env_vars`.
    pub gr_env_vars: CostPer<Calls>,

    /// Cost of calling `gr_block_height`.
    pub gr_block_height: CostPer<Calls>,

    /// Cost of calling `gr_block_timestamp`.
    pub gr_block_timestamp: CostPer<Calls>,

    /// Cost of calling `gr_random`.
    pub gr_random: CostPer<Calls>,

    /// Cost of calling `gr_reply_deposit`.
    pub gr_reply_deposit: CostPer<Calls>,

    /// Cost of calling `gr_send`
    pub gr_send: CostPer<Calls>,

    /// Cost per bytes for `gr_send`.
    pub gr_send_per_byte: CostPer<Bytes>,

    /// Cost of calling `gr_send_wgas`.
    pub gr_send_wgas: CostPer<Calls>,

    /// Cost of calling `gr_send_wgas` per one payload byte.
    pub gr_send_wgas_per_byte: CostPer<Bytes>,

    /// Cost of calling `gr_send_init`.
    pub gr_send_init: CostPer<Calls>,

    /// Cost of calling `gr_send_push`.
    pub gr_send_push: CostPer<Calls>,

    /// Cost per payload byte by `gr_send_push`.
    pub gr_send_push_per_byte: CostPer<Bytes>,

    /// Cost of calling `gr_send_commit`.
    pub gr_send_commit: CostPer<Calls>,

    /// Cost of calling `gr_send_commit_wgas`.
    pub gr_send_commit_wgas: CostPer<Calls>,

    /// Cost of calling `gr_reservation_send`.
    pub gr_reservation_send: CostPer<Calls>,

    /// Cost of calling `gr_reservation_send` per one payload byte.
    pub gr_reservation_send_per_byte: CostPer<Bytes>,

    /// Cost of calling `gr_reservation_send_commit`.
    pub gr_reservation_send_commit: CostPer<Calls>,

    /// Cost of calling `gr_send_init`.
    pub gr_send_input: CostPer<Calls>,

    /// Cost of calling `gr_send_init_wgas`.
    pub gr_send_input_wgas: CostPer<Calls>,

    /// Cost of calling `gr_send_push_input`.
    pub gr_send_push_input: CostPer<Calls>,

    /// Cost per payload byte by `gr_send_push_input`.
    pub gr_send_push_input_per_byte: CostPer<Bytes>,

    /// Cost of calling `gr_reply`.
    pub gr_reply: CostPer<Calls>,

    /// Cost of calling `gr_reply` per one payload byte.
    pub gr_reply_per_byte: CostPer<Bytes>,

    /// Cost of calling `gr_reply_wgas`.
    pub gr_reply_wgas: CostPer<Calls>,

    /// Cost of calling `gr_reply_wgas` per one payload byte.
    pub gr_reply_wgas_per_byte: CostPer<Bytes>,

    /// Cost of calling `gr_reply_commit`.
    pub gr_reply_commit: CostPer<Calls>,

    /// Cost of calling `gr_reply_commit_wgas`.
    pub gr_reply_commit_wgas: CostPer<Calls>,

    /// Cost of calling `gr_reservation_reply`.
    pub gr_reservation_reply: CostPer<Calls>,

    /// Cost of calling `gr_reservation_reply` per one payload byte.
    pub gr_reservation_reply_per_byte: CostPer<Bytes>,

    /// Cost of calling `gr_reservation_reply_commit`.
    pub gr_reservation_reply_commit: CostPer<Calls>,

    /// Cost of calling `gr_reply_push`.
    pub gr_reply_push: CostPer<Calls>,

    /// Cost per payload byte by `gr_reply_push`.
    pub gr_reply_push_per_byte: CostPer<Bytes>,

    /// Cost of calling `gr_reply_input`.
    pub gr_reply_input: CostPer<Calls>,

    /// Cost of calling `gr_reply_input_wgas`.
    pub gr_reply_input_wgas: CostPer<Calls>,

    /// Cost of calling `gr_reply_push_input`.
    pub gr_reply_push_input: CostPer<Calls>,

    /// Cost per payload byte by `gr_reply_push_input`.
    pub gr_reply_push_input_per_byte: CostPer<Bytes>,

    /// Cost of calling `gr_reply_to`.
    pub gr_reply_to: CostPer<Calls>,

    /// Cost of calling `gr_signal_code`.
    pub gr_signal_code: CostPer<Calls>,

    /// Cost of calling `gr_signal_from`.
    pub gr_signal_from: CostPer<Calls>,

    /// Cost of calling `gr_debug`.
    pub gr_debug: CostPer<Calls>,

    /// Cost per payload byte by `gr_debug`.
    pub gr_debug_per_byte: CostPer<Bytes>,

    /// Cost of calling `gr_reply_code`.
    pub gr_reply_code: CostPer<Calls>,

    /// Cost of calling `gr_exit`.
    pub gr_exit: CostPer<Calls>,

    /// Cost of calling `gr_leave`.
    pub gr_leave: CostPer<Calls>,

    /// Cost of calling `gr_wait`.
    pub gr_wait: CostPer<Calls>,

    /// Cost of calling `gr_wait_for`.
    pub gr_wait_for: CostPer<Calls>,

    /// Cost of calling `gr_wait_up_to`.
    pub gr_wait_up_to: CostPer<Calls>,

    /// Cost of calling `gr_wake`.
    pub gr_wake: CostPer<Calls>,

    /// Cost of calling `gr_create_program_wgas`.
    pub gr_create_program: CostPer<Calls>,

    /// Cost per payload byte by `gr_create_program_wgas`.
    pub gr_create_program_payload_per_byte: CostPer<Bytes>,

    /// Cost per salt byte by `gr_create_program_wgas`.
    pub gr_create_program_salt_per_byte: CostPer<Bytes>,

    /// Cost of calling `gr_create_program_wgas`.
    pub gr_create_program_wgas: CostPer<Calls>,

    /// Cost per payload byte by `gr_create_program_wgas`.
    pub gr_create_program_wgas_payload_per_byte: CostPer<Bytes>,

    /// Cost per salt byte by `gr_create_program_wgas`.
    pub gr_create_program_wgas_salt_per_byte: CostPer<Bytes>,
}

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
    /// Charge for calling `gr_send`, taking in account payload size.
    Send(Bytes),
    /// Charge for calling `gr_send_wgas`, taking in account payload size.
    SendWGas(Bytes),
    /// Charge for calling `gr_send_init`.
    SendInit,
    /// Charge for calling `gr_send_push`, taking in account payload size.
    SendPush(Bytes),
    /// Charge for calling `gr_send_commit`.
    SendCommit,
    /// Charge for calling `gr_send_commit_wgas`.
    SendCommitWGas,
    /// Charge for calling `gr_reservation_send`, taking in account payload size.
    ReservationSend(Bytes),
    /// Charge for calling `gr_reservation_send_commit`.
    ReservationSendCommit,
    /// Charge for calling `gr_send_input`.
    SendInput,
    /// Charge for calling `gr_send_input_wgas`.
    SendInputWGas,
    /// Charge for calling `gr_send_push_input`.
    SendPushInput,
    /// Charge for calling `gr_reply`, taking in account payload size.
    Reply(Bytes),
    /// Charge for calling `gr_reply_wgas`, taking in account payload size.
    ReplyWGas(Bytes),
    /// Charge for calling `gr_reply_push`, taking in account payload size.
    ReplyPush(Bytes),
    /// Charge for calling `gr_reply_commit`.
    ReplyCommit,
    /// Charge for calling `gr_reply_commit_wgas`.
    ReplyCommitWGas,
    /// Charge for calling `gr_reservation_reply`, taking in account payload size.
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
    /// Charge for calling `gr_debug`, taking in account payload size.
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
    /// Charge for calling `gr_create_program`, taking in account payload and salt size.
    CreateProgram(Bytes, Bytes),
    /// Charge for calling `gr_create_program_wgas`, taking in account payload and salt size.
    CreateProgramWGas(Bytes, Bytes),
}

impl SyscallCosts {
    /// Get cost for a token.
    pub fn cost_for_token(&self, token: CostToken) -> u64 {
        use CostToken::*;

        macro_rules! cost_with_per_byte {
            ($name:ident, $len:expr) => {
                paste! {
                    self.$name.one().saturating_add(self.[< $name _per_byte >].calc($len))
                }
            };
        }

        match token {
            Null => 0,
            Alloc => self.alloc.one(),
            Free => self.free.one(),
            FreeRange => self.free_range.one(),
            ReserveGas => self.gr_reserve_gas.one(),
            UnreserveGas => self.gr_unreserve_gas.one(),
            SystemReserveGas => self.gr_system_reserve_gas.one(),
            GasAvailable => self.gr_gas_available.one(),
            MsgId => self.gr_message_id.one(),
            ProgramId => self.gr_program_id.one(),
            Source => self.gr_source.one(),
            Value => self.gr_value.one(),
            ValueAvailable => self.gr_value_available.one(),
            Size => self.gr_size.one(),
            Read => self.gr_read.one(),
            EnvVars => self.gr_env_vars.one(),
            BlockHeight => self.gr_block_height.one(),
            BlockTimestamp => self.gr_block_timestamp.one(),
            Random => self.gr_random.one(),
            ReplyDeposit => self.gr_reply_deposit.one(),
            Send(len) => cost_with_per_byte!(gr_send, len),
            SendWGas(len) => cost_with_per_byte!(gr_send_wgas, len),
            SendInit => self.gr_send_init.one(),
            SendPush(len) => cost_with_per_byte!(gr_send_push, len),
            SendCommit => self.gr_send_commit.one(),
            SendCommitWGas => self.gr_send_commit_wgas.one(),
            ReservationSend(len) => cost_with_per_byte!(gr_reservation_send, len),
            ReservationSendCommit => self.gr_reservation_send_commit.one(),
            SendInput => self.gr_send_input.one(),
            SendInputWGas => self.gr_send_input_wgas.one(),
            SendPushInput => self.gr_send_push_input.one(),
            Reply(len) => cost_with_per_byte!(gr_reply, len),
            ReplyWGas(len) => cost_with_per_byte!(gr_reply_wgas, len),
            ReplyPush(len) => cost_with_per_byte!(gr_reply_push, len),
            ReplyCommit => self.gr_reply_commit.one(),
            ReplyCommitWGas => self.gr_reply_commit_wgas.one(),
            ReservationReply(len) => cost_with_per_byte!(gr_reservation_reply, len),
            ReservationReplyCommit => self.gr_reservation_reply_commit.one(),
            ReplyInput => self.gr_reply_input.one(),
            ReplyInputWGas => self.gr_reply_input_wgas.one(),
            ReplyPushInput => self.gr_reply_push_input.one(),
            ReplyTo => self.gr_reply_to.one(),
            SignalCode => self.gr_signal_code.one(),
            SignalFrom => self.gr_signal_from.one(),
            Debug(len) => cost_with_per_byte!(gr_debug, len),
            ReplyCode => self.gr_reply_code.one(),
            Exit => self.gr_exit.one(),
            Leave => self.gr_leave.one(),
            Wait => self.gr_wait.one(),
            WaitFor => self.gr_wait_for.one(),
            WaitUpTo => self.gr_wait_up_to.one(),
            Wake => self.gr_wake.one(),
            CreateProgram(payload, salt) => self
                .gr_create_program
                .one()
                .saturating_add(self.gr_create_program_payload_per_byte.calc(payload))
                .saturating_add(self.gr_create_program_salt_per_byte.calc(salt)),
            CreateProgramWGas(payload, salt) => self
                .gr_create_program_wgas
                .one()
                .saturating_add(self.gr_create_program_wgas_payload_per_byte.calc(payload))
                .saturating_add(self.gr_create_program_wgas_salt_per_byte.calc(salt)),
        }
    }
}
