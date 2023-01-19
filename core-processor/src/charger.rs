// Copyright (C)  2023 Gear Technologies Inc.
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
//

use crate::{
    common::{ExecutionErrorReason, GasOperation},
    configs::PagesConfig,
};
use alloc::collections::BTreeSet;
use gear_core::{
    gas::{ChargeResult, GasAllowanceCounter, GasCounter},
    memory::{PageU32Size, WasmPageNumber},
};
use gear_core_errors::MemoryError;

pub enum ProcessorChargeError {
    BlockGasExceeded,
    GasExceeded,
}

pub struct ProcessorGasCharger<'a> {
    counter: &'a mut GasCounter,
    allowance_counter: &'a mut GasAllowanceCounter,
}

impl<'a> ProcessorGasCharger<'a> {
    pub fn new(
        counter: &'a mut GasCounter,
        allowance_counter: &'a mut GasAllowanceCounter,
    ) -> Self {
        Self {
            counter,
            allowance_counter,
        }
    }

    pub fn charge_gas_per_byte(&mut self, amount: u64) -> Result<(), ProcessorChargeError> {
        if self.allowance_counter.charge(amount) != ChargeResult::Enough {
            return Err(ProcessorChargeError::BlockGasExceeded);
        }
        if self.counter.charge(amount) != ChargeResult::Enough {
            return Err(ProcessorChargeError::GasExceeded);
        }

        Ok(())
    }

    fn charge_gas(
        &mut self,
        operation: GasOperation,
        amount: u64,
    ) -> Result<(), ExecutionErrorReason> {
        if self.allowance_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionErrorReason::BlockGasExceeded(operation));
        }
        if self.counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionErrorReason::GasExceeded(operation));
        }

        Ok(())
    }

    pub fn charge_gas_for_program_data(
        &mut self,
        read_cost: u64,
        per_byte_cost: u64,
    ) -> Result<(), ProcessorChargeError> {
        self.charge_gas_per_byte(calculate_gas_for_program(read_cost, per_byte_cost))
    }

    pub fn charge_gas_for_program_code(
        &mut self,
        read_cost: u64,
        per_byte_cost: u64,
        code_len_bytes: u32,
    ) -> Result<(), ProcessorChargeError> {
        self.charge_gas_per_byte(calculate_gas_for_code(
            read_cost,
            per_byte_cost,
            code_len_bytes.into(),
        ))
    }

    pub fn charge_gas_for_instantiation(
        &mut self,
        gas_per_byte: u64,
        code_length: u32,
    ) -> Result<(), ExecutionErrorReason> {
        let amount = gas_per_byte * code_length as u64;
        self.charge_gas(GasOperation::ModuleInstantiation, amount)
    }

    pub fn charge_gas_for_instrumentation(
        &mut self,
        instrumentation_cost: u64,
        instrumentation_byte_cost: u64,
        original_code_len_bytes: u32,
    ) -> Result<(), ProcessorChargeError> {
        let amount = instrumentation_cost.saturating_add(
            instrumentation_byte_cost.saturating_mul(original_code_len_bytes.into()),
        );
        self.charge_gas_per_byte(amount)
    }

    fn charge_gas_for_initial_memory(
        &mut self,
        settings: &PagesConfig,
        static_pages: WasmPageNumber,
    ) -> Result<(), ExecutionErrorReason> {
        // TODO: check calculation is safe: #2007.
        let amount = settings.init_cost * static_pages.raw() as u64;
        self.charge_gas(GasOperation::InitialMemory, amount)
    }

    fn charge_gas_for_load_memory(
        &mut self,
        settings: &PagesConfig,
        allocations: &BTreeSet<WasmPageNumber>,
        static_pages: WasmPageNumber,
    ) -> Result<(), ExecutionErrorReason> {
        // TODO: check calculation is safe: #2007.
        let amount =
            settings.load_page_cost * (allocations.len() as u64 + static_pages.raw() as u64);
        self.charge_gas(GasOperation::LoadMemory, amount)
    }

    fn charge_gas_for_grow_memory(
        &mut self,
        settings: &PagesConfig,
        max_wasm_page: WasmPageNumber,
        static_pages: WasmPageNumber,
    ) -> Result<(), ExecutionErrorReason> {
        // TODO: make separate class for size in pages (here is static_pages): #2008.
        // TODO: check calculation is safe: #2007.
        let amount =
            settings.mem_grow_cost * (max_wasm_page.raw() as u64 + 1 - static_pages.raw() as u64);
        self.charge_gas(GasOperation::GrowMemory, amount)
    }

    /// Charge gas for pages init/load/grow and checks that there is enough gas for that.
    /// Returns size of wasm memory buffer which must be created in execution environment.
    // TODO: (issue #1894) remove charging for pages access/write/read/upload. But we should charge for
    // potential situation when gas limit/allowance exceeded during lazy-pages handling,
    // but we should continue execution until the end of block. During that execution
    // another signals can occur, which also take some time to process them.
    pub fn charge_gas_for_pages(
        &mut self,
        settings: &PagesConfig,
        allocations: &BTreeSet<WasmPageNumber>,
        static_pages: WasmPageNumber,
        initial_execution: bool,
        subsequent_execution: bool,
    ) -> Result<WasmPageNumber, ExecutionErrorReason> {
        // Initial execution: just charge for static pages
        if initial_execution {
            // Charging gas for initial pages
            self.charge_gas_for_initial_memory(settings, static_pages)?;
            return Ok(static_pages);
        }

        let max_wasm_page = if let Some(page) = allocations.iter().next_back() {
            *page
        } else if let Ok(max_wasm_page) = static_pages.dec() {
            max_wasm_page
        } else {
            return Ok(WasmPageNumber::zero());
        };

        if !subsequent_execution {
            // Charging gas for loaded pages
            self.charge_gas_for_load_memory(settings, allocations, static_pages)?;
        }

        // Charging gas for mem size
        self.charge_gas_for_grow_memory(settings, max_wasm_page, static_pages)?;

        // +1 because pages numeration begins from 0
        let wasm_mem_size = max_wasm_page
            .inc()
            .map_err(|_| ExecutionErrorReason::Memory(MemoryError::OutOfBounds))?;

        Ok(wasm_mem_size)
    }
}

/// Calculates gas amount required to charge for program loading.
pub fn calculate_gas_for_program(read_cost: u64, _per_byte_cost: u64) -> u64 {
    read_cost
}

/// Calculates gas amount required to charge for code loading.
pub fn calculate_gas_for_code(read_cost: u64, per_byte_cost: u64, code_len_bytes: u64) -> u64 {
    read_cost.saturating_add(code_len_bytes.saturating_mul(per_byte_cost))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gear_backend_common::lazy_pages::LazyPagesWeights;
    use gear_core::memory::PageNumber;

    fn prepare_allocs() -> BTreeSet<WasmPageNumber> {
        let data = [0u16, 1, 2, 8, 18, 25, 27, 28, 93, 146, 240, 518];
        data.map(Into::into).map(|p: PageNumber| p.to_page()).into()
    }

    fn prepare_alloc_config() -> PagesConfig {
        PagesConfig {
            max_pages: 32.into(),
            lazy_pages_weights: LazyPagesWeights {
                read: 100,
                write: 100,
                write_after_read: 100,
            },
            init_cost: 1000,
            alloc_cost: 2000,
            mem_grow_cost: 3000,
            load_page_cost: 4000,
        }
    }

    fn prepare_gas_counters() -> (GasCounter, GasAllowanceCounter) {
        (
            GasCounter::new(1_000_000),
            GasAllowanceCounter::new(4_000_000),
        )
    }

    #[test]
    fn gas_for_pages_initial() {
        let settings = prepare_alloc_config();
        let (mut gas_counter, mut gas_allowance_counter) = prepare_gas_counters();
        let mut charger = ProcessorGasCharger::new(&mut gas_counter, &mut gas_allowance_counter);
        let static_pages = 4;
        let res = charger.charge_gas_for_pages(
            &settings,
            &Default::default(),
            static_pages.into(),
            true,
            false,
        );
        // Result is static pages count
        assert_eq!(res, Ok(static_pages.into()));
        // Charging for static pages initialization
        let charge = settings.init_cost * static_pages as u64;
        assert_eq!(charger.counter.left(), 1_000_000 - charge);
        assert_eq!(charger.allowance_counter.left(), 4_000_000 - charge);
    }

    #[test]
    fn gas_for_pages_static() {
        let settings = prepare_alloc_config();
        let (mut gas_counter, mut gas_allowance_counter) = prepare_gas_counters();
        let mut charger = ProcessorGasCharger::new(&mut gas_counter, &mut gas_allowance_counter);
        let static_pages = 4;
        let res = charger.charge_gas_for_pages(
            &settings,
            &Default::default(),
            static_pages.into(),
            false,
            false,
        );
        // Result is static pages count
        assert_eq!(res, Ok(static_pages.into()));
        // Charge for the first load of static pages
        let charge = settings.load_page_cost * static_pages as u64;
        assert_eq!(charger.counter.left(), 1_000_000 - charge);
        assert_eq!(charger.allowance_counter.left(), 4_000_000 - charge);
    }

    #[test]
    fn gas_for_pages_alloc() {
        let settings = prepare_alloc_config();
        let (mut gas_counter, mut gas_allowance_counter) = prepare_gas_counters();
        let mut charger = ProcessorGasCharger::new(&mut gas_counter, &mut gas_allowance_counter);
        let allocs = prepare_allocs();
        let static_pages = 4;
        let res =
            charger.charge_gas_for_pages(&settings, &allocs, static_pages.into(), false, false);
        // Result is the last page plus one
        let last = *allocs.iter().last().unwrap();
        assert_eq!(res, Ok(last.inc().unwrap()));
        // Charge for loading and mem grow
        let load_charge = settings.load_page_cost * (allocs.len() as u64 + static_pages as u64);
        let grow_charge = settings.mem_grow_cost * (last.raw() as u64 + 1 - static_pages as u64);
        assert_eq!(
            charger.counter.left(),
            1_000_000 - load_charge - grow_charge
        );
        assert_eq!(
            charger.allowance_counter.left(),
            4_000_000 - load_charge - grow_charge
        );

        // Use the second time (`subsequent` = `true`)
        let (mut gas_counter, mut gas_allowance_counter) = prepare_gas_counters();
        let mut charger = ProcessorGasCharger::new(&mut gas_counter, &mut gas_allowance_counter);
        let res =
            charger.charge_gas_for_pages(&settings, &allocs, static_pages.into(), false, true);
        assert_eq!(res, Ok(last.inc().unwrap()));
        // Charge for mem grow only
        assert_eq!(charger.counter.left(), 1_000_000 - grow_charge);
        assert_eq!(charger.allowance_counter.left(), 4_000_000 - grow_charge);
    }
}
