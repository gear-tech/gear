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

use std::num::{NonZeroU32, NonZeroUsize};

use arbitrary::{Arbitrary, Unstructured};
use gear_wasm_gen::{
    ConfigsBundle, GearWasmGeneratorConfig, MemoryPagesConfig, SelectableParams,
    SyscallsConfigBuilder, SyscallsInjectionTypes,
};
use gear_wasm_instrument::{
    gas_metering::MemoryGrowCost, parity_wasm::elements::Instruction, Rules,
};
use wasm_smith::InstructionKind;

use crate::INITIAL_PAGES;

pub struct FuzzerConfigBundle;

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
                remove_recursions: true,
                inject_memory_accesses: true,
                ..Default::default()
            },
            SelectableParams {
                allowed_instructions: vec![
                    Numeric, Reference, Parametric, Variable, Table, Memory, Control,
                ],
                max_instructions: 500,
                min_funcs: NonZeroUsize::new(3).expect("from non zero value; qed."),
                max_funcs: NonZeroUsize::new(5).expect("from non zero value; qed."),
            },
        )
    }
}

pub struct FuzzerCostRules;

impl Rules for FuzzerCostRules {
    fn instruction_cost(&self, _instruction: &Instruction) -> Option<u32> {
        Some(13)
    }

    fn memory_grow_cost(&self) -> MemoryGrowCost {
        MemoryGrowCost::Linear(NonZeroU32::new(1242).unwrap())
    }

    fn call_per_local_cost(&self) -> u32 {
        132
    }
}
pub struct FuzzerInput<'a>(pub(crate) &'a [u8]);

impl<'a> Arbitrary<'a> for FuzzerInput<'a> {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let data = u
            .peek_bytes(u.len())
            .ok_or(arbitrary::Error::NotEnoughData)?;

        Ok(Self(data))
    }
}

impl std::fmt::Debug for FuzzerInput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RuntimeFuzzerInput")
            .field(&"Mock `Debug` impl")
            .finish()
    }
}
