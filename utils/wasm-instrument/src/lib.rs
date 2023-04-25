// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

extern crate alloc;

use alloc::vec;

use wasm_instrument::{
    gas_metering::{self, Rules},
    parity_wasm::{
        builder,
        elements::{self, Instruction, ValueType},
    },
};

use crate::syscalls::SysCallName;
pub use wasm_instrument::{self, parity_wasm};

#[cfg(test)]
mod tests;

pub mod syscalls;

pub const GLOBAL_NAME_GAS: &str = "gear_gas";
pub const GLOBAL_NAME_ALLOWANCE: &str = "gear_allowance";
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
                entry.module() == gas_module_name
                    && (entry.field() == SysCallName::OutOfGas.to_str()
                        || entry.field() == SysCallName::OutOfAllowance.to_str())
            })
        })
        .unwrap_or(false)
    {
        return Err(module);
    }

    if module
        .export_section()
        .map(|section| {
            section.entries().iter().any(|entry| {
                entry.field() == GLOBAL_NAME_ALLOWANCE || entry.field() == GLOBAL_NAME_GAS
            })
        })
        .unwrap_or(false)
    {
        return Err(module);
    }

    let mut mbuilder = builder::from_module(module);

    // fn out_of_...() -> ();
    let import_sig = mbuilder.push_signature(builder::signature().build_sig());

    mbuilder.push_import(
        builder::import()
            .module(gas_module_name)
            .field(SysCallName::OutOfGas.to_str())
            .external()
            .func(import_sig)
            .build(),
    );

    mbuilder.push_import(
        builder::import()
            .module(gas_module_name)
            .field(SysCallName::OutOfAllowance.to_str())
            .external()
            .func(import_sig)
            .build(),
    );

    // back to plain module
    let module = mbuilder.build();

    let import_count = module.import_count(elements::ImportCountType::Function);
    let out_of_gas_index = import_count as u32 - 2;
    let out_of_allowance_index = import_count as u32 - 1;

    let gas_charge_index = module.functions_space();
    let gas_index = module.globals_space() as u32;
    let allowance_index = gas_index + 1;

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
            .field(GLOBAL_NAME_ALLOWANCE)
            .internal()
            .global(allowance_index)
            .build(),
    );

    // TODO: #1706
    let mut elements = vec![
        // check if there is enough gas
        Instruction::GetGlobal(gas_index),
        // calculate gas_to_charge + cost_for_func
        // {
        Instruction::GetLocal(0),
        Instruction::I64ExtendUI32,
        Instruction::I64Const(i64::MAX),
        Instruction::I64Add,
        // }
        // if gas < (gas_to_charge + cost_for_func)
        Instruction::I64LtU,
        Instruction::If(elements::BlockType::NoResult),
        Instruction::Call(out_of_gas_index),
        Instruction::Unreachable,
        Instruction::End,
        // update gas
        Instruction::GetGlobal(gas_index),
        // calculate gas_to_charge + cost_for_func
        // {
        Instruction::GetLocal(0),
        Instruction::I64ExtendUI32,
        Instruction::I64Const(i64::MAX),
        Instruction::I64Add,
        // }
        // gas -= (gas_to_charge + cost_for_func)
        // {
        Instruction::I64Sub,
        Instruction::SetGlobal(gas_index),
        // }
        // check if there is enough gas allowance
        Instruction::GetGlobal(allowance_index),
        // calculate gas_to_charge + cost_for_func
        // {
        Instruction::GetLocal(0),
        Instruction::I64ExtendUI32,
        Instruction::I64Const(i64::MAX),
        Instruction::I64Add,
        // }
        // if allowance < (gas_to_charge + cost_for_func)
        Instruction::I64LtU,
        Instruction::If(elements::BlockType::NoResult),
        Instruction::Call(out_of_allowance_index),
        Instruction::Unreachable,
        Instruction::End,
        // update gas allowance
        Instruction::GetGlobal(allowance_index),
        // calculate gas_to_charge + cost_for_func
        // {
        Instruction::GetLocal(0),
        Instruction::I64ExtendUI32,
        Instruction::I64Const(i64::MAX),
        Instruction::I64Add,
        // }
        // allowance -= (gas_to_charge + cost_for_func)
        // {
        Instruction::I64Sub,
        Instruction::SetGlobal(allowance_index),
        // }
        Instruction::End,
    ];

    // determine cost for successful execution
    let cost_blocks = match elements
        .iter()
        .take(7)
        // block with update instructions
        .chain(elements.iter().skip(10).take(7))
        .try_fold(0u64, |cost, instruction| {
            rules
                .instruction_cost(instruction)
                .and_then(|c| cost.checked_add(c.into()))
        }) {
        Some(c) => 2 * c,
        None => return Err(mbuilder.build()),
    };

    let cost_call = match rules.instruction_cost(&Instruction::Call(0)) {
        Some(c) => c as u64,
        None => return Err(mbuilder.build()),
    };

    let cost = cost_call + cost_blocks;
    // the cost is added to gas_to_charge which cannot
    // exceed u32::MAX value. This check ensures
    // there is no u64 overflow.
    if cost > u64::MAX - u64::from(u32::MAX) {
        return Err(mbuilder.build());
    }

    // update cost for 'gas_charge' function itself
    for instruction in elements
        .iter_mut()
        .filter(|i| matches!(i, Instruction::I64Const(_)))
    {
        *instruction = Instruction::I64Const(cost as i64);
    }

    // gas_charge function
    mbuilder.push_function(
        builder::function()
            .signature()
            .with_param(ValueType::I32)
            .build()
            .body()
            .with_instructions(elements::Instructions::new(elements))
            .build()
            .build(),
    );

    // back to plain module
    let module = mbuilder.build();

    gas_metering::post_injection_handler(module, rules, gas_charge_index, out_of_gas_index, 2)
}
