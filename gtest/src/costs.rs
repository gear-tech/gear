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
use gear_lazy_pages_common::LazyPagesCosts;
use gear_wasm_instrument::gas_metering::{InstantiationWeights, MemoryWeights, Schedule};

pub fn lazy_pages_costs(val: &MemoryWeights) -> LazyPagesCosts {
    LazyPagesCosts {
        host_func_read: val.lazy_pages_host_func_read.ref_time.into(),
        host_func_write: val.lazy_pages_host_func_write.ref_time.into(),
        host_func_write_after_read: val.lazy_pages_host_func_write_after_read.ref_time.into(),
        load_page_storage_data: val.load_page_data.ref_time.into(),
        signal_read: val.lazy_pages_signal_read.ref_time.into(),
        signal_write: val.lazy_pages_signal_write.ref_time.into(),
        signal_write_after_read: val.lazy_pages_signal_write_after_read.ref_time.into(),
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

pub fn process_costs(schedule: &Schedule) -> ProcessCosts {
    ProcessCosts {
        ext: ExtCosts {
            rent: RentCosts {
                waitlist: 100u64.into(),
                dispatch_stash: 100u64.into(),
                reservation: 100u64.into(),
            },
            syscalls: Default::default(),
            mem_grow: schedule.memory_weights.mem_grow.ref_time.into(),
            mem_grow_per_page: schedule.memory_weights.mem_grow_per_page.ref_time.into(),
        },
        lazy_pages: lazy_pages_costs(&schedule.memory_weights),
        read: 25000000u64.into(),
        write: 100000000u64.into(),
        read_per_byte: 569u64.into(),
        instrumentation: 306821000u64.into(),
        instrumentation_per_byte: 627777u64.into(),
        instantiation_costs: instantiation_costs(&schedule.instantiation_weights),
        load_allocations_per_interval: 20729u64.into(),
    }
}
