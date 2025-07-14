// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use gear_wasm_gen::{
    ConfigsBundle, GearWasmGeneratorConfig, InstructionKind, MemoryPagesConfig, SelectableParams,
    SyscallsConfigBuilder, SyscallsInjectionTypes,
};
use gear_wasm_instrument::{Instruction, Rules, gas_metering::MemoryGrowCost};
use std::num::NonZero;

use crate::{
    INITIAL_PAGES,
    generate::{InjectGlobalsConfig, InjectMemoryAccessesConfig},
};

#[derive(Debug, Default, Clone)]
pub struct FuzzerConfigBundle {
    pub memory_accesses: InjectMemoryAccessesConfig,
    pub globals: InjectGlobalsConfig,
}

impl ConfigsBundle for FuzzerConfigBundle {
    fn into_parts(self) -> (GearWasmGeneratorConfig, SelectableParams) {
        use InstructionKind::*;
        (
            GearWasmGeneratorConfig {
                memory_config: MemoryPagesConfig {
                    initial_size: INITIAL_PAGES,
                    upper_limit: None,
                    stack_end_page: None,
                },
                syscalls_config: SyscallsConfigBuilder::new(SyscallsInjectionTypes::all_never())
                    .build(),
                remove_recursions: false,
                ..Default::default()
            },
            SelectableParams {
                allowed_instructions: vec![
                    Numeric, Reference, Parametric, Variable, Table, Memory, Control,
                ],
                max_instructions: 500,
                min_funcs: NonZero::<usize>::new(3).expect("non zero value"),
                max_funcs: NonZero::<usize>::new(5).expect("non zero value"),
            },
        )
    }
}

/// Dummy cost rules for the fuzzer
/// We don't care about the actual costs, just that they are non-zero
pub struct DummyCostRules;

impl Rules for DummyCostRules {
    fn instruction_cost(&self, _instruction: &Instruction) -> Option<u32> {
        const DUMMY_COST: u32 = 13;
        Some(DUMMY_COST)
    }

    fn memory_grow_cost(&self) -> MemoryGrowCost {
        const DUMMY_MEMORY_GROW_COST: u32 = 1242;
        MemoryGrowCost::Linear(NonZero::<u32>::new(DUMMY_MEMORY_GROW_COST).unwrap())
    }

    fn call_per_local_cost(&self) -> u32 {
        const DUMMY_COST_PER_CALL: u32 = 132;
        DUMMY_COST_PER_CALL
    }
}
