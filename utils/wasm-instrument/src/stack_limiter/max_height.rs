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

use super::{instrument_call, resolve_func_type};
use crate::{
    Module,
    module::{CodeSection, FuncSection, Instruction, TypeSection},
};
use alloc::vec::Vec;
use wasmparser::{BlockType, FuncType, TypeRef};

// The cost in stack items that should be charged per call of a function. This is
// is a static cost that is added to each function call. This makes sense because even
// if a function does not use any parameters or locals some stack space on the host
// machine might be consumed to hold some context.
const ACTIVATION_FRAME_COST: u32 = 2;

/// Control stack frame.
#[derive(Debug)]
struct Frame {
    /// Stack becomes polymorphic only after an instruction that
    /// never passes control further was executed.
    is_polymorphic: bool,

    /// Count of values which will be pushed after the exit
    /// from the current block.
    end_arity: u32,

    /// Count of values which should be popped upon a branch to
    /// this frame.
    ///
    /// This might be different from `end_arity` since branch
    /// to the loop header can't take any values.
    branch_arity: u32,

    /// Stack height before entering in the block.
    start_height: u32,
}

/// This is a compound stack that abstracts tracking height of the value stack
/// and manipulation of the control stack.
struct Stack {
    height: u32,
    control_stack: Vec<Frame>,
}

impl Stack {
    fn new() -> Self {
        Self {
            height: ACTIVATION_FRAME_COST,
            control_stack: Vec::new(),
        }
    }

    /// Returns current height of the value stack.
    fn height(&self) -> u32 {
        self.height
    }

    /// Returns a reference to a frame by specified depth relative to the top of
    /// control stack.
    fn frame(&self, rel_depth: u32) -> Result<&Frame, &'static str> {
        let control_stack_height: usize = self.control_stack.len();
        let last_idx = control_stack_height
            .checked_sub(1)
            .ok_or("control stack is empty")?;
        let idx = last_idx
            .checked_sub(rel_depth as usize)
            .ok_or("control stack out-of-bounds")?;
        Ok(&self.control_stack[idx])
    }

    /// Mark successive instructions as unreachable.
    ///
    /// This effectively makes stack polymorphic.
    fn mark_unreachable(&mut self) -> Result<(), &'static str> {
        let top_frame = self
            .control_stack
            .last_mut()
            .ok_or("stack must be non-empty")?;
        top_frame.is_polymorphic = true;
        Ok(())
    }

    /// Push control frame into the control stack.
    fn push_frame(&mut self, frame: Frame) {
        self.control_stack.push(frame);
    }

    /// Pop control frame from the control stack.
    ///
    /// Returns `Err` if the control stack is empty.
    fn pop_frame(&mut self) -> Result<Frame, &'static str> {
        self.control_stack.pop().ok_or("stack must be non-empty")
    }

    /// Truncate the height of value stack to the specified height.
    fn trunc(&mut self, new_height: u32) {
        self.height = new_height;
    }

    /// Push specified number of values into the value stack.
    ///
    /// Returns `Err` if the height overflow usize value.
    fn push_values(&mut self, value_count: u32) -> Result<(), &'static str> {
        self.height = self
            .height
            .checked_add(value_count)
            .ok_or("stack overflow")?;
        Ok(())
    }

    /// Pop specified number of values from the value stack.
    ///
    /// Returns `Err` if the stack happen to be negative value after
    /// values popped.
    fn pop_values(&mut self, value_count: u32) -> Result<(), &'static str> {
        if value_count == 0 {
            return Ok(());
        }
        {
            let top_frame = self.frame(0)?;
            if self.height == top_frame.start_height {
                // It is an error to pop more values than was pushed in the current frame
                // (ie pop values pushed in the parent frame), unless the frame became
                // polymorphic.
                return if top_frame.is_polymorphic {
                    Ok(())
                } else {
                    return Err("trying to pop more values than pushed");
                };
            }
        }

        self.height = self
            .height
            .checked_sub(value_count)
            .ok_or("stack underflow")?;

        Ok(())
    }
}

/// This is a helper context that is used by [`MaxStackHeightCounter`].
#[derive(Clone, Copy)]
pub(crate) struct MaxStackHeightCounterContext<'m> {
    pub module: &'m Module,
    pub func_imports: u32,
    pub func_section: &'m FuncSection,
    pub code_section: &'m CodeSection,
    pub type_section: &'m TypeSection,
}

impl<'m> MaxStackHeightCounterContext<'m> {
    pub fn new(module: &'m Module) -> Result<Self, &'static str> {
        Ok(Self {
            module,
            func_imports: module
                .import_count(|ty| matches!(ty, TypeRef::Func(_)))
                .try_into()
                .map_err(|_| "Can't convert func imports count to u32")?,
            func_section: module
                .function_section
                .as_ref()
                .ok_or("No function section")?,
            code_section: module.code_section.as_ref().ok_or("No code section")?,
            type_section: module.type_section.as_ref().ok_or("No type section")?,
        })
    }
}

/// This is a counter for the maximum stack height with the ability to take into account the
/// overhead that is added by the [`instrument_call!`] function.
pub(crate) struct MaxStackHeightCounter<'m, I, F>
where
    I: IntoIterator<Item = Instruction>,
    I::IntoIter: ExactSizeIterator + Clone,
    F: Fn(&FuncType) -> I,
{
    context: MaxStackHeightCounterContext<'m>,
    stack: Stack,
    max_height: u32,
    injection_fn: F,
    count_instrumented_calls: bool,
}

impl<'m, I, F> MaxStackHeightCounter<'m, I, F>
where
    I: IntoIterator<Item = Instruction>,
    I::IntoIter: ExactSizeIterator + Clone,
    F: Fn(&FuncType) -> I,
{
    /// Creates a [`MaxStackHeightCounter`] from [`MaxStackHeightCounterContext`].
    pub fn new_with_context(
        context: MaxStackHeightCounterContext<'m>,
        injection_fn: F,
    ) -> MaxStackHeightCounter<'m, I, F> {
        Self {
            context,
            stack: Stack::new(),
            max_height: 0,
            injection_fn,
            count_instrumented_calls: false,
        }
    }

    /// Should the overhead of the [`instrument_call`] function be taken into account?
    pub fn count_instrumented_calls(mut self, count_instrumented_calls: bool) -> Self {
        self.count_instrumented_calls = count_instrumented_calls;
        self
    }

    /// Tries to calculate the maximum stack height for the `func_idx` defined in the wasm module.
    pub fn compute_for_defined_func(&mut self, func_idx: u32) -> Result<u32, &'static str> {
        let MaxStackHeightCounterContext {
            func_section,
            code_section,
            type_section,
            ..
        } = self.context;

        // Get a signature and a body of the specified function.
        let &func_sig_idx = func_section
            .get(func_idx as usize)
            .ok_or("Function is not found in func section")?;
        let func_signature = type_section
            .get(func_sig_idx as usize)
            .ok_or("Function is not found in func section")?;
        let body = code_section
            .get(func_idx as usize)
            .ok_or("Function body for the index isn't found")?;
        let instructions = &body.instructions;

        self.compute_for_raw_func(func_signature, instructions)
    }

    /// Tries to calculate the maximum stack height for a raw function, which consists of
    /// `func_signature` and `instructions`.
    pub fn compute_for_raw_func(
        &mut self,
        func_signature: &FuncType,
        instructions: &[Instruction],
    ) -> Result<u32, &'static str> {
        // Add implicit frame for the function. Breaks to this frame and execution of
        // the last end should deal with this frame.
        let func_arity = func_signature.results().len() as u32;
        self.stack.push_frame(Frame {
            is_polymorphic: false,
            end_arity: func_arity,
            branch_arity: func_arity,
            start_height: 0,
        });

        for instruction in instructions {
            let maybe_instructions = 'block: {
                if !self.count_instrumented_calls {
                    break 'block None;
                }

                let &Instruction::Call(function_index) = instruction else {
                    break 'block None;
                };

                if function_index < self.context.func_imports {
                    break 'block None;
                }

                let body_of_condition = (self.injection_fn)(func_signature).into_iter();

                let mut instructions = Vec::with_capacity(14 + body_of_condition.len());
                instrument_call(
                    &mut instructions,
                    function_index,
                    0,
                    0,
                    0,
                    body_of_condition,
                    [],
                );

                Some(instructions)
            };

            if let Some(instructions) = maybe_instructions.as_ref() {
                for instruction in instructions {
                    self.process_instruction(instruction, func_arity)?;
                }
            } else {
                self.process_instruction(instruction, func_arity)?;
            }
        }

        Ok(self.max_height)
    }

    /// This function processes all incoming instructions and updates the `self.max_height` field.
    fn process_instruction(
        &mut self,
        opcode: &Instruction,
        func_arity: u32,
    ) -> Result<(), &'static str> {
        use Instruction::*;

        let Self {
            stack, max_height, ..
        } = self;
        let MaxStackHeightCounterContext {
            module,
            type_section,
            ..
        } = self.context;

        // If current value stack is higher than maximal height observed so far,
        // save the new height.
        // However, we don't increase maximal value in unreachable code.
        if stack.height() > *max_height && !stack.frame(0)?.is_polymorphic {
            *max_height = stack.height();
        }

        match opcode {
            Nop => {}
            Block(blockty) | Loop(blockty) | If(blockty) => {
                let end_arity = if *blockty == BlockType::Empty { 0 } else { 1 };
                let branch_arity = if let Loop(_blockty) = *opcode {
                    0
                } else {
                    end_arity
                };
                if let If { .. } = *opcode {
                    stack.pop_values(1)?;
                }
                let height = stack.height();
                stack.push_frame(Frame {
                    is_polymorphic: false,
                    end_arity,
                    branch_arity,
                    start_height: height,
                });
            }
            Else => {
                // The frame at the top should be pushed by `If`. So we leave
                // it as is.
            }
            End => {
                let frame = stack.pop_frame()?;
                stack.trunc(frame.start_height);
                stack.push_values(frame.end_arity)?;
            }
            Unreachable => {
                stack.mark_unreachable()?;
            }
            Br(relative_depth) => {
                // Pop values for the destination block result.
                let target_arity = stack.frame(*relative_depth)?.branch_arity;
                stack.pop_values(target_arity)?;

                // This instruction unconditionally transfers control to the specified block,
                // thus all instruction until the end of the current block is deemed unreachable
                stack.mark_unreachable()?;
            }
            BrIf(relative_depth) => {
                // Pop values for the destination block result.
                let target_arity = stack.frame(*relative_depth)?.branch_arity;
                stack.pop_values(target_arity)?;

                // Pop condition value.
                stack.pop_values(1)?;

                // Push values back.
                stack.push_values(target_arity)?;
            }
            BrTable(targets) => {
                let arity_of_default = stack.frame(targets.default)?.branch_arity;

                // Check that all jump targets have an equal arities.
                for &target in &targets.targets {
                    let arity = stack.frame(target)?.branch_arity;
                    if arity != arity_of_default {
                        return Err("Arity of all jump-targets must be equal");
                    }
                }

                // Because all jump targets have an equal arities, we can just take arity of
                // the default branch.
                stack.pop_values(arity_of_default)?;

                // This instruction doesn't let control flow to go further, since the control flow
                // should take either one of branches depending on the value or the default branch.
                stack.mark_unreachable()?;
            }
            Return => {
                // Pop return values of the function. Mark successive instructions as unreachable
                // since this instruction doesn't let control flow to go further.
                stack.pop_values(func_arity)?;
                stack.mark_unreachable()?;
            }
            Call(function_index) => {
                let ty = resolve_func_type(*function_index, module)?;

                // Pop values for arguments of the function.
                stack.pop_values(ty.params().len() as u32)?;

                // Push result of the function execution to the stack.
                let callee_arity = ty.results().len() as u32;
                stack.push_values(callee_arity)?;
            }
            CallIndirect(type_index) => {
                let ty = type_section
                    .get(*type_index as usize)
                    .ok_or("Type not found")?;

                // Pop the offset into the function table.
                stack.pop_values(1)?;

                // Pop values for arguments of the function.
                stack.pop_values(ty.params().len() as u32)?;

                // Push result of the function execution to the stack.
                let callee_arity = ty.results().len() as u32;
                stack.push_values(callee_arity)?;
            }
            Drop => {
                stack.pop_values(1)?;
            }
            Select => {
                // Pop two values and one condition.
                stack.pop_values(2)?;
                stack.pop_values(1)?;

                // Push the selected value.
                stack.push_values(1)?;
            }
            LocalGet { .. } => {
                stack.push_values(1)?;
            }
            LocalSet { .. } => {
                stack.pop_values(1)?;
            }
            LocalTee { .. } => {
                // This instruction pops and pushes the value, so
                // effectively it doesn't modify the stack height.
                stack.pop_values(1)?;
                stack.push_values(1)?;
            }
            GlobalGet { .. } => {
                stack.push_values(1)?;
            }
            GlobalSet { .. } => {
                stack.pop_values(1)?;
            }
            I32Load { .. }
            | I64Load { .. }
            | I32Load8S { .. }
            | I32Load8U { .. }
            | I32Load16S { .. }
            | I32Load16U { .. }
            | I64Load8S { .. }
            | I64Load8U { .. }
            | I64Load16S { .. }
            | I64Load16U { .. }
            | I64Load32S { .. }
            | I64Load32U { .. } => {
                // These instructions pop the address and pushes the result,
                // which effictively don't modify the stack height.
                stack.pop_values(1)?;
                stack.push_values(1)?;
            }

            I32Store { .. }
            | I64Store { .. }
            | I32Store8 { .. }
            | I32Store16 { .. }
            | I64Store8 { .. }
            | I64Store16 { .. }
            | I64Store32 { .. } => {
                // These instructions pop the address and the value.
                stack.pop_values(2)?;
            }

            MemorySize { .. } => {
                // Pushes current memory size
                stack.push_values(1)?;
            }
            MemoryGrow { .. } => {
                // Grow memory takes the value of pages to grow and pushes
                stack.pop_values(1)?;
                stack.push_values(1)?;
            }

            I32Const { .. } | I64Const { .. } => {
                // These instructions just push the single literal value onto the stack.
                stack.push_values(1)?;
            }

            I32Eqz | I64Eqz => {
                // These instructions pop the value and compare it against zero, and pushes
                // the result of the comparison.
                stack.pop_values(1)?;
                stack.push_values(1)?;
            }

            I32Eq | I32Ne | I32LtS | I32LtU | I32GtS | I32GtU | I32LeS | I32LeU | I32GeS
            | I32GeU | I64Eq | I64Ne | I64LtS | I64LtU | I64GtS | I64GtU | I64LeS | I64LeU
            | I64GeS | I64GeU => {
                // Comparison operations take two operands and produce one result.
                stack.pop_values(2)?;
                stack.push_values(1)?;
            }

            I32Clz | I32Ctz | I32Popcnt | I64Clz | I64Ctz | I64Popcnt => {
                // Unary operators take one operand and produce one result.
                stack.pop_values(1)?;
                stack.push_values(1)?;
            }

            I32Add | I32Sub | I32Mul | I32DivS | I32DivU | I32RemS | I32RemU | I32And | I32Or
            | I32Xor | I32Shl | I32ShrS | I32ShrU | I32Rotl | I32Rotr | I64Add | I64Sub
            | I64Mul | I64DivS | I64DivU | I64RemS | I64RemU | I64And | I64Or | I64Xor | I64Shl
            | I64ShrS | I64ShrU | I64Rotl | I64Rotr => {
                // Binary operators take two operands and produce one result.
                stack.pop_values(2)?;
                stack.push_values(1)?;
            }

            I32WrapI64 | I64ExtendI32S | I64ExtendI32U => {
                // Conversion operators take one value and produce one result.
                stack.pop_values(1)?;
                stack.push_values(1)?;
            }

            I32Extend8S | I32Extend16S | I64Extend8S | I64Extend16S | I64Extend32S => {
                stack.pop_values(1)?;
                stack.push_values(1)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::parse_wat;

    fn compute(func_idx: u32, module: &Module) -> Result<u32, &'static str> {
        MaxStackHeightCounter::new_with_context(MaxStackHeightCounterContext::new(module)?, |_| {
            [Instruction::Unreachable]
        })
        .count_instrumented_calls(true)
        .compute_for_defined_func(func_idx)
    }

    #[test]
    fn simple_test() {
        let module = parse_wat(
            r#"
(module
	(func
		i32.const 1
			i32.const 2
				i32.const 3
				drop
			drop
		drop
	)
)
"#,
        );

        let height = compute(0, &module).unwrap();
        assert_eq!(height, 3 + ACTIVATION_FRAME_COST);
    }

    #[test]
    fn implicit_and_explicit_return() {
        let module = parse_wat(
            r#"
(module
	(func (result i32)
		i32.const 0
		return
	)
)
"#,
        );

        let height = compute(0, &module).unwrap();
        assert_eq!(height, 1 + ACTIVATION_FRAME_COST);
    }

    #[test]
    fn dont_count_in_unreachable() {
        let module = parse_wat(
            r#"
(module
  (memory 0)
  (func (result i32)
	unreachable
	memory.grow
  )
)
"#,
        );

        let height = compute(0, &module).unwrap();
        assert_eq!(height, ACTIVATION_FRAME_COST);
    }

    #[test]
    fn yet_another_test() {
        let module = parse_wat(
            r#"
(module
  (memory 0)
  (func
	;; Push two values and then pop them.
	;; This will make max depth to be equal to 2.
	i32.const 0
	i32.const 1
	drop
	drop

	;; Code after `unreachable` shouldn't have an effect
	;; on the max depth.
	unreachable
	i32.const 0
	i32.const 1
	i32.const 2
  )
)
"#,
        );

        let height = compute(0, &module).unwrap();
        assert_eq!(height, 2 + ACTIVATION_FRAME_COST);
    }

    #[test]
    fn call_indirect() {
        let module = parse_wat(
            r#"
(module
	(table $ptr 1 1 funcref)
	(elem $ptr (i32.const 0) func 1)
	(func $main
		(call_indirect (i32.const 0))
		(call_indirect (i32.const 0))
		(call_indirect (i32.const 0))
	)
	(func $callee
		i64.const 42
		drop
	)
)
"#,
        );

        let height = compute(0, &module).unwrap();
        assert_eq!(height, 1 + ACTIVATION_FRAME_COST);
    }

    #[test]
    fn breaks() {
        let module = parse_wat(
            r#"
(module
	(func $main
		block (result i32)
			block (result i32)
				i32.const 99
				br 1
			end
		end
		drop
	)
)
"#,
        );

        let height = compute(0, &module).unwrap();
        assert_eq!(height, 1 + ACTIVATION_FRAME_COST);
    }

    #[test]
    fn if_else_works() {
        let module = parse_wat(
            r#"
(module
	(func $main
		i32.const 7
		i32.const 1
		if (result i32)
			i32.const 42
		else
			i32.const 99
		end
		i32.const 97
		drop
		drop
		drop
	)
)
"#,
        );

        let height = compute(0, &module).unwrap();
        assert_eq!(height, 3 + ACTIVATION_FRAME_COST);
    }
}
