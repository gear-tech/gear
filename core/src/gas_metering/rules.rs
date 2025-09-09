// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use super::Schedule;
use alloc::vec::Vec;
use gear_wasm_instrument::{
    Instruction, Module, Rules,
    gas_metering::{ConstantCostRules, MemoryGrowCost},
};

/// This type provides the functionality of [`ConstantCostRules`].
///
/// This implementation of [`Rules`] will also check the WASM module for
/// instructions that are not supported by Gear Protocol.
pub struct CustomConstantCostRules {
    constant_cost_rules: ConstantCostRules,
}

impl CustomConstantCostRules {
    /// Create a new [`CustomConstantCostRules`].
    ///
    /// Uses `instruction_cost` for every instruction and `memory_grow_cost` to
    /// dynamically meter the memory growth instruction.
    pub fn new(instruction_cost: u32, memory_grow_cost: u32, call_per_local_cost: u32) -> Self {
        Self {
            constant_cost_rules: ConstantCostRules::new(
                instruction_cost,
                memory_grow_cost,
                call_per_local_cost,
            ),
        }
    }
}

impl Default for CustomConstantCostRules {
    /// Uses instruction cost of `1` and disables memory growth instrumentation.
    fn default() -> Self {
        Self {
            constant_cost_rules: ConstantCostRules::new(1, 0, 1),
        }
    }
}

impl Rules for CustomConstantCostRules {
    fn instruction_cost(&self, instruction: &Instruction) -> Option<u32> {
        if instruction.is_user_forbidden() {
            return None;
        }

        self.constant_cost_rules.instruction_cost(instruction)
    }

    fn memory_grow_cost(&self) -> MemoryGrowCost {
        self.constant_cost_rules.memory_grow_cost()
    }

    fn call_per_local_cost(&self) -> u32 {
        self.constant_cost_rules.call_per_local_cost()
    }
}

/// This type provides real gas cost of instructions on pallet-gear.
pub struct ScheduleRules<'a> {
    schedule: &'a Schedule,
    params: Vec<u32>,
}

impl Schedule {
    /// Returns real gas rules that are used by pallet gear.
    pub fn rules(&self, module: &Module) -> impl Rules + use<'_> {
        ScheduleRules {
            schedule: self,
            params: module
                .type_section
                .as_ref()
                .iter()
                .copied()
                .flatten()
                .map(|func| func.params().len() as u32)
                .collect(),
        }
    }
}

impl Rules for ScheduleRules<'_> {
    fn instruction_cost(&self, instruction: &Instruction) -> Option<u32> {
        use Instruction::*;

        let w = &self.schedule.instruction_weights;
        let max_params = self.schedule.limits.parameters;

        Some(match instruction {
            // Returning None makes the gas instrumentation fail which we intend for
            // unsupported or unknown instructions.
            MemoryGrow { .. } => return None,
            //
            End | Unreachable | Return | Else | Block { .. } | Loop { .. } | Nop | Drop => 0,
            I32Const { .. } | I64Const { .. } => w.i64const,
            I32Load { .. }
            | I32Load8S { .. }
            | I32Load8U { .. }
            | I32Load16S { .. }
            | I32Load16U { .. } => w.i32load,
            I64Load { .. }
            | I64Load8S { .. }
            | I64Load8U { .. }
            | I64Load16S { .. }
            | I64Load16U { .. }
            | I64Load32S { .. }
            | I64Load32U { .. } => w.i64load,
            I32Store { .. } | I32Store8 { .. } | I32Store16 { .. } => w.i32store,
            I64Store { .. } | I64Store8 { .. } | I64Store16 { .. } | I64Store32 { .. } => {
                w.i64store
            }
            Select => w.select,
            If { .. } => w.r#if,
            Br { .. } => w.br,
            BrIf { .. } => w.br_if,
            Call { .. } => w.call,
            LocalGet { .. } => w.local_get,
            LocalSet { .. } => w.local_set,
            LocalTee { .. } => w.local_tee,
            GlobalGet { .. } => w.global_get,
            GlobalSet { .. } => w.global_set,
            MemorySize { .. } => w.memory_current,
            CallIndirect(idx) => {
                let params = self
                    .params
                    .get(*idx as usize)
                    .copied()
                    .unwrap_or(max_params);
                w.call_indirect
                    .saturating_add(w.call_indirect_per_param.saturating_mul(params))
            }
            BrTable(targets) => w
                .br_table
                .saturating_add(w.br_table_per_entry.saturating_mul(targets.len())),
            I32Clz => w.i32clz,
            I64Clz => w.i64clz,
            I32Ctz => w.i32ctz,
            I64Ctz => w.i64ctz,
            I32Popcnt => w.i32popcnt,
            I64Popcnt => w.i64popcnt,
            I32Eqz => w.i32eqz,
            I64Eqz => w.i64eqz,
            // TODO: rename fields
            I64ExtendI32S => w.i64extendsi32,
            I64ExtendI32U => w.i64extendui32,
            I32WrapI64 => w.i32wrapi64,
            I32Eq => w.i32eq,
            I64Eq => w.i64eq,
            I32Ne => w.i32ne,
            I64Ne => w.i64ne,
            I32LtS => w.i32lts,
            I64LtS => w.i64lts,
            I32LtU => w.i32ltu,
            I64LtU => w.i64ltu,
            I32GtS => w.i32gts,
            I64GtS => w.i64gts,
            I32GtU => w.i32gtu,
            I64GtU => w.i64gtu,
            I32LeS => w.i32les,
            I64LeS => w.i64les,
            I32LeU => w.i32leu,
            I64LeU => w.i64leu,
            I32GeS => w.i32ges,
            I64GeS => w.i64ges,
            I32GeU => w.i32geu,
            I64GeU => w.i64geu,
            I32Add => w.i32add,
            I64Add => w.i64add,
            I32Sub => w.i32sub,
            I64Sub => w.i64sub,
            I32Mul => w.i32mul,
            I64Mul => w.i64mul,
            I32DivS => w.i32divs,
            I64DivS => w.i64divs,
            I32DivU => w.i32divu,
            I64DivU => w.i64divu,
            I32RemS => w.i32rems,
            I64RemS => w.i64rems,
            I32RemU => w.i32remu,
            I64RemU => w.i64remu,
            I32And => w.i32and,
            I64And => w.i64and,
            I32Or => w.i32or,
            I64Or => w.i64or,
            I32Xor => w.i32xor,
            I64Xor => w.i64xor,
            I32Shl => w.i32shl,
            I64Shl => w.i64shl,
            I32ShrS => w.i32shrs,
            I64ShrS => w.i64shrs,
            I32ShrU => w.i32shru,
            I64ShrU => w.i64shru,
            I32Rotl => w.i32rotl,
            I64Rotl => w.i64rotl,
            I32Rotr => w.i32rotr,
            I64Rotr => w.i64rotr,
            I32Extend8S => w.i32extend8s,
            I32Extend16S => w.i32extend16s,
            I64Extend8S => w.i64extend8s,
            I64Extend16S => w.i64extend16s,
            I64Extend32S => w.i64extend32s,
        })
    }

    fn memory_grow_cost(&self) -> MemoryGrowCost {
        MemoryGrowCost::Free
    }

    fn call_per_local_cost(&self) -> u32 {
        self.schedule.instruction_weights.call_per_local
    }
}
