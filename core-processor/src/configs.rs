// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

//! Configurations.

use alloc::{collections::BTreeSet, vec::Vec};
use gear_core::{
    costs::{Bytes, Calls, CostPer, CostToken, Blocks},
    pages::WasmPage,
};
use gear_lazy_pages_common::LazyPagesWeights;
use gear_wasm_instrument::syscalls::SyscallName;
use paste::paste;

/// Number of max pages number to use it in tests.
pub const TESTS_MAX_PAGES_NUMBER: u16 = 512;

/// Contextual block information.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlockInfo {
    /// Height.
    pub height: u32,
    /// Timestamp.
    pub timestamp: u64,
}

/// +_+_+
#[derive(Clone, Debug, Default)]
pub struct ProcessCosts {
    /// +_+_+
    pub execution: ExtWeights,
    /// +_+_+
    pub lazy_pages: LazyPagesWeights,
    /// +_+_+
    pub read: CostPer<Calls>,
    /// +_+_+
    pub read_per_byte: CostPer<Bytes>,
    /// +_+_+
    pub write: CostPer<Calls>,
    /// +_+_+
    pub instrumentation: CostPer<Calls>,
    /// +_+_+
    pub instrumentation_per_byte: CostPer<Bytes>,
    /// +_+_+
    pub static_page: CostPer<WasmPage>,
    /// WASM module instantiation byte cost.
    pub module_instantiation_byte_cost: CostPer<Bytes>,
}

/// Execution settings for handling messages.
pub(crate) struct ExecutionSettings {
    /// Contextual block information.
    pub block_info: BlockInfo,
    /// Performance multiplier.
    pub performance_multiplier: gsys::Percent,
    pub ext_costs: ExtWeights,
    pub lazy_pages_costs: LazyPagesWeights,
    /// Existential deposit.
    pub existential_deposit: u128,
    /// Mailbox threshold.
    pub mailbox_threshold: u64,
    pub max_pages: WasmPage,
    pub forbidden_funcs: BTreeSet<SyscallName>,
    /// Reserve for parameter of scheduling.
    pub reserve_for: u32,
    /// Most recently determined random seed, along with the time in the past since when it was determinable by chain observers.
    // TODO: find a way to put a random seed inside block config.
    pub random_data: (Vec<u8>, u32),
    /// Gas multiplier.
    pub gas_multiplier: gsys::GasMultiplier,
}

/// Stable parameters for the whole block across processing runs.
#[derive(Clone)]
pub struct BlockConfig {
    /// Block info.
    pub block_info: BlockInfo,
    /// Performance multiplier.
    pub performance_multiplier: gsys::Percent,
    /// Forbidden functions.
    pub forbidden_funcs: BTreeSet<SyscallName>,
    /// Reserve for parameter of scheduling.
    pub reserve_for: u32,
    /// Gas multiplier.
    pub gas_multiplier: gsys::GasMultiplier,
    /// +_+_+
    pub costs: ProcessCosts,
    /// Existential deposit.
    pub existential_deposit: u128,
    /// Mailbox threshold.
    pub mailbox_threshold: u64,
    /// Amount of reservations can exist for 1 program.
    pub max_reservations: u64,
    /// Max allowed page numbers for wasm program.
    pub max_pages: WasmPage,
    /// Outgoing limit.
    pub outgoing_limit: u32,
    /// Outgoing bytes limit.
    pub outgoing_bytes_limit: u32,
}

/// +_+_+
#[derive(Debug, Clone, Default)]
pub struct SyscallCosts {
    /// Weight of calling `alloc`.
    pub alloc: CostPer<Calls>,
    /// Weight per allocated page for `alloc`.
    pub alloc_per_page: CostPer<WasmPage>,
    /// Weight of calling `free`.
    pub free: CostPer<Calls>,
    /// Weight of calling `free_range`
    pub free_range: CostPer<Calls>,
    /// Weight of calling `free_range` per page
    pub free_range_per_page: CostPer<WasmPage>,
    /// Weight of calling `gr_reserve_gas`.
    pub gr_reserve_gas: CostPer<Calls>,
    /// Weight of calling `gr_unreserve_gas`
    pub gr_unreserve_gas: CostPer<Calls>,
    /// Weight of calling `gr_system_reserve_gas`
    pub gr_system_reserve_gas: CostPer<Calls>,
    /// Weight of calling `gr_gas_available`.
    pub gr_gas_available: CostPer<Calls>,
    /// Weight of calling `gr_message_id`.
    pub gr_message_id: CostPer<Calls>,
    /// Weight of calling `gr_program_id`.
    pub gr_program_id: CostPer<Calls>,
    /// Weight of calling `gr_source`.
    pub gr_source: CostPer<Calls>,
    /// Weight of calling `gr_value`.
    pub gr_value: CostPer<Calls>,
    /// Weight of calling `gr_value_available`.
    pub gr_value_available: CostPer<Calls>,
    /// Weight of calling `gr_size`.
    pub gr_size: CostPer<Calls>,
    /// Weight of calling `gr_read`.
    pub gr_read: CostPer<Calls>,
    /// Weight per payload byte for `gr_read`.
    pub gr_read_per_byte: CostPer<Bytes>,
    /// Weight of calling `gr_env_vars`.
    pub gr_env_vars: CostPer<Calls>,
    /// Weight of calling `gr_block_height`.
    pub gr_block_height: CostPer<Calls>,
    /// Weight of calling `gr_block_timestamp`.
    pub gr_block_timestamp: CostPer<Calls>,
    /// Weight of calling `gr_random`.
    pub gr_random: CostPer<Calls>,
    /// Weight of calling `gr_reply_deposit`.
    pub gr_reply_deposit: CostPer<Calls>,
    /// Weight of calling `gr_send`
    pub gr_send: CostPer<Calls>,
    /// Weight per bytes for `gr_send`.
    pub gr_send_per_byte: CostPer<Bytes>,
    /// Weight of calling `gr_send_wgas`.
    pub gr_send_wgas: CostPer<Calls>,
    /// Weight of calling `gr_send_wgas` per one payload byte.
    pub gr_send_wgas_per_byte: CostPer<Bytes>,
    /// Weight of calling `gr_send_init`.
    pub gr_send_init: CostPer<Calls>,
    /// Weight of calling `gr_send_push`.
    pub gr_send_push: CostPer<Calls>,
    /// Weight per payload byte by `gr_send_push`.
    pub gr_send_push_per_byte: CostPer<Bytes>,
    /// Weight of calling `gr_send_commit`.
    pub gr_send_commit: CostPer<Calls>,
    /// Weight of calling `gr_send_commit_wgas`.
    pub gr_send_commit_wgas: CostPer<Calls>,
    /// Weight of calling `gr_reservation_send`.
    pub gr_reservation_send: CostPer<Calls>,
    /// Weight of calling `gr_reservation_send` per one payload byte.
    pub gr_reservation_send_per_byte: CostPer<Bytes>,
    /// Weight of calling `gr_reservation_send_commit`.
    pub gr_reservation_send_commit: CostPer<Calls>,
    /// Weight of calling `gr_send_init`.
    pub gr_send_input: CostPer<Calls>,
    /// Weight of calling `gr_send_init_wgas`.
    pub gr_send_input_wgas: CostPer<Calls>,
    /// Weight of calling `gr_send_push_input`.
    pub gr_send_push_input: CostPer<Calls>,
    /// Weight per payload byte by `gr_send_push_input`.
    pub gr_send_push_input_per_byte: CostPer<Bytes>,
    /// Weight of calling `gr_reply`.
    pub gr_reply: CostPer<Calls>,
    /// Weight of calling `gr_reply` per one payload byte.
    pub gr_reply_per_byte: CostPer<Bytes>,
    /// Weight of calling `gr_reply_wgas`.
    pub gr_reply_wgas: CostPer<Calls>,
    /// Weight of calling `gr_reply_wgas` per one payload byte.
    pub gr_reply_wgas_per_byte: CostPer<Bytes>,
    /// Weight of calling `gr_reply_commit`.
    pub gr_reply_commit: CostPer<Calls>,
    /// Weight of calling `gr_reply_commit_wgas`.
    pub gr_reply_commit_wgas: CostPer<Calls>,
    /// Weight of calling `gr_reservation_reply`.
    pub gr_reservation_reply: CostPer<Calls>,
    /// Weight of calling `gr_reservation_reply` per one payload byte.
    pub gr_reservation_reply_per_byte: CostPer<Bytes>,
    /// Weight of calling `gr_reservation_reply_commit`.
    pub gr_reservation_reply_commit: CostPer<Calls>,
    /// Weight of calling `gr_reply_push`.
    pub gr_reply_push: CostPer<Calls>,
    /// Weight per payload byte by `gr_reply_push`.
    pub gr_reply_push_per_byte: CostPer<Bytes>,
    /// Weight of calling `gr_reply_input`.
    pub gr_reply_input: CostPer<Calls>,
    /// Weight of calling `gr_reply_input_wgas`.
    pub gr_reply_input_wgas: CostPer<Calls>,
    /// Weight of calling `gr_reply_push_input`.
    pub gr_reply_push_input: CostPer<Calls>,
    /// Weight per payload byte by `gr_reply_push_input`.
    pub gr_reply_push_input_per_byte: CostPer<Bytes>,
    /// Weight of calling `gr_reply_to`.
    pub gr_reply_to: CostPer<Calls>,
    /// Weight of calling `gr_signal_code`.
    pub gr_signal_code: CostPer<Calls>,
    /// Weight of calling `gr_signal_from`.
    pub gr_signal_from: CostPer<Calls>,
    /// Weight of calling `gr_debug`.
    pub gr_debug: CostPer<Calls>,
    /// Weight per payload byte by `gr_debug`.
    pub gr_debug_per_byte: CostPer<Bytes>,
    /// Weight of calling `gr_reply_code`.
    pub gr_reply_code: CostPer<Calls>,
    /// Weight of calling `gr_exit`.
    pub gr_exit: CostPer<Calls>,
    /// Weight of calling `gr_leave`.
    pub gr_leave: CostPer<Calls>,
    /// Weight of calling `gr_wait`.
    pub gr_wait: CostPer<Calls>,
    /// Weight of calling `gr_wait_for`.
    pub gr_wait_for: CostPer<Calls>,
    /// Weight of calling `gr_wait_up_to`.
    pub gr_wait_up_to: CostPer<Calls>,
    /// Weight of calling `gr_wake`.
    pub gr_wake: CostPer<Calls>,
    /// Weight of calling `gr_create_program_wgas`.
    pub gr_create_program: CostPer<Calls>,
    /// Weight per payload byte by `gr_create_program_wgas`.
    pub gr_create_program_payload_per_byte: CostPer<Bytes>,
    /// Weight per salt byte by `gr_create_program_wgas`.
    pub gr_create_program_salt_per_byte: CostPer<Bytes>,
    /// Weight of calling `gr_create_program_wgas`.
    pub gr_create_program_wgas: CostPer<Calls>,
    /// Weight per payload byte by `gr_create_program_wgas`.
    pub gr_create_program_wgas_payload_per_byte: CostPer<Bytes>,
    /// Weight per salt byte by `gr_create_program_wgas`.
    pub gr_create_program_wgas_salt_per_byte: CostPer<Bytes>,
}

// +_+_+ change naming weights to cost
// +_+_+ Comments
/// Describes the weight for each imported function that a program is allowed to call.
#[derive(Debug, Default, Clone)]
pub struct ExtWeights {
    /// +_+_+
    pub syscalls: SyscallCosts,
    /// Cost for single block waitlist holding.
    pub waitlist_cost: CostPer<Blocks>,
    /// Cost of holding a message in dispatch stash.
    pub dispatch_hold_cost: CostPer<Blocks>,
    /// Cost for reservation holding.
    pub reservation: CostPer<Blocks>,
    /// +_+_+
    pub mem_grow: CostPer<WasmPage>,
}

impl SyscallCosts {
    /// +_+_+
    pub fn cost_for_token(&self, token: CostToken) -> u64 {
        use gear_core::costs::CostToken::*;

        macro_rules! cost_with_weight_per_byte {
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
            Send(len) => cost_with_weight_per_byte!(gr_send, len),
            SendWGas(len) => cost_with_weight_per_byte!(gr_send_wgas, len),
            SendInit => self.gr_send_init.one(),
            SendPush(len) => cost_with_weight_per_byte!(gr_send_push, len),
            SendCommit => self.gr_send_commit.one(),
            SendCommitWGas => self.gr_send_commit_wgas.one(),
            ReservationSend(len) => cost_with_weight_per_byte!(gr_reservation_send, len),
            ReservationSendCommit => self.gr_reservation_send_commit.one(),
            SendInput => self.gr_send_input.one(),
            SendInputWGas => self.gr_send_input_wgas.one(),
            SendPushInput => self.gr_send_push_input.one(),
            Reply(len) => cost_with_weight_per_byte!(gr_reply, len),
            ReplyWGas(len) => cost_with_weight_per_byte!(gr_reply_wgas, len),
            ReplyPush(len) => cost_with_weight_per_byte!(gr_reply_push, len),
            ReplyCommit => self.gr_reply_commit.one(),
            ReplyCommitWGas => self.gr_reply_commit_wgas.one(),
            ReservationReply(len) => cost_with_weight_per_byte!(gr_reservation_reply, len),
            ReservationReplyCommit => self.gr_reservation_reply_commit.one(),
            ReplyInput => self.gr_reply_input.one(),
            ReplyInputWGas => self.gr_reply_input_wgas.one(),
            ReplyPushInput => self.gr_reply_push_input.one(),
            ReplyTo => self.gr_reply_to.one(),
            SignalCode => self.gr_signal_code.one(),
            SignalFrom => self.gr_signal_from.one(),
            Debug(len) => cost_with_weight_per_byte!(gr_debug, len),
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
