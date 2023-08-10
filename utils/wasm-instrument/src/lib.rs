// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::items_after_test_module)]

extern crate alloc;

use alloc::vec;

use wasm_instrument::{
    gas_metering::{self, Rules},
    parity_wasm::{
        builder,
        elements::{self, BlockType, ImportCountType, Instruction, Instructions, Local, ValueType},
    },
};

use crate::syscalls::SysCallName;
pub use wasm_instrument::{self, parity_wasm};

#[cfg(test)]
mod tests;

pub mod rules;
pub mod syscalls;

// TODO #3057
pub const GLOBAL_NAME_GAS: &str = "gear_gas";
pub const GLOBAL_NAME_FLAGS: &str = "gear_flags";

/// '__gear_stack_end' export is inserted by wasm-proc or wasm-builder,
/// it indicates the end of program stack memory.
pub const STACK_END_EXPORT_NAME: &str = "__gear_stack_end";

pub fn inject<R: Rules>(
    module: elements::Module,
    rules: &R,
    gas_module_name: &str,
) -> Result<elements::Module, elements::Module> {
    if module
        .import_section()
        .map(|section| {
            section.entries().iter().any(|entry| {
                entry.module() == gas_module_name && entry.field() == SysCallName::OutOfGas.to_str()
            })
        })
        .unwrap_or(false)
    {
        return Err(module);
    }

    if module
        .export_section()
        .map(|section| {
            section
                .entries()
                .iter()
                .any(|entry| entry.field() == GLOBAL_NAME_GAS)
        })
        .unwrap_or(false)
    {
        return Err(module);
    }

    let mut mbuilder = builder::from_module(module);

    // fn out_of_gas() -> ();
    let import_sig = mbuilder.push_signature(builder::signature().build_sig());

    mbuilder.push_import(
        builder::import()
            .module(gas_module_name)
            .field(SysCallName::OutOfGas.to_str())
            .external()
            .func(import_sig)
            .build(),
    );

    // back to plain module
    let module = mbuilder.build();

    let import_count = module.import_count(ImportCountType::Function);
    let out_of_gas_index = import_count as u32 - 1;

    let gas_charge_index = module.functions_space();
    let gas_index = module.globals_space() as u32;

    let mut mbuilder = builder::from_module(module);

    mbuilder.push_global(
        builder::global()
            .value_type()
            .i64()
            .init_expr(Instruction::I64Const(0))
            .mutable()
            .build(),
    );

    mbuilder.push_export(
        builder::export()
            .field(GLOBAL_NAME_GAS)
            .internal()
            .global(gas_index)
            .build(),
    );

    // This const is introduced to avoid future errors in code if some other
    // `I64Const` instructions appear in gas charge function body.
    const GAS_CHARGE_COST_PLACEHOLDER: i64 = 1248163264128;

    let mut elements = vec![
        // I. Put global with value of current gas counter of any type.
        Instruction::GetGlobal(gas_index),
        // II. Calculating total gas to charge as sum of:
        //  - `gas_charge(..)` argument;
        //  - `gas_charge(..)` call cost.
        //
        // Setting the sum into local with index 1 with keeping it on stack.
        Instruction::GetLocal(0),
        Instruction::I64ExtendUI32,
        Instruction::I64Const(GAS_CHARGE_COST_PLACEHOLDER),
        Instruction::I64Add,
        Instruction::TeeLocal(1),
        // III. Validating left amount of gas.
        //
        // In case of requested value is bigger than actual gas counter value,
        // than we call `out_of_gas()` that will terminate execution.
        Instruction::I64LtU,
        Instruction::If(BlockType::NoResult),
        Instruction::Call(out_of_gas_index),
        Instruction::End,
        // IV. Calculating new global value by subtraction.
        //
        // Result is stored back into global.
        Instruction::GetGlobal(gas_index),
        Instruction::GetLocal(1),
        Instruction::I64Sub,
        Instruction::SetGlobal(gas_index),
        // V. Ending `gas_charge()` function.
        Instruction::End,
    ];

    // determine cost for successful execution
    let mut block_of_code = false;

    let cost_blocks = match elements
        .iter()
        .filter(|instruction| match instruction {
            Instruction::If(_) => {
                block_of_code = true;
                true
            }
            Instruction::End => {
                block_of_code = false;
                false
            }
            _ => !block_of_code,
        })
        .try_fold(0u64, |cost, instruction| {
            rules
                .instruction_cost(instruction)
                .and_then(|c| cost.checked_add(c.into()))
        }) {
        Some(c) => c,
        None => return Err(mbuilder.build()),
    };

    let cost_push_arg = match rules.instruction_cost(&Instruction::I32Const(0)) {
        Some(c) => c as u64,
        None => return Err(mbuilder.build()),
    };

    let cost_call = match rules.instruction_cost(&Instruction::Call(0)) {
        Some(c) => c as u64,
        None => return Err(mbuilder.build()),
    };

    let cost_local_var = rules.call_per_local_cost() as u64;

    let cost = cost_push_arg + cost_call + cost_local_var + cost_blocks;

    // the cost is added to gas_to_charge which cannot
    // exceed u32::MAX value. This check ensures
    // there is no u64 overflow.
    if cost > u64::MAX - u64::from(u32::MAX) {
        return Err(mbuilder.build());
    }

    // update cost for 'gas_charge' function itself
    let cost_instr = elements
        .iter_mut()
        .find(|i| **i == Instruction::I64Const(GAS_CHARGE_COST_PLACEHOLDER))
        .expect("Const for cost of the fn not found");
    *cost_instr = Instruction::I64Const(cost as i64);

    // gas_charge function
    mbuilder.push_function(
        builder::function()
            .signature()
            .with_param(ValueType::I32)
            .build()
            .body()
            .with_locals(vec![Local::new(1, ValueType::I64)])
            .with_instructions(Instructions::new(elements))
            .build()
            .build(),
    );

    // back to plain module
    let module = mbuilder.build();

    gas_metering::post_injection_handler(module, rules, gas_charge_index, out_of_gas_index, 1)
}
