//! This module is used to instrument a Wasm module with gas metering code.
//!
//! The primary public interface is the [`inject`] function which transforms a given
//! module into one that charges gas for code to be executed. See function documentation for usage
//! and details.

#[cfg(test)]
mod validation;

use super::utils;
use alloc::{vec, vec::Vec};
use core::{cmp::min, mem, num::NonZeroU32};
use parity_wasm::{
	builder,
	elements::{self, Instruction, ValueType},
};

/// An interface that describes instruction costs.
pub trait Rules {
	/// Returns the cost for the passed `instruction`.
	///
	/// Returning `None` makes the gas instrumention end with an error. This is meant
	/// as a way to have a partial rule set where any instruction that is not specifed
	/// is considered as forbidden.
	fn instruction_cost(&self, instruction: &Instruction) -> Option<u32>;

	/// Returns the costs for growing the memory using the `memory.grow` instruction.
	///
	/// Please note that these costs are in addition to the costs specified by `instruction_cost`
	/// for the `memory.grow` instruction. Those are meant as dynamic costs which take the
	/// amount of pages that the memory is grown by into consideration. This is not possible
	/// using `instruction_cost` because those costs depend on the stack and must be injected as
	/// code into the function calling `memory.grow`. Therefore returning anything but
	/// [`MemoryGrowCost::Free`] introduces some overhead to the `memory.grow` instruction.
	fn memory_grow_cost(&self) -> MemoryGrowCost;

	/// A surcharge cost to calling a function that is added per local variable of the function.
	fn call_per_local_cost(&self) -> u32;
}

/// Dynamic costs for memory growth.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum MemoryGrowCost {
	/// Skip per page charge.
	///
	/// # Note
	///
	/// This makes sense when the amount of pages that a module is allowed to use is limited
	/// to a rather small number by static validation. In that case it is viable to
	/// benchmark the costs of `memory.grow` as the worst case (growing to to the maximum
	/// number of pages).
	Free,
	/// Charge the specified amount for each page that the memory is grown by.
	Linear(NonZeroU32),
}

impl MemoryGrowCost {
	/// True iff memory growths code needs to be injected.
	fn enabled(&self) -> bool {
		match self {
			Self::Free => false,
			Self::Linear(_) => true,
		}
	}
}

/// A type that implements [`Rules`] so that every instruction costs the same.
///
/// This is a simplification that is mostly useful for development and testing.
///
/// # Note
///
/// In a production environment it usually makes no sense to assign every instruction
/// the same cost. A proper implemention of [`Rules`] should be prived that is probably
/// created by benchmarking.
pub struct ConstantCostRules {
	instruction_cost: u32,
	memory_grow_cost: u32,
	call_per_local_cost: u32,
}

impl ConstantCostRules {
	/// Create a new [`ConstantCostRules`].
	///
	/// Uses `instruction_cost` for every instruction and `memory_grow_cost` to dynamically
	/// meter the memory growth instruction.
	pub fn new(instruction_cost: u32, memory_grow_cost: u32, call_per_local_cost: u32) -> Self {
		Self { instruction_cost, memory_grow_cost, call_per_local_cost }
	}
}

impl Default for ConstantCostRules {
	/// Uses instruction cost of `1` and disables memory growth instrumentation.
	fn default() -> Self {
		Self { instruction_cost: 1, memory_grow_cost: 0, call_per_local_cost: 1 }
	}
}

impl Rules for ConstantCostRules {
	fn instruction_cost(&self, _: &Instruction) -> Option<u32> {
		Some(self.instruction_cost)
	}

	fn memory_grow_cost(&self) -> MemoryGrowCost {
		NonZeroU32::new(self.memory_grow_cost).map_or(MemoryGrowCost::Free, MemoryGrowCost::Linear)
	}

	fn call_per_local_cost(&self) -> u32 {
		self.call_per_local_cost
	}
}

/// Transforms a given module into one that charges gas for code to be executed by proxy of an
/// imported gas metering function.
///
/// The output module imports a function "gas" from the specified module with type signature
/// [i32] -> []. The argument is the amount of gas required to continue execution. The external
/// function is meant to keep track of the total amount of gas used and trap or otherwise halt
/// execution of the runtime if the gas usage exceeds some allowed limit.
///
/// The body of each function is divided into metered blocks, and the calls to charge gas are
/// inserted at the beginning of every such block of code. A metered block is defined so that,
/// unless there is a trap, either all of the instructions are executed or none are. These are
/// similar to basic blocks in a control flow graph, except that in some cases multiple basic
/// blocks can be merged into a single metered block. This is the case if any path through the
/// control flow graph containing one basic block also contains another.
///
/// Charging gas is at the beginning of each metered block ensures that 1) all instructions
/// executed are already paid for, 2) instructions that will not be executed are not charged for
/// unless execution traps, and 3) the number of calls to "gas" is minimized. The corollary is that
/// modules instrumented with this metering code may charge gas for instructions not executed in
/// the event of a trap.
///
/// Additionally, each `memory.grow` instruction found in the module is instrumented to first make
/// a call to charge gas for the additional pages requested. This cannot be done as part of the
/// block level gas charges as the gas cost is not static and depends on the stack argument to
/// `memory.grow`.
///
/// The above transformations are performed for every function body defined in the module. This
/// function also rewrites all function indices references by code, table elements, etc., since
/// the addition of an imported functions changes the indices of module-defined functions. If the
/// the module has a NameSection, added by calling `parse_names`, the indices will also be updated.
///
/// This routine runs in time linear in the size of the input module.
///
/// The function fails if the module contains any operation forbidden by gas rule set, returning
/// the original module as an Err.
pub fn inject<R: Rules>(
	module: elements::Module,
	rules: &R,
	gas_module_name: &str,
) -> Result<elements::Module, elements::Module> {
	// Injecting gas counting external
	let mut mbuilder = builder::from_module(module);
	let import_sig =
		mbuilder.push_signature(builder::signature().with_param(ValueType::I32).build_sig());

	mbuilder.push_import(
		builder::import()
			.module(gas_module_name)
			.field("gas")
			.external()
			.func(import_sig)
			.build(),
	);

	// back to plain module
	let module = mbuilder.build();
	let gas_func = module.import_count(elements::ImportCountType::Function) - 1;

	let module = utils::rewrite_sections_after_insertion(module, gas_func as u32, 1)?;

	post_injection_handler(module, rules, gas_func)
}

/// Helper procedure that makes adjustments after gas metering function injected.
///
/// See documentation for [`inject`] for more details.
pub fn post_injection_handler<R: Rules>(
	mut module: elements::Module,
	rules: &R,
	gas_charge_index: usize,
) -> Result<elements::Module, elements::Module> {
	// calculate actual function index of the imported definition
	//    (subtract all imports that are NOT functions)

	let import_count = module.import_count(elements::ImportCountType::Function);
	let total_func = module.functions_space() as u32;
	let mut need_grow_counter = false;

	if let Some(code_section) = module.code_section_mut() {
		for (i, func_body) in code_section.bodies_mut().iter_mut().enumerate() {
			if i + import_count == gas_charge_index {
				continue
			}

			let result = func_body
				.locals()
				.iter()
				.try_fold(0u32, |count, val_type| count.checked_add(val_type.count()))
				.ok_or(())
				.and_then(|locals_count| {
					inject_counter(
						func_body.code_mut(),
						rules,
						locals_count,
						gas_charge_index as u32,
					)
				});

			if result.is_err() {
				return Err(module)
			}

			if rules.memory_grow_cost().enabled() &&
				inject_grow_counter(func_body.code_mut(), total_func) > 0
			{
				need_grow_counter = true;
			}
		}
	}

	match need_grow_counter {
		true => Ok(add_grow_counter(module, rules, gas_charge_index as u32)),
		false => Ok(module),
	}
}

/// A control flow block is opened with the `block`, `loop`, and `if` instructions and is closed
/// with `end`. Each block implicitly defines a new label. The control blocks form a stack during
/// program execution.
///
/// An example of block:
///
/// ```wasm
/// loop
///   i32.const 1
///   local.get 0
///   i32.sub
///   local.tee 0
///   br_if 0
/// end
/// ```
///
/// The start of the block is `i32.const 1`.
#[derive(Debug)]
struct ControlBlock {
	/// The lowest control stack index corresponding to a forward jump targeted by a br, br_if, or
	/// br_table instruction within this control block. The index must refer to a control block
	/// that is not a loop, meaning it is a forward jump. Given the way Wasm control flow is
	/// structured, the lowest index on the stack represents the furthest forward branch target.
	///
	/// This value will always be at most the index of the block itself, even if there is no
	/// explicit br instruction targeting this control block. This does not affect how the value is
	/// used in the metering algorithm.
	lowest_forward_br_target: usize,

	/// The active metering block that new instructions contribute a gas cost towards.
	active_metered_block: MeteredBlock,

	/// Whether the control block is a loop. Loops have the distinguishing feature that branches to
	/// them jump to the beginning of the block, not the end as with the other control blocks.
	is_loop: bool,
}

/// A block of code that metering instructions will be inserted at the beginning of. Metered blocks
/// are constructed with the property that, in the absence of any traps, either all instructions in
/// the block are executed or none are.
#[derive(Debug)]
struct MeteredBlock {
	/// Index of the first instruction (aka `Opcode`) in the block.
	start_pos: usize,
	/// Sum of costs of all instructions until end of the block.
	cost: BlockCostCounter,
}

/// Metering block cost counter, which handles arithmetic overflows.
#[derive(Debug, PartialEq, PartialOrd)]
#[cfg_attr(test, derive(Copy, Clone, Default))]
struct BlockCostCounter {
	/// Arithmetical overflows can occur while summarizing costs of some
	/// instruction set. To handle this, we count amount of such overflows
	/// with a separate counter and continue counting cost of metering block.
	///
	/// The overflow counter can overflow itself. However, this is not the
	/// problem for the following reason. The returning after module instrumentation
	/// set of instructions is a `Vec` which can't allocate more than `isize::MAX`
	/// amount of memory, If, for instance, we are running the counter on the host
	/// machine with 32 pointer size, reaching a huge amount of overflows can fail
	/// instrumentation even if `overflows` is not overflowed, because we will
	/// have a resulting set of instructions so big, that it will be impossible to
	/// allocate a vector for it. So regardless of overflow of `overflows` field,
	/// the field having huge value can fail instrumentation. This memory allocation
	/// problem allows us to exhale and not think about the overflow of the
	/// `overflows` field. What's more, the memory allocation problem (size of
	/// instrumenting WASM) is a caller side concern.
	overflows: usize,
	/// Block's cost accumulator.
	accumulator: u32,
}

impl BlockCostCounter {
	/// Maximum value of the `gas` call argument.
	///
	/// This constant bounds maximum value of argument
	/// in `gas` operation in order to prevent arithmetic
	/// overflow. For more information see type docs.
	const MAX_GAS_ARG: u32 = u32::MAX;

	fn zero() -> Self {
		Self::initialize(0)
	}

	fn initialize(initial_cost: u32) -> Self {
		Self { overflows: 0, accumulator: initial_cost }
	}

	fn add(&mut self, counter: BlockCostCounter) {
		// Overflow of `self.overflows` is not a big deal. See `overflows` field docs.
		self.overflows = self.overflows.saturating_add(counter.overflows);
		self.increment(counter.accumulator)
	}

	fn increment(&mut self, val: u32) {
		if let Some(res) = self.accumulator.checked_add(val) {
			self.accumulator = res;
		} else {
			// Case when self.accumulator + val > Self::MAX_GAS_ARG
			self.accumulator = val - (u32::MAX - self.accumulator);
			// Overflow of `self.overflows` is not a big deal. See `overflows` field docs.
			self.overflows = self.overflows.saturating_add(1);
		}
	}

	/// Returns the tuple of costs, where the first element is an amount of overflows
	/// emerged when summating block's cost, and the second element is the current
	/// (not overflowed remainder) block's cost.
	fn block_costs(&self) -> (usize, u32) {
		(self.overflows, self.accumulator)
	}

	/// Returns amount of costs for each of which the gas charging
	/// procedure will be called.
	fn costs_num(&self) -> usize {
		if self.accumulator != 0 {
			self.overflows + 1
		} else {
			self.overflows
		}
	}
}

/// Counter is used to manage state during the gas metering algorithm implemented by
/// `inject_counter`.
struct Counter {
	/// A stack of control blocks. This stack grows when new control blocks are opened with
	/// `block`, `loop`, and `if` and shrinks when control blocks are closed with `end`. The first
	/// block on the stack corresponds to the function body, not to any labelled block. Therefore
	/// the actual Wasm label index associated with each control block is 1 less than its position
	/// in this stack.
	stack: Vec<ControlBlock>,

	/// A list of metered blocks that have been finalized, meaning they will no longer change.
	finalized_blocks: Vec<MeteredBlock>,
}

impl Counter {
	fn new() -> Counter {
		Counter { stack: Vec::new(), finalized_blocks: Vec::new() }
	}

	/// Open a new control block. The cursor is the position of the first instruction in the block.
	fn begin_control_block(&mut self, cursor: usize, is_loop: bool) {
		let index = self.stack.len();
		self.stack.push(ControlBlock {
			lowest_forward_br_target: index,
			active_metered_block: MeteredBlock {
				start_pos: cursor,
				cost: BlockCostCounter::zero(),
			},
			is_loop,
		})
	}

	/// Close the last control block. The cursor is the position of the final (pseudo-)instruction
	/// in the block.
	fn finalize_control_block(&mut self, cursor: usize) -> Result<(), ()> {
		// This either finalizes the active metered block or merges its cost into the active
		// metered block in the previous control block on the stack.
		self.finalize_metered_block(cursor)?;

		// Pop the control block stack.
		let closing_control_block = self.stack.pop().ok_or(())?;
		let closing_control_index = self.stack.len();

		if self.stack.is_empty() {
			return Ok(())
		}

		// Update the lowest_forward_br_target for the control block now on top of the stack.
		{
			let control_block = self.stack.last_mut().ok_or(())?;
			control_block.lowest_forward_br_target = min(
				control_block.lowest_forward_br_target,
				closing_control_block.lowest_forward_br_target,
			);
		}

		// If there may have been a branch to a lower index, then also finalize the active metered
		// block for the previous control block. Otherwise, finalize it and begin a new one.
		let may_br_out = closing_control_block.lowest_forward_br_target < closing_control_index;
		if may_br_out {
			self.finalize_metered_block(cursor)?;
		}

		Ok(())
	}

	/// Finalize the current active metered block.
	///
	/// Finalized blocks have final cost which will not change later.
	fn finalize_metered_block(&mut self, cursor: usize) -> Result<(), ()> {
		let closing_metered_block = {
			let control_block = self.stack.last_mut().ok_or(())?;
			mem::replace(
				&mut control_block.active_metered_block,
				MeteredBlock { start_pos: cursor + 1, cost: BlockCostCounter::zero() },
			)
		};

		// If the block was opened with a `block`, then its start position will be set to that of
		// the active metered block in the control block one higher on the stack. This is because
		// any instructions between a `block` and the first branch are part of the same basic block
		// as the preceding instruction. In this case, instead of finalizing the block, merge its
		// cost into the other active metered block to avoid injecting unnecessary instructions.
		let last_index = self.stack.len() - 1;
		if last_index > 0 {
			let prev_control_block = self
				.stack
				.get_mut(last_index - 1)
				.expect("last_index is greater than 0; last_index is stack size - 1; qed");
			let prev_metered_block = &mut prev_control_block.active_metered_block;
			if closing_metered_block.start_pos == prev_metered_block.start_pos {
				prev_metered_block.cost.add(closing_metered_block.cost);
				return Ok(())
			}
		}

		if closing_metered_block.cost > BlockCostCounter::zero() {
			self.finalized_blocks.push(closing_metered_block);
		}
		Ok(())
	}

	/// Handle a branch instruction in the program. The cursor is the index of the branch
	/// instruction in the program. The indices are the stack positions of the target control
	/// blocks. Recall that the index is 0 for a `return` and relatively indexed from the top of
	/// the stack by the label of `br`, `br_if`, and `br_table` instructions.
	fn branch(&mut self, cursor: usize, indices: &[usize]) -> Result<(), ()> {
		self.finalize_metered_block(cursor)?;

		// Update the lowest_forward_br_target of the current control block.
		for &index in indices {
			let target_is_loop = {
				let target_block = self.stack.get(index).ok_or(())?;
				target_block.is_loop
			};
			if target_is_loop {
				continue
			}

			let control_block = self.stack.last_mut().ok_or(())?;
			control_block.lowest_forward_br_target =
				min(control_block.lowest_forward_br_target, index);
		}

		Ok(())
	}

	/// Returns the stack index of the active control block. Returns None if stack is empty.
	fn active_control_block_index(&self) -> Option<usize> {
		self.stack.len().checked_sub(1)
	}

	/// Get a reference to the currently active metered block.
	fn active_metered_block(&mut self) -> Result<&mut MeteredBlock, ()> {
		let top_block = self.stack.last_mut().ok_or(())?;
		Ok(&mut top_block.active_metered_block)
	}

	/// Increment the cost of the current block by the specified value.
	fn increment(&mut self, val: u32) -> Result<(), ()> {
		let top_block = self.active_metered_block()?;
		top_block.cost.increment(val);
		Ok(())
	}
}

fn inject_grow_counter(instructions: &mut elements::Instructions, grow_counter_func: u32) -> usize {
	use parity_wasm::elements::Instruction::*;
	let mut counter = 0;
	for instruction in instructions.elements_mut() {
		if let GrowMemory(_) = *instruction {
			*instruction = Call(grow_counter_func);
			counter += 1;
		}
	}
	counter
}

fn add_grow_counter<R: Rules>(
	module: elements::Module,
	rules: &R,
	gas_func: u32,
) -> elements::Module {
	use parity_wasm::elements::Instruction::*;

	let cost = match rules.memory_grow_cost() {
		MemoryGrowCost::Free => return module,
		MemoryGrowCost::Linear(val) => val.get(),
	};

	let mut b = builder::from_module(module);
	b.push_function(
		builder::function()
			.signature()
			.with_param(ValueType::I32)
			.with_result(ValueType::I32)
			.build()
			.body()
			.with_instructions(elements::Instructions::new(vec![
				GetLocal(0),
				GetLocal(0),
				I32Const(cost as i32),
				I32Mul,
				// todo: there should be strong guarantee that it does not return anything on
				// stack?
				Call(gas_func),
				GrowMemory(0),
				End,
			]))
			.build()
			.build(),
	);

	b.build()
}

fn determine_metered_blocks<R: Rules>(
	instructions: &elements::Instructions,
	rules: &R,
	locals_count: u32,
) -> Result<Vec<MeteredBlock>, ()> {
	use parity_wasm::elements::Instruction::*;

	let mut counter = Counter::new();

	// Begin an implicit function (i.e. `func...end`) block.
	counter.begin_control_block(0, false);

	// Add locals initialization cost to the function block.
	let locals_init_cost = rules.call_per_local_cost().checked_mul(locals_count).ok_or(())?;
	counter.increment(locals_init_cost)?;

	for cursor in 0..instructions.elements().len() {
		let instruction = &instructions.elements()[cursor];
		let instruction_cost = rules.instruction_cost(instruction).ok_or(())?;
		match instruction {
			Block(_) => {
				counter.increment(instruction_cost)?;

				// Begin new block. The cost of the following opcodes until `end` or `else` will
				// be included into this block. The start position is set to that of the previous
				// active metered block to signal that they should be merged in order to reduce
				// unnecessary metering instructions.
				let top_block_start_pos = counter.active_metered_block()?.start_pos;
				counter.begin_control_block(top_block_start_pos, false);
			},
			If(_) => {
				counter.increment(instruction_cost)?;
				counter.begin_control_block(cursor + 1, false);
			},
			Loop(_) => {
				counter.increment(instruction_cost)?;
				counter.begin_control_block(cursor + 1, true);
			},
			End => {
				counter.finalize_control_block(cursor)?;
			},
			Else => {
				counter.finalize_metered_block(cursor)?;
			},
			Br(label) | BrIf(label) => {
				counter.increment(instruction_cost)?;

				// Label is a relative index into the control stack.
				let active_index = counter.active_control_block_index().ok_or(())?;
				let target_index = active_index.checked_sub(*label as usize).ok_or(())?;
				counter.branch(cursor, &[target_index])?;
			},
			BrTable(br_table_data) => {
				counter.increment(instruction_cost)?;

				let active_index = counter.active_control_block_index().ok_or(())?;
				let target_indices = [br_table_data.default]
					.iter()
					.chain(br_table_data.table.iter())
					.map(|label| active_index.checked_sub(*label as usize))
					.collect::<Option<Vec<_>>>()
					.ok_or(())?;
				counter.branch(cursor, &target_indices)?;
			},
			Return => {
				counter.increment(instruction_cost)?;
				counter.branch(cursor, &[0])?;
			},
			_ => {
				// An ordinal non control flow instruction increments the cost of the current block.
				counter.increment(instruction_cost)?;
			},
		}
	}

	counter.finalized_blocks.sort_unstable_by_key(|block| block.start_pos);
	Ok(counter.finalized_blocks)
}

fn inject_counter<R: Rules>(
	instructions: &mut elements::Instructions,
	rules: &R,
	locals_count: u32,
	gas_func: u32,
) -> Result<(), ()> {
	let blocks = determine_metered_blocks(instructions, rules, locals_count)?;
	insert_metering_calls(instructions, blocks, gas_func)
}

// Then insert metering calls into a sequence of instructions given the block locations and costs.
fn insert_metering_calls(
	instructions: &mut elements::Instructions,
	blocks: Vec<MeteredBlock>,
	gas_func: u32,
) -> Result<(), ()> {
	let block_cost_instrs = calculate_blocks_costs_num(&blocks);
	// To do this in linear time, construct a new vector of instructions, copying over old
	// instructions one by one and injecting new ones as required.
	let new_instrs_len = instructions.elements().len() + 2 * block_cost_instrs;
	let original_instrs =
		mem::replace(instructions.elements_mut(), Vec::with_capacity(new_instrs_len));
	let new_instrs = instructions.elements_mut();

	let mut block_iter = blocks.into_iter().peekable();
	for (original_pos, instr) in original_instrs.into_iter().enumerate() {
		// If there the next block starts at this position, inject metering instructions.
		let used_block = if let Some(block) = block_iter.peek() {
			if block.start_pos == original_pos {
				insert_gas_call(new_instrs, block, gas_func);
				true
			} else {
				false
			}
		} else {
			false
		};

		if used_block {
			block_iter.next();
		}

		// Copy over the original instruction.
		new_instrs.push(instr);
	}

	if block_iter.next().is_some() {
		return Err(())
	}

	Ok(())
}

// Calculates total amount of costs (potential gas charging calls) in blocks
fn calculate_blocks_costs_num(blocks: &[MeteredBlock]) -> usize {
	blocks.iter().map(|block| block.cost.costs_num()).sum()
}

fn insert_gas_call(new_instrs: &mut Vec<Instruction>, current_block: &MeteredBlock, gas_func: u32) {
	use parity_wasm::elements::Instruction::*;

	let (mut overflows_num, current_cost) = current_block.cost.block_costs();
	// First insert gas charging call with maximum argument due to overflows.
	while overflows_num != 0 {
		new_instrs.push(I32Const(BlockCostCounter::MAX_GAS_ARG as i32));
		new_instrs.push(Call(gas_func));
		overflows_num -= 1;
	}
	// Second insert remaining block's cost, if necessary.
	if current_cost != 0 {
		new_instrs.push(I32Const(current_cost as i32));
		new_instrs.push(Call(gas_func));
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use parity_wasm::{builder, elements, elements::Instruction::*, serialize};

	fn get_function_body(
		module: &elements::Module,
		index: usize,
	) -> Option<&[elements::Instruction]> {
		module
			.code_section()
			.and_then(|code_section| code_section.bodies().get(index))
			.map(|func_body| func_body.code().elements())
	}

	fn prebuilt_simple_module() -> elements::Module {
		builder::module()
			.global()
			.value_type()
			.i32()
			.build()
			.function()
			.signature()
			.param()
			.i32()
			.build()
			.body()
			.build()
			.build()
			.function()
			.signature()
			.param()
			.i32()
			.build()
			.body()
			.with_instructions(elements::Instructions::new(vec![
				Call(0),
				If(elements::BlockType::NoResult),
				Call(0),
				Call(0),
				Call(0),
				Else,
				Call(0),
				Call(0),
				End,
				Call(0),
				End,
			]))
			.build()
			.build()
			.build()
	}

	#[test]
	fn simple_grow() {
		let module = parse_wat(
			r#"(module
			(func (result i32)
			  global.get 0
			  memory.grow)
			(global i32 (i32.const 42))
			(memory 0 1)
			)"#,
		);

		let injected_module = inject(module, &ConstantCostRules::new(1, 10_000, 1), "env").unwrap();

		assert_eq!(
			get_function_body(&injected_module, 0).unwrap(),
			&vec![I32Const(2), Call(0), GetGlobal(0), Call(2), End][..]
		);
		assert_eq!(
			get_function_body(&injected_module, 1).unwrap(),
			&vec![GetLocal(0), GetLocal(0), I32Const(10000), I32Mul, Call(0), GrowMemory(0), End,]
				[..]
		);

		let binary = serialize(injected_module).expect("serialization failed");
		wasmparser::validate(&binary).unwrap();
	}

	#[test]
	fn grow_no_gas_no_track() {
		let module = parse_wat(
			r"(module
			(func (result i32)
			  global.get 0
			  memory.grow)
			(global i32 (i32.const 42))
			(memory 0 1)
			)",
		);

		let injected_module = inject(module, &ConstantCostRules::default(), "env").unwrap();

		assert_eq!(
			get_function_body(&injected_module, 0).unwrap(),
			&vec![I32Const(2), Call(0), GetGlobal(0), GrowMemory(0), End][..]
		);

		assert_eq!(injected_module.functions_space(), 2);

		let binary = serialize(injected_module).expect("serialization failed");
		wasmparser::validate(&binary).unwrap();
	}

	#[test]
	fn call_index() {
		let injected_module =
			inject(prebuilt_simple_module(), &ConstantCostRules::default(), "env").unwrap();

		assert_eq!(
			get_function_body(&injected_module, 1).unwrap(),
			&vec![
				I32Const(3),
				Call(0),
				Call(1),
				If(elements::BlockType::NoResult),
				I32Const(3),
				Call(0),
				Call(1),
				Call(1),
				Call(1),
				Else,
				I32Const(2),
				Call(0),
				Call(1),
				Call(1),
				End,
				Call(1),
				End
			][..]
		);
	}

	#[test]
	fn cost_overflow() {
		let instruction_cost = u32::MAX / 2;
		let injected_module = inject(
			prebuilt_simple_module(),
			&ConstantCostRules::new(instruction_cost, 0, instruction_cost),
			"env",
		)
		.unwrap();

		assert_eq!(
			get_function_body(&injected_module, 1).unwrap(),
			&vec![
				// (instruction_cost * 3) as i32 => ((2147483647 * 2) + 2147483647) as i32 =>
				// ((2147483647 + 2147483647 + 1) + 2147483646) as i32 =>
				// (u32::MAX as i32) + 2147483646 as i32
				I32Const(-1),
				Call(0),
				I32Const((instruction_cost - 1) as i32),
				Call(0),
				Call(1),
				If(elements::BlockType::NoResult),
				// Same as upper
				I32Const(-1),
				Call(0),
				I32Const((instruction_cost - 1) as i32),
				Call(0),
				Call(1),
				Call(1),
				Call(1),
				Else,
				// (instruction_cost * 2) as i32
				I32Const(-2),
				Call(0),
				Call(1),
				Call(1),
				End,
				Call(1),
				End
			][..]
		);
	}

	fn parse_wat(source: &str) -> elements::Module {
		let module_bytes = wat::parse_str(source).unwrap();
		elements::deserialize_buffer(module_bytes.as_ref()).unwrap()
	}

	macro_rules! test_gas_counter_injection {
		(name = $name:ident; input = $input:expr; expected = $expected:expr) => {
			#[test]
			fn $name() {
				let input_module = parse_wat($input);
				let expected_module = parse_wat($expected);

				let injected_module = inject(input_module, &ConstantCostRules::default(), "env")
					.expect("inject_gas_counter call failed");

				let actual_func_body = get_function_body(&injected_module, 0)
					.expect("injected module must have a function body");
				let expected_func_body = get_function_body(&expected_module, 0)
					.expect("post-module must have a function body");

				assert_eq!(actual_func_body, expected_func_body);
			}
		};
	}

	test_gas_counter_injection! {
		name = simple;
		input = r#"
		(module
			(func (result i32)
				(global.get 0)))
		"#;
		expected = r#"
		(module
			(func (result i32)
				(call 0 (i32.const 1))
				(global.get 0)))
		"#
	}

	test_gas_counter_injection! {
		name = nested;
		input = r#"
		(module
			(func (result i32)
				(global.get 0)
				(block
					(global.get 0)
					(global.get 0)
					(global.get 0))
				(global.get 0)))
		"#;
		expected = r#"
		(module
			(func (result i32)
				(call 0 (i32.const 6))
				(global.get 0)
				(block
					(global.get 0)
					(global.get 0)
					(global.get 0))
				(global.get 0)))
		"#
	}

	test_gas_counter_injection! {
		name = ifelse;
		input = r#"
		(module
			(func (result i32)
				(global.get 0)
				(if
					(then
						(global.get 0)
						(global.get 0)
						(global.get 0))
					(else
						(global.get 0)
						(global.get 0)))
				(global.get 0)))
		"#;
		expected = r#"
		(module
			(func (result i32)
				(call 0 (i32.const 3))
				(global.get 0)
				(if
					(then
						(call 0 (i32.const 3))
						(global.get 0)
						(global.get 0)
						(global.get 0))
					(else
						(call 0 (i32.const 2))
						(global.get 0)
						(global.get 0)))
				(global.get 0)))
		"#
	}

	test_gas_counter_injection! {
		name = branch_innermost;
		input = r#"
		(module
			(func (result i32)
				(global.get 0)
				(block
					(global.get 0)
					(drop)
					(br 0)
					(global.get 0)
					(drop))
				(global.get 0)))
		"#;
		expected = r#"
		(module
			(func (result i32)
				(call 0 (i32.const 6))
				(global.get 0)
				(block
					(global.get 0)
					(drop)
					(br 0)
					(call 0 (i32.const 2))
					(global.get 0)
					(drop))
				(global.get 0)))
		"#
	}

	test_gas_counter_injection! {
		name = branch_outer_block;
		input = r#"
		(module
			(func (result i32)
				(global.get 0)
				(block
					(global.get 0)
					(if
						(then
							(global.get 0)
							(global.get 0)
							(drop)
							(br_if 1)))
					(global.get 0)
					(drop))
				(global.get 0)))
		"#;
		expected = r#"
		(module
			(func (result i32)
				(call 0 (i32.const 5))
				(global.get 0)
				(block
					(global.get 0)
					(if
						(then
							(call 0 (i32.const 4))
							(global.get 0)
							(global.get 0)
							(drop)
							(br_if 1)))
					(call 0 (i32.const 2))
					(global.get 0)
					(drop))
				(global.get 0)))
		"#
	}

	test_gas_counter_injection! {
		name = branch_outer_loop;
		input = r#"
		(module
			(func (result i32)
				(global.get 0)
				(loop
					(global.get 0)
					(if
						(then
							(global.get 0)
							(br_if 0))
						(else
							(global.get 0)
							(global.get 0)
							(drop)
							(br_if 1)))
					(global.get 0)
					(drop))
				(global.get 0)))
		"#;
		expected = r#"
		(module
			(func (result i32)
				(call 0 (i32.const 3))
				(global.get 0)
				(loop
					(call 0 (i32.const 4))
					(global.get 0)
					(if
						(then
							(call 0 (i32.const 2))
							(global.get 0)
							(br_if 0))
						(else
							(call 0 (i32.const 4))
							(global.get 0)
							(global.get 0)
							(drop)
							(br_if 1)))
					(global.get 0)
					(drop))
				(global.get 0)))
		"#
	}

	test_gas_counter_injection! {
		name = return_from_func;
		input = r#"
		(module
			(func (result i32)
				(global.get 0)
				(if
					(then
						(return)))
				(global.get 0)))
		"#;
		expected = r#"
		(module
			(func (result i32)
				(call 0 (i32.const 2))
				(global.get 0)
				(if
					(then
						(call 0 (i32.const 1))
						(return)))
				(call 0 (i32.const 1))
				(global.get 0)))
		"#
	}

	test_gas_counter_injection! {
		name = branch_from_if_not_else;
		input = r#"
		(module
			(func (result i32)
				(global.get 0)
				(block
					(global.get 0)
					(if
						(then (br 1))
						(else (br 0)))
					(global.get 0)
					(drop))
				(global.get 0)))
		"#;
		expected = r#"
		(module
			(func (result i32)
				(call 0 (i32.const 5))
				(global.get 0)
				(block
					(global.get 0)
					(if
						(then
							(call 0 (i32.const 1))
							(br 1))
						(else
							(call 0 (i32.const 1))
							(br 0)))
					(call 0 (i32.const 2))
					(global.get 0)
					(drop))
				(global.get 0)))
		"#
	}

	test_gas_counter_injection! {
		name = empty_loop;
		input = r#"
		(module
			(func
				(loop
					(br 0)
				)
				unreachable
			)
		)
		"#;
		expected = r#"
		(module
			(func
				(call 0 (i32.const 2))
				(loop
					(call 0 (i32.const 1))
					(br 0)
				)
				unreachable
			)
		)
		"#
	}
}
