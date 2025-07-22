// This file is part of Gear.
//
// Copyright (C) 2017-2024 Parity Technologies.
// Copyright (C) 2025 Gear Technologies Inc.
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

//! Contains the code for the stack height limiter instrumentation.

use crate::{
    Module,
    module::{ConstExpr, Export, Global, Instruction, ModuleBuilder},
};
use alloc::{string::ToString, vec::Vec};
use core::mem;
use max_height::{MaxStackHeightCounter, MaxStackHeightCounterContext};
use wasmparser::{BlockType, FuncType, GlobalType, TypeRef, ValType};

mod max_height;
mod thunk;

pub(crate) struct Context {
    stack_height_global_idx: u32,
    func_stack_costs: Vec<u32>,
    stack_limit: u32,
}

impl Context {
    /// Returns index in a global index space of a stack_height global variable.
    fn stack_height_global_idx(&self) -> u32 {
        self.stack_height_global_idx
    }

    /// Returns `stack_cost` for `func_idx`.
    fn stack_cost(&self, func_idx: u32) -> Option<u32> {
        self.func_stack_costs.get(func_idx as usize).cloned()
    }

    /// Returns stack limit specified by the rules.
    fn stack_limit(&self) -> u32 {
        self.stack_limit
    }
}

/// Inject the instumentation that makes stack overflows deterministic, by introducing
/// an upper bound of the stack size.
///
/// This pass introduces a global mutable variable to track stack height,
/// and instruments all calls with preamble and postamble.
///
/// Stack height is increased prior the call. Otherwise, the check would
/// be made after the stack frame is allocated.
///
/// The preamble is inserted before the call. It increments
/// the global stack height variable with statically determined "stack cost"
/// of the callee. If after the increment the stack height exceeds
/// the limit (specified by the `rules`) then execution traps.
/// Otherwise, the call is executed.
///
/// The postamble is inserted after the call. The purpose of the postamble is to decrease
/// the stack height by the "stack cost" of the callee function.
///
/// Note, that we can't instrument all possible ways to return from the function. The simplest
/// example would be a trap issued by the host function.
/// That means stack height global won't be equal to zero upon the next execution after such trap.
///
/// # Thunks
///
/// Because stack height is increased prior the call few problems arises:
///
/// - Stack height isn't increased upon an entry to the first function, i.e. exported function.
/// - Start function is executed externally (similar to exported functions).
/// - It is statically unknown what function will be invoked in an indirect call.
///
/// The solution for this problems is to generate a intermediate functions, called 'thunks', which
/// will increase before and decrease the stack height after the call to original function, and
/// then make exported function and table entries, start section to point to a corresponding thunks.
///
/// # Stack cost
///
/// Stack cost of the function is calculated as a sum of it's locals
/// and the maximal height of the value stack.
///
/// All values are treated equally, as they have the same size.
///
/// The rationale is that this makes it possible to use the following very naive wasm executor:
///
/// - values are implemented by a union, so each value takes a size equal to the size of the largest
///   possible value type this union can hold. (In MVP it is 8 bytes)
/// - each value from the value stack is placed on the native stack.
/// - each local variable and function argument is placed on the native stack.
/// - arguments pushed by the caller are copied into callee stack rather than shared between the
///   frames.
/// - upon entry into the function entire stack frame is allocated.
pub fn inject(module: Module, stack_limit: u32) -> Result<Module, &'static str> {
    inject_with_config(
        module,
        InjectionConfig {
            stack_limit,
            injection_fn: |_| [Instruction::Unreachable],
            stack_height_export_name: None,
        },
    )
}

/// Represents the injection configuration. See [`inject_with_config`] for more details.
pub struct InjectionConfig<'a, I, F>
where
    I: IntoIterator<Item = Instruction>,
    I::IntoIter: ExactSizeIterator + Clone,
    F: Fn(&FuncType) -> I,
{
    pub stack_limit: u32,
    pub injection_fn: F,
    pub stack_height_export_name: Option<&'a str>,
}

/// Same as the [`inject`] function, but allows to configure exit instructions when the stack limit
/// is reached and the export name of the stack height global.
pub fn inject_with_config<I>(
    module: Module,
    injection_config: InjectionConfig<I, impl Fn(&FuncType) -> I>,
) -> Result<Module, &'static str>
where
    I: IntoIterator<Item = Instruction>,
    I::IntoIter: ExactSizeIterator + Clone,
{
    let InjectionConfig {
        stack_limit,
        injection_fn,
        stack_height_export_name,
    } = injection_config;

    let (mut module, stack_height_global_idx) =
        generate_stack_height_global(module, stack_height_export_name);
    let mut ctx = Context {
        stack_height_global_idx,
        func_stack_costs: compute_stack_costs(&module, &injection_fn)?,
        stack_limit,
    };

    instrument_functions(&mut ctx, &mut module, &injection_fn)?;
    let module = thunk::generate_thunks(&mut ctx, module, &injection_fn)?;

    Ok(module)
}

/// Generate a new global that will be used for tracking current stack height.
fn generate_stack_height_global(
    module: Module,
    stack_height_export_name: Option<&str>,
) -> (Module, u32) {
    let global_entry = Global {
        ty: GlobalType {
            content_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        init_expr: ConstExpr::i32_value(0),
    };

    let mut mbuilder = ModuleBuilder::from_module(module);

    let stack_height_global_idx = mbuilder.push_global(global_entry);

    if let Some(stack_height_export_name) = stack_height_export_name {
        mbuilder.push_export(Export::global(
            stack_height_export_name.to_string(),
            stack_height_global_idx,
        ));
    }

    (mbuilder.build(), stack_height_global_idx)
}

/// Calculate stack costs for all functions.
///
/// Returns a vector with a stack cost for each function, including imports.
fn compute_stack_costs<I>(
    module: &Module,
    injection_fn: impl Fn(&FuncType) -> I,
) -> Result<Vec<u32>, &'static str>
where
    I::IntoIter: ExactSizeIterator + Clone,
    I: IntoIterator<Item = Instruction>,
{
    let functions_space = module
        .functions_space()
        .try_into()
        .map_err(|_| "Can't convert functions space to u32")?;

    // Don't create context when there are no functions (this will fail).
    if functions_space == 0 {
        return Ok(Vec::new());
    }

    // This context already contains the module, number of imports and section references.
    // So we can use it to optimize access to these objects.
    let context = MaxStackHeightCounterContext::new(module)?;

    (0..functions_space)
        .map(|func_idx| {
            if func_idx < context.func_imports {
                // We can't calculate stack_cost of the import functions.
                Ok(0)
            } else {
                compute_stack_cost(context, func_idx, &injection_fn)
            }
        })
        .collect()
}

/// Stack cost of the given *defined* function is the sum of it's locals count (that is,
/// number of arguments plus number of local variables) and the maximal stack
/// height.
fn compute_stack_cost<I: IntoIterator<Item = Instruction>>(
    context: MaxStackHeightCounterContext<'_>,
    func_idx: u32,
    injection_fn: impl Fn(&FuncType) -> I,
) -> Result<u32, &'static str>
where
    I::IntoIter: ExactSizeIterator + Clone,
{
    // To calculate the cost of a function we need to convert index from
    // function index space to defined function spaces.
    let defined_func_idx = func_idx
        .checked_sub(context.func_imports)
        .ok_or("This should be a index of a defined function")?;

    let body = context
        .code_section
        .get(defined_func_idx as usize)
        .ok_or("Function body is out of bounds")?;

    let mut locals_count: u32 = 0;
    for local_group in &body.locals {
        locals_count = locals_count
            .checked_add(local_group.0)
            .ok_or("Overflow in local count")?;
    }

    let max_stack_height = MaxStackHeightCounter::new_with_context(context, injection_fn)
        .count_instrumented_calls(true)
        .compute_for_defined_func(defined_func_idx)?;

    locals_count
        .checked_add(max_stack_height)
        .ok_or("Overflow in adding locals_count and max_stack_height")
}

fn instrument_functions<I: IntoIterator<Item = Instruction>>(
    ctx: &mut Context,
    module: &mut Module,
    injection_fn: impl Fn(&FuncType) -> I,
) -> Result<(), &'static str>
where
    I::IntoIter: ExactSizeIterator + Clone,
{
    if ctx.func_stack_costs.is_empty() {
        return Ok(());
    }

    // Func stack costs collection is not empty, so stack height counter has counted costs
    // for module with non empty function and type sections.
    let types = module
        .type_section
        .as_ref()
        .expect("checked earlier")
        .clone();
    let functions = module
        .function_section
        .as_ref()
        .expect("checked earlier")
        .clone();

    if let Some(code_section) = &mut module.code_section {
        for (func_idx, func_body) in code_section.iter_mut().enumerate() {
            let opcodes = &mut func_body.instructions;

            let signature_index = functions[func_idx];
            let signature = &types[signature_index as usize];

            instrument_function(ctx, opcodes, signature, &injection_fn)?;
        }
    }

    Ok(())
}

/// This function searches `call` instructions and wrap each call
/// with preamble and postamble.
///
/// Before:
///
/// ```text
/// local.get 0
/// local.get 1
/// call 228
/// drop
/// ```
///
/// After:
///
/// ```text
/// local.get 0
/// local.get 1
///
/// < ... preamble ... >
///
/// call 228
///
/// < .. postamble ... >
///
/// drop
/// ```
fn instrument_function<I: IntoIterator<Item = Instruction>>(
    ctx: &mut Context,
    func: &mut Vec<Instruction>,
    signature: &FuncType,
    injection_fn: impl Fn(&FuncType) -> I,
) -> Result<(), &'static str>
where
    I::IntoIter: ExactSizeIterator + Clone,
{
    use Instruction::*;

    struct InstrumentCall {
        offset: usize,
        callee: u32,
        cost: u32,
    }

    let calls: Vec<_> = func
        .iter()
        .enumerate()
        .filter_map(|(offset, instruction)| {
            if let &Call(function_index) = instruction {
                ctx.stack_cost(function_index).and_then(|cost| {
                    if cost > 0 {
                        Some(InstrumentCall {
                            callee: function_index,
                            offset,
                            cost,
                        })
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .collect();

    // To pre-allocate memory, we need to count `8 + N + 6 - 1`, i.e. `13 + N`.
    // We need to subtract one because it is assumed that we already have the original call
    // instruction in `func.elements()`. See `instrument_call` function for details.
    let body_of_condition = injection_fn(signature).into_iter();
    let len = func.len() + calls.len() * (13 + body_of_condition.len());
    let original_instrs = mem::replace(func, Vec::with_capacity(len));
    let new_instrs = func;

    let mut calls = calls.into_iter().peekable();
    for (original_pos, instr) in original_instrs.into_iter().enumerate() {
        // whether there is some call instruction at this position that needs to be instrumented
        let did_instrument = if let Some(call) = calls.peek() {
            if call.offset == original_pos {
                instrument_call(
                    new_instrs,
                    call.callee,
                    call.cost as i32,
                    ctx.stack_height_global_idx(),
                    ctx.stack_limit(),
                    body_of_condition.clone(),
                    [],
                );
                true
            } else {
                false
            }
        } else {
            false
        };

        if did_instrument {
            calls.next();
        } else {
            new_instrs.push(instr);
        }
    }

    if calls.next().is_some() {
        return Err("Not all calls were used");
    }

    Ok(())
}

/// This function generates preamble and postamble.
fn instrument_call(
    instructions: &mut Vec<Instruction>,
    callee_idx: u32,
    callee_stack_cost: i32,
    stack_height_global_idx: u32,
    stack_limit: u32,
    body_of_condition: impl IntoIterator<Item = Instruction>,
    arguments: impl IntoIterator<Item = Instruction>,
) {
    use Instruction::*;

    // 8 + body_of_condition.len() + 1 instructions
    generate_preamble(
        instructions,
        callee_stack_cost,
        stack_height_global_idx,
        stack_limit,
        body_of_condition,
    );

    // arguments.len() instructions
    instructions.extend(arguments);

    // Original call, 1 instruction
    instructions.push(Call(callee_idx));

    // 4 instructions
    generate_postamble(instructions, callee_stack_cost, stack_height_global_idx);
}

/// This function generates preamble.
fn generate_preamble(
    instructions: &mut Vec<Instruction>,
    callee_stack_cost: i32,
    stack_height_global_idx: u32,
    stack_limit: u32,
    body_of_condition: impl IntoIterator<Item = Instruction>,
) {
    use Instruction::*;

    // 8 instructions
    instructions.extend_from_slice(&[
        // stack_height += stack_cost(F)
        GlobalGet(stack_height_global_idx),
        I32Const(callee_stack_cost),
        I32Add,
        GlobalSet(stack_height_global_idx),
        // if stack_counter > LIMIT: unreachable or custom instructions
        GlobalGet(stack_height_global_idx),
        I32Const(stack_limit as i32),
        I32GtU,
        If(BlockType::Empty),
    ]);

    // body_of_condition.len() instructions
    instructions.extend(body_of_condition);

    // 1 instruction
    instructions.push(End);
}

/// This function generates postamble.
#[inline]
fn generate_postamble(
    instructions: &mut Vec<Instruction>,
    callee_stack_cost: i32,
    stack_height_global_idx: u32,
) {
    use Instruction::*;

    // 4 instructions
    instructions.extend_from_slice(&[
        // stack_height -= stack_cost(F)
        GlobalGet(stack_height_global_idx),
        I32Const(callee_stack_cost),
        I32Sub,
        GlobalSet(stack_height_global_idx),
    ]);
}

fn resolve_func_type(func_idx: u32, module: &Module) -> Result<&FuncType, &'static str> {
    let types = module.type_section.as_deref().unwrap_or_default();
    let functions = module.function_section.as_deref().unwrap_or_default();

    let func_imports = module.import_count(|ty| matches!(ty, TypeRef::Func(_)));
    let sig_idx = if func_idx < func_imports as u32 {
        module
            .import_section
            .as_ref()
            .expect("function import count is not zero; import section must exists; qed")
            .iter()
            .filter_map(|entry| match entry.ty {
                TypeRef::Func(idx) => Some(idx),
                _ => None,
            })
            .nth(func_idx as usize)
            .expect(
                "func_idx is less than function imports count;
				nth function import must be `Some`;
				qed",
            )
    } else {
        functions
            .get(func_idx as usize - func_imports)
            .copied()
            .ok_or("Function at the specified index is not defined")?
    };
    let ty = types
        .get(sig_idx as usize)
        .ok_or("The signature as specified by a function isn't defined")?;
    Ok(ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::parse_wat;

    fn validate_module(module: Module) {
        let binary = module.serialize().expect("Failed to serialize");
        wasmparser::validate(&binary).expect("Invalid module");
    }

    #[test]
    fn test_with_params_and_result() {
        let module = parse_wat(
            r#"
(module
	(func (export "i32.add") (param i32 i32) (result i32)
		local.get 0
	local.get 1
	i32.add
	)
)
"#,
        );

        let module = inject(module, 1024).expect("Failed to inject stack counter");
        validate_module(module);
    }
}
