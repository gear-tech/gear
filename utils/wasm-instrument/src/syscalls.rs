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

//! Gear syscalls for smart contracts execution signatures.

use crate::parity_wasm::elements::{FunctionType, ValueType};
use alloc::{vec, vec::Vec};

/// Syscall param type.
#[derive(Debug, Clone, Copy)]
pub enum ParamType {
    Size,            // i32 buffers size in memory
    Ptr,             // i32 pointer
    Gas,             // i64 gas amount
    MessagePosition, // i32 message position
    Duration,        // i32 duration in blocks
    Delay,           // i32 delay in blocks
    Handler,         // i32 handler number
    Alloc,           // i32 alloc pages
    Free,            // i32 free page
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
#[derive(Debug, Clone)]
pub struct SysCallSignature {
    pub params: Vec<ParamType>,
    pub results: Vec<ValueType>,
}

impl SysCallSignature {
    fn gr<const N: usize>(params: [ParamType; N]) -> Self {
        Self {
            params: params.to_vec(),
            results: Default::default(),
        }
    }

    fn system<const N: usize, const M: usize>(
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

/// Returns list of all syscall names (actually supported by this module syscalls).
pub fn syscalls_name_list() -> Vec<&'static str> {
    vec![
        "alloc",
        "free",
        "gr_debug",
        "gr_error",
        "gr_block_height",
        "gr_block_timestamp",
        "gr_exit",
        "gr_gas_available",
        "gr_program_id",
        "gr_origin",
        "gr_leave",
        "gr_value_available",
        "gr_wait",
        "gr_wait_up_to",
        "gr_wait_for",
        "gr_wake",
        "gr_status_code",
        "gr_message_id",
        "gr_read",
        "gr_reply",
        "gr_reply_wgas",
        "gr_reply_commit",
        "gr_reply_commit_wgas",
        "gr_reservation_reply",
        "gr_reservation_reply_commit",
        "gr_reply_push",
        "gr_reply_push_input",
        "gr_reply_to",
        "gr_send",
        "gr_send_wgas",
        "gr_send_commit",
        "gr_send_commit_wgas",
        "gr_send_init",
        "gr_send_push",
        "gr_send_push_input",
        "gr_reservation_send",
        "gr_reservation_send_commit",
        "gr_size",
        "gr_source",
        "gr_value",
        "gr_create_program",
        "gr_create_program_wgas",
        "gr_reserve_gas",
        "gr_unreserve_gas",
        "gr_random",
    ]
}

/// Returns signature for syscall by name.
pub fn syscall_signature(name: &str) -> SysCallSignature {
    use ParamType::*;
    use ValueType::I32;
    match name {
        "alloc" => SysCallSignature::system([Alloc], [I32]),
        "free" => SysCallSignature::system([Free], []),
        "gr_debug" => SysCallSignature::gr([Ptr, Size]),
        "gr_error" => SysCallSignature::gr([Ptr, Ptr]),
        "gr_block_height" => SysCallSignature::gr([Ptr]),
        "gr_block_timestamp" => SysCallSignature::gr([Ptr]),
        "gr_exit" => SysCallSignature::gr([Ptr]),
        "gr_gas_available" => SysCallSignature::gr([Ptr]),
        "gr_program_id" => SysCallSignature::gr([Ptr]),
        "gr_origin" => SysCallSignature::gr([Ptr]),
        "gr_leave" => SysCallSignature::gr([]),
        "gr_value_available" => SysCallSignature::gr([Ptr]),
        "gr_wait" => SysCallSignature::gr([]),
        "gr_wait_up_to" => SysCallSignature::gr([Duration]),
        "gr_wait_for" => SysCallSignature::gr([Duration]),
        "gr_wake" => SysCallSignature::gr([Ptr, Delay, Ptr]),
        "gr_status_code" => SysCallSignature::gr([Ptr]),
        "gr_message_id" => SysCallSignature::gr([Ptr]),
        "gr_read" => SysCallSignature::gr([MessagePosition, Size, Ptr, Ptr]),
        "gr_reply" => SysCallSignature::gr([Ptr, Size, Ptr, Delay, Ptr]),
        "gr_reply_input" => SysCallSignature::gr([Size, Size, Ptr, Delay, Ptr]),
        "gr_reply_wgas" => SysCallSignature::gr([Ptr, Size, Gas, Ptr, Delay, Ptr]),
        "gr_reply_input_wgas" => SysCallSignature::gr([Size, Size, Gas, Ptr, Delay, Ptr]),
        "gr_reply_commit" => SysCallSignature::gr([Ptr, Delay, Ptr]),
        "gr_reply_commit_wgas" => SysCallSignature::gr([Gas, Ptr, Delay, Ptr]),
        "gr_reservation_reply" => SysCallSignature::gr([Ptr, Ptr, Size, Delay, Ptr]),
        "gr_reservation_reply_commit" => SysCallSignature::gr([Ptr, Delay, Ptr]),
        "gr_reply_push" => SysCallSignature::gr([Ptr, Size, Ptr]),
        "gr_reply_push_input" => SysCallSignature::gr([Size, Size, Ptr]),
        "gr_reply_to" => SysCallSignature::gr([Ptr]),
        "gr_send" => SysCallSignature::gr([Ptr, Ptr, Size, Delay, Ptr]),
        "gr_send_input" => SysCallSignature::gr([Ptr, Size, Size, Delay, Ptr]),
        "gr_send_wgas" => SysCallSignature::gr([Ptr, Ptr, Size, Gas, Delay, Ptr]),
        "gr_send_input_wgas" => SysCallSignature::gr([Ptr, Size, Size, Gas, Delay, Ptr]),
        "gr_send_commit" => SysCallSignature::gr([Handler, Ptr, Delay, Ptr]),
        "gr_send_commit_wgas" => SysCallSignature::gr([Handler, Ptr, Gas, Delay, Ptr]),
        "gr_send_init" => SysCallSignature::gr([Ptr]),
        "gr_send_push" => SysCallSignature::gr([Handler, Ptr, Size, Ptr]),
        "gr_send_push_input" => SysCallSignature::gr([Handler, Size, Size, Ptr]),
        "gr_reservation_send" => SysCallSignature::gr([Ptr, Ptr, Size, Delay, Ptr]),
        "gr_reservation_send_commit" => SysCallSignature::gr([Handler, Ptr, Delay, Ptr]),
        "gr_size" => SysCallSignature::gr([Ptr]),
        "gr_source" => SysCallSignature::gr([Ptr]),
        "gr_value" => SysCallSignature::gr([Ptr]),
        "gr_create_program" => SysCallSignature::gr([Ptr, Ptr, Size, Ptr, Size, Delay, Ptr]),
        "gr_create_program_wgas" => {
            SysCallSignature::gr([Ptr, Ptr, Size, Ptr, Size, Gas, Delay, Ptr])
        }
        "gr_reserve_gas" => SysCallSignature::gr([Gas, Duration, Ptr]),
        "gr_unreserve_gas" => SysCallSignature::gr([Ptr, Ptr]),
        "gr_system_reserve_gas" => SysCallSignature::gr([Gas, Ptr]),
        "gr_random" => SysCallSignature::gr([Ptr, Size, Ptr]),
        other => panic!("Unknown syscall name: '{}'", other),
    }
}
