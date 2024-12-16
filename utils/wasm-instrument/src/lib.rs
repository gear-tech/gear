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

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::items_after_test_module)]
#![allow(clippy::result_large_err)]

extern crate alloc;
extern crate core;

pub use crate::{gas_metering::Rules, syscalls::SyscallName};
pub use module::Module;

use crate::{
    module::{ConstExpr, Function, Global, ModuleBuilder},
    stack_limiter::InjectionConfig,
};
use alloc::vec;
use wasmparser::{
    BlockType, Export, ExternalKind, FuncType, GlobalType, Import, Operator, TypeRef, ValType,
};

mod gas_metering;
mod module;
mod stack_limiter;
#[cfg(test)]
mod tests;

pub mod syscalls;

// TODO #3057
/// Gas global export name in WASM module.
pub const GLOBAL_NAME_GAS: &str = "gear_gas";

/// `__gear_stack_end` export is inserted by wasm-proc or wasm-builder,
/// it indicates the end of program stack memory.
pub const STACK_END_EXPORT_NAME: &str = "__gear_stack_end";
/// `__gear_stack_height` export is inserted by gwasm-instrument,
/// it points to stack height global that is used by
/// [`gwasm_instrument::stack_limiter`].
pub const STACK_HEIGHT_EXPORT_NAME: &str = "__gear_stack_height";

/// System break code for [`SyscallName::SystemBreak`] syscall.
#[derive(Debug, Clone, Copy)]
pub enum SystemBreakCode {
    OutOfGas = 0,
    StackLimitExceeded = 1,
}

/// The error type returned when a conversion from `i32` or `u32` to
/// [`SystemBreakCode`] fails.
#[derive(Clone, Debug, derive_more::Display)]
#[display(fmt = "Unsupported system break code")]
pub struct SystemBreakCodeTryFromError;

impl TryFrom<i32> for SystemBreakCode {
    type Error = SystemBreakCodeTryFromError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::OutOfGas),
            1 => Ok(Self::StackLimitExceeded),
            _ => Err(SystemBreakCodeTryFromError),
        }
    }
}

impl TryFrom<u32> for SystemBreakCode {
    type Error = SystemBreakCodeTryFromError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        SystemBreakCode::try_from(value as i32)
    }
}

/// WASM module instrumentation error.
#[derive(Debug, PartialEq, Eq, derive_more::Display)]
pub enum InstrumentationError {
    /// Error occurred during injecting `gr_system_break` import.
    #[display(fmt = "The WASM module already has `gr_system_break` import")]
    SystemBreakImportAlreadyExists,
    /// Error occurred during stack height instrumentation.
    #[display(fmt = "Failed to inject stack height limits")]
    StackLimitInjection,
    /// Error occurred during injecting `gear_gas` global.
    #[display(fmt = "The WASM module already has `gear_gas` global")]
    GasGlobalAlreadyExists,
    /// Error occurred during calculating the cost of the `gas_charge` function.
    #[display(
        fmt = "An overflow occurred while calculating the cost of the `gas_charge` function"
    )]
    CostCalculationOverflow,
    /// Error occurred while trying to get the instruction cost.
    #[display(fmt = "Failed to get instruction cost")]
    InstructionCostNotFound,
    /// Error occurred during injecting gas metering instructions.
    ///
    /// This might be due to program contained unsupported/non-deterministic
    /// instructions (floats, memory grow, etc.).
    #[display(fmt = "Failed to inject instructions for gas metrics: may be in case \
        program contains unsupported instructions (floats, memory grow, etc.)")]
    GasInjection,
}

/// This is an auxiliary builder that allows to instrument WASM module.
pub struct InstrumentationBuilder<'a, R, GetRulesFn>
where
    R: Rules,
    GetRulesFn: FnMut(&Module) -> R,
{
    /// name of module to import syscalls
    module_name: &'a str,
    /// configuration of stack_limiter
    stack_limiter: Option<(u32, bool)>,
    /// configuration of gas limiter
    gas_limiter: Option<GetRulesFn>,
}

impl<'a, R, GetRulesFn> InstrumentationBuilder<'a, R, GetRulesFn>
where
    R: Rules,
    GetRulesFn: FnMut(&Module) -> R,
{
    /// Creates a new [`InstrumentationBuilder`] with the given module name to
    /// import syscalls.
    pub fn new(module_name: &'a str) -> Self {
        Self {
            module_name,
            stack_limiter: None,
            gas_limiter: None,
        }
    }

    /// Whether to insert a stack limiter into WASM module.
    pub fn with_stack_limiter(&mut self, stack_limit: u32, export_stack_height: bool) -> &mut Self {
        self.stack_limiter = Some((stack_limit, export_stack_height));
        self
    }

    /// Whether to insert a gas limiter into WASM module.
    pub fn with_gas_limiter(&mut self, get_gas_rules: GetRulesFn) -> &mut Self {
        self.gas_limiter = Some(get_gas_rules);
        self
    }

    /// Performs instrumentation of a given WASM module depending
    /// on the parameters with which the [`InstrumentationBuilder`] was created.
    pub fn instrument(&mut self, module: Module<'a>) -> Result<Module<'a>, InstrumentationError> {
        if let (None, None) = (self.stack_limiter, &self.gas_limiter) {
            return Ok(module);
        }

        let (gr_system_break_index, mut module) =
            inject_system_break_import(module, self.module_name)?;

        if let Some((stack_limit, export_stack_height)) = self.stack_limiter {
            let injection_config = InjectionConfig {
                stack_limit,
                injection_fn: |_| {
                    [
                        Operator::I32Const {
                            value: SystemBreakCode::StackLimitExceeded as i32,
                        },
                        Operator::Call {
                            function_index: gr_system_break_index,
                        },
                    ]
                },
                stack_height_export_name: export_stack_height.then_some(STACK_HEIGHT_EXPORT_NAME),
            };

            module = stack_limiter::inject_with_config(module, injection_config)
                .map_err(|_| InstrumentationError::StackLimitInjection)?;
        }

        if let Some(ref mut get_gas_rules) = self.gas_limiter {
            let gas_rules = get_gas_rules(&module);
            module = inject_gas_limiter(module, &gas_rules, gr_system_break_index)?;
        }

        Ok(module)
    }
}

fn inject_system_break_import<'a>(
    module: Module<'a>,
    break_module_name: &'a str,
) -> Result<(u32, Module<'a>), InstrumentationError> {
    if module
        .import_section()
        .map(|section| {
            section.iter().any(|entry| {
                entry.module == break_module_name && entry.name == SyscallName::SystemBreak.to_str()
            })
        })
        .unwrap_or(false)
    {
        return Err(InstrumentationError::SystemBreakImportAlreadyExists);
    }

    let mut mbuilder = ModuleBuilder::from_module(module);
    // fn gr_system_break(code: u32) -> !;
    let import_idx = mbuilder.push_type(FuncType::new([ValType::I32], []));

    // back to plain module
    mbuilder.push_import(Import {
        module: break_module_name,
        name: SyscallName::SystemBreak.to_str(),
        ty: TypeRef::Func(import_idx),
    });

    let import_count = mbuilder
        .as_module()
        .import_count(|ty| matches!(ty, TypeRef::Func(_)));
    let inserted_index = import_count as u32 - 1;

    let module = mbuilder
        .rewrite_sections_after_insertion(inserted_index, 1)
        .expect("Failed to rewrite sections")
        .build();

    Ok((inserted_index, module))
}

fn inject_gas_limiter<'a, R: Rules>(
    module: Module<'a>,
    rules: &R,
    gr_system_break_index: u32,
) -> Result<Module<'a>, InstrumentationError> {
    if module
        .export_section()
        .map(|section| section.iter().any(|entry| entry.name == GLOBAL_NAME_GAS))
        .unwrap_or(false)
    {
        return Err(InstrumentationError::GasGlobalAlreadyExists);
    }

    let gas_charge_index = module.functions_space();
    let gas_index = module.globals_space() as u32;

    let mut mbuilder = ModuleBuilder::from_module(module);

    mbuilder.push_global(Global {
        ty: GlobalType {
            content_type: ValType::I64,
            mutable: true,
            shared: false,
        },
        init_expr: ConstExpr {
            instructions: vec![Operator::I64Const { value: 0 }],
        },
    });

    mbuilder.push_export(Export {
        name: GLOBAL_NAME_GAS,
        kind: ExternalKind::Global,
        index: gas_index,
    });

    // This const is introduced to avoid future errors in code if some other
    // `I64Const` instructions appear in gas charge function body.
    const GAS_CHARGE_COST_PLACEHOLDER: i64 = 1248163264128;

    let mut elements = vec![
        // I. Put global with value of current gas counter of any type.
        Operator::GlobalGet {
            global_index: gas_index,
        },
        // II. Calculating total gas to charge as sum of:
        //  - `gas_charge(..)` argument;
        //  - `gas_charge(..)` call cost.
        //
        // Setting the sum into local with index 1 with keeping it on stack.
        Operator::LocalGet { local_index: 0 },
        Operator::I64ExtendI32U,
        Operator::I64Const {
            value: GAS_CHARGE_COST_PLACEHOLDER,
        },
        Operator::I64Add,
        Operator::LocalTee { local_index: 1 },
        // III. Validating left amount of gas.
        //
        // In case of requested value is bigger than actual gas counter value,
        // than we call `out_of_gas()` that will terminate execution.
        Operator::I64LtU,
        Operator::If {
            blockty: BlockType::Empty,
        },
        Operator::I32Const {
            value: SystemBreakCode::OutOfGas as i32,
        },
        Operator::Call {
            function_index: gr_system_break_index,
        },
        Operator::End,
        // IV. Calculating new global value by subtraction.
        //
        // Result is stored back into global.
        Operator::GlobalGet {
            global_index: gas_index,
        },
        Operator::LocalGet { local_index: 1 },
        Operator::I64Sub,
        Operator::GlobalSet {
            global_index: gas_index,
        },
        // V. Ending `gas_charge()` function.
        Operator::End,
    ];

    // determine cost for successful execution
    let mut block_of_code = false;

    let cost_blocks = elements
        .iter()
        .filter(|instruction| match instruction {
            Operator::If { .. } => {
                block_of_code = true;
                true
            }
            Operator::End => {
                block_of_code = false;
                false
            }
            _ => !block_of_code,
        })
        .try_fold(0u64, |cost, instruction| {
            rules
                .instruction_cost(instruction)
                .and_then(|c| cost.checked_add(c.into()))
        })
        .ok_or(InstrumentationError::CostCalculationOverflow)?;

    let cost_push_arg = rules
        .instruction_cost(&Operator::I32Const { value: 0 })
        .map(|c| c as u64)
        .ok_or(InstrumentationError::InstructionCostNotFound)?;

    let cost_call = rules
        .instruction_cost(&Operator::Call { function_index: 0 })
        .map(|c| c as u64)
        .ok_or(InstrumentationError::InstructionCostNotFound)?;

    let cost_local_var = rules.call_per_local_cost() as u64;

    let cost = cost_push_arg + cost_call + cost_local_var + cost_blocks;

    // the cost is added to gas_to_charge which cannot
    // exceed u32::MAX value. This check ensures
    // there is no u64 overflow.
    if cost > u64::MAX - u64::from(u32::MAX) {
        return Err(InstrumentationError::CostCalculationOverflow);
    }

    // update cost for 'gas_charge' function itself
    let cost_instr = elements
        .iter_mut()
        .find(|i| {
            **i == Operator::I64Const {
                value: GAS_CHARGE_COST_PLACEHOLDER,
            }
        })
        .expect("Const for cost of the fn not found");
    *cost_instr = Operator::I64Const { value: cost as i64 };

    // gas_charge function
    mbuilder.push_type(FuncType::new([ValType::I32], []));
    mbuilder.push_function(Function {
        locals: vec![(1, ValType::I64)],
        instructions: elements,
    });

    // back to plain module
    let module = mbuilder.build();

    gas_metering::post_injection_handler(module, rules, gas_charge_index)
        .map_err(|_| InstrumentationError::GasInjection)
}
