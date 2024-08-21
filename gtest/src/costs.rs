// This file is part of Gear.
//
// Copyright (C) 2021-2024 Gear Technologies Inc.
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

#![doc = r" This is auto-generated module that contains costs constructors"]
#![doc = r" `pallets/gear/src/schedule.rs`."]
#![doc = r""]
#![doc = r" See `./scripts/weight-dump.sh` if you want to update it."]

use core_processor::configs::{ExtCosts, InstantiationCosts, ProcessCosts, RentCosts};
use gear_core::costs::SyscallCosts;
use gear_lazy_pages_common::LazyPagesCosts;
use gear_wasm_instrument::gas_metering::{
    InstantiationWeights, MemoryWeights, Schedule, SyscallWeights,
};

pub fn lazy_pages_costs(val: &MemoryWeights) -> LazyPagesCosts {
    LazyPagesCosts {
        host_func_read: val.lazy_pages_host_func_read.ref_time.into(),
        host_func_write: val
            .lazy_pages_host_func_write
            .ref_time
            .saturating_add(val.upload_page_data.ref_time)
            .into(),
        host_func_write_after_read: val
            .lazy_pages_host_func_write_after_read
            .ref_time
            .saturating_add(val.upload_page_data.ref_time)
            .into(),
        load_page_storage_data: val
            .load_page_data
            .ref_time
            .saturating_add(val.parachain_read_heuristic.ref_time)
            .into(),
        signal_read: val.lazy_pages_signal_read.ref_time.into(),
        signal_write: val
            .lazy_pages_signal_write
            .ref_time
            .saturating_add(val.upload_page_data.ref_time)
            .into(),
        signal_write_after_read: val
            .lazy_pages_signal_write_after_read
            .ref_time
            .saturating_add(val.upload_page_data.ref_time)
            .into(),
    }
}

pub fn instantiation_costs(val: &InstantiationWeights) -> InstantiationCosts {
    InstantiationCosts {
        code_section_per_byte: val.code_section_per_byte.ref_time.into(),
        data_section_per_byte: val.data_section_per_byte.ref_time.into(),
        global_section_per_byte: val.global_section_per_byte.ref_time.into(),
        table_section_per_byte: val.table_section_per_byte.ref_time.into(),
        element_section_per_byte: val.element_section_per_byte.ref_time.into(),
        type_section_per_byte: val.type_section_per_byte.ref_time.into(),
    }
}

pub fn syscall_costs(val: &SyscallWeights) -> SyscallCosts {
    SyscallCosts {
        alloc: val.alloc.ref_time.into(),
        free: val.free.ref_time.into(),
        free_range: val.free_range.ref_time.into(),
        free_range_per_page: val.free_range_per_page.ref_time.into(),
        gr_reserve_gas: val.gr_reserve_gas.ref_time.into(),
        gr_unreserve_gas: val.gr_unreserve_gas.ref_time.into(),
        gr_system_reserve_gas: val.gr_system_reserve_gas.ref_time.into(),
        gr_gas_available: val.gr_gas_available.ref_time.into(),
        gr_message_id: val.gr_message_id.ref_time.into(),
        gr_program_id: val.gr_program_id.ref_time.into(),
        gr_source: val.gr_source.ref_time.into(),
        gr_value: val.gr_value.ref_time.into(),
        gr_value_available: val.gr_value_available.ref_time.into(),
        gr_size: val.gr_size.ref_time.into(),
        gr_read: val.gr_read.ref_time.into(),
        gr_read_per_byte: val.gr_read_per_byte.ref_time.into(),
        gr_env_vars: val.gr_env_vars.ref_time.into(),
        gr_block_height: val.gr_block_height.ref_time.into(),
        gr_block_timestamp: val.gr_block_timestamp.ref_time.into(),
        gr_random: val.gr_random.ref_time.into(),
        gr_reply_deposit: val.gr_reply_deposit.ref_time.into(),
        gr_send: val.gr_send.ref_time.into(),
        gr_send_per_byte: val.gr_send_per_byte.ref_time.into(),
        gr_send_wgas: val.gr_send_wgas.ref_time.into(),
        gr_send_wgas_per_byte: val.gr_send_wgas_per_byte.ref_time.into(),
        gr_send_init: val.gr_send_init.ref_time.into(),
        gr_send_push: val.gr_send_push.ref_time.into(),
        gr_send_push_per_byte: val.gr_send_push_per_byte.ref_time.into(),
        gr_send_commit: val.gr_send_commit.ref_time.into(),
        gr_send_commit_wgas: val.gr_send_commit_wgas.ref_time.into(),
        gr_reservation_send: val.gr_reservation_send.ref_time.into(),
        gr_reservation_send_per_byte: val.gr_reservation_send_per_byte.ref_time.into(),
        gr_reservation_send_commit: val.gr_reservation_send_commit.ref_time.into(),
        gr_reply_commit: val.gr_reply_commit.ref_time.into(),
        gr_reply_commit_wgas: val.gr_reply_commit_wgas.ref_time.into(),
        gr_reservation_reply: val.gr_reservation_reply.ref_time.into(),
        gr_reservation_reply_per_byte: val.gr_reservation_reply_per_byte.ref_time.into(),
        gr_reservation_reply_commit: val.gr_reservation_reply_commit.ref_time.into(),
        gr_reply_push: val.gr_reply_push.ref_time.into(),
        gr_reply: val.gr_reply.ref_time.into(),
        gr_reply_per_byte: val.gr_reply_per_byte.ref_time.into(),
        gr_reply_wgas: val.gr_reply_wgas.ref_time.into(),
        gr_reply_wgas_per_byte: val.gr_reply_wgas_per_byte.ref_time.into(),
        gr_reply_push_per_byte: val.gr_reply_push_per_byte.ref_time.into(),
        gr_reply_to: val.gr_reply_to.ref_time.into(),
        gr_signal_code: val.gr_signal_code.ref_time.into(),
        gr_signal_from: val.gr_signal_from.ref_time.into(),
        gr_reply_input: val.gr_reply_input.ref_time.into(),
        gr_reply_input_wgas: val.gr_reply_input_wgas.ref_time.into(),
        gr_reply_push_input: val.gr_reply_push_input.ref_time.into(),
        gr_reply_push_input_per_byte: val.gr_reply_push_input_per_byte.ref_time.into(),
        gr_send_input: val.gr_send_input.ref_time.into(),
        gr_send_input_wgas: val.gr_send_input_wgas.ref_time.into(),
        gr_send_push_input: val.gr_send_push_input.ref_time.into(),
        gr_send_push_input_per_byte: val.gr_send_push_input_per_byte.ref_time.into(),
        gr_debug: val.gr_debug.ref_time.into(),
        gr_debug_per_byte: val.gr_debug_per_byte.ref_time.into(),
        gr_reply_code: val.gr_reply_code.ref_time.into(),
        gr_exit: val.gr_exit.ref_time.into(),
        gr_leave: val.gr_leave.ref_time.into(),
        gr_wait: val.gr_wait.ref_time.into(),
        gr_wait_for: val.gr_wait_for.ref_time.into(),
        gr_wait_up_to: val.gr_wait_up_to.ref_time.into(),
        gr_wake: val.gr_wake.ref_time.into(),
        gr_create_program: val.gr_create_program.ref_time.into(),
        gr_create_program_payload_per_byte: val.gr_create_program_payload_per_byte.ref_time.into(),
        gr_create_program_salt_per_byte: val.gr_create_program_salt_per_byte.ref_time.into(),
        gr_create_program_wgas: val.gr_create_program_wgas.ref_time.into(),
        gr_create_program_wgas_payload_per_byte: val
            .gr_create_program_wgas_payload_per_byte
            .ref_time
            .into(),
        gr_create_program_wgas_salt_per_byte: val
            .gr_create_program_wgas_salt_per_byte
            .ref_time
            .into(),
    }
}

pub fn process_costs(schedule: &Schedule) -> ProcessCosts {
    ProcessCosts {
        ext: ExtCosts {
            rent: RentCosts {
                waitlist: schedule.rent_weights.waitlist.ref_time.into(),
                dispatch_stash: schedule.rent_weights.dispatch_stash.ref_time.into(),
                reservation: schedule.rent_weights.reservation.ref_time.into(),
            },
            syscalls: syscall_costs(&schedule.syscall_weights),
            mem_grow: schedule.memory_weights.mem_grow.ref_time.into(),
            mem_grow_per_page: schedule.memory_weights.mem_grow_per_page.ref_time.into(),
        },
        lazy_pages: lazy_pages_costs(&schedule.memory_weights),
        read: schedule.db_weights.read.ref_time.into(),
        write: schedule.db_weights.write.ref_time.into(),
        read_per_byte: schedule.db_weights.read_per_byte.ref_time.into(),
        instrumentation: schedule.code_instrumentation_cost.ref_time.into(),
        instrumentation_per_byte: schedule.code_instrumentation_byte_cost.ref_time.into(),
        instantiation_costs: instantiation_costs(&schedule.instantiation_weights),
        load_allocations_per_interval: schedule.load_allocations_weight.ref_time.into(),
    }
}
