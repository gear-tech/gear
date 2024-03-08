// This file is part of Gear.

// Copyright (C) 2023-2024 Gear Technologies Inc.
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

//! Mock for `ScheduleRules`.

use gwasm_instrument::{
    gas_metering::{ConstantCostRules, MemoryGrowCost, Rules},
    parity_wasm::elements::{self, Instruction},
};

/// This type provides the functionality of
/// [`gwasm_instrument::gas_metering::ConstantCostRules`].
///
/// This implementation of [`Rules`] will also check the WASM module for
/// instructions that are not supported by Gear Protocol. So, it's preferable to
/// use this type instead of `pallet_gear::Schedule::default().rules()` in unit
/// testing.
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
        use self::elements::Instruction::*;

        // list of allowed instructions from `ScheduleRules::<T>::instruction_cost(...)`
        // method
        match *instruction {
            End
            | Unreachable
            | Return
            | Else
            | Block(_)
            | Loop(_)
            | Nop
            | Drop
            | I32Const(_)
            | I64Const(_)
            | I32Load(_, _)
            | I32Load8S(_, _)
            | I32Load8U(_, _)
            | I32Load16S(_, _)
            | I32Load16U(_, _)
            | I64Load(_, _)
            | I64Load8S(_, _)
            | I64Load8U(_, _)
            | I64Load16S(_, _)
            | I64Load16U(_, _)
            | I64Load32S(_, _)
            | I64Load32U(_, _)
            | I32Store(_, _)
            | I32Store8(_, _)
            | I32Store16(_, _)
            | I64Store(_, _)
            | I64Store8(_, _)
            | I64Store16(_, _)
            | I64Store32(_, _)
            | Select
            | If(_)
            | Br(_)
            | BrIf(_)
            | Call(_)
            | GetLocal(_)
            | SetLocal(_)
            | TeeLocal(_)
            | GetGlobal(_)
            | SetGlobal(_)
            | CurrentMemory(_)
            | CallIndirect(_, _)
            | BrTable(_)
            | I32Clz
            | I64Clz
            | I32Ctz
            | I64Ctz
            | I32Popcnt
            | I64Popcnt
            | I32Eqz
            | I64Eqz
            | I64ExtendSI32
            | I64ExtendUI32
            | I32WrapI64
            | I32Eq
            | I64Eq
            | I32Ne
            | I64Ne
            | I32LtS
            | I64LtS
            | I32LtU
            | I64LtU
            | I32GtS
            | I64GtS
            | I32GtU
            | I64GtU
            | I32LeS
            | I64LeS
            | I32LeU
            | I64LeU
            | I32GeS
            | I64GeS
            | I32GeU
            | I64GeU
            | I32Add
            | I64Add
            | I32Sub
            | I64Sub
            | I32Mul
            | I64Mul
            | I32DivS
            | I64DivS
            | I32DivU
            | I64DivU
            | I32RemS
            | I64RemS
            | I32RemU
            | I64RemU
            | I32And
            | I64And
            | I32Or
            | I64Or
            | I32Xor
            | I64Xor
            | I32Shl
            | I64Shl
            | I32ShrS
            | I64ShrS
            | I32ShrU
            | I64ShrU
            | I32Rotl
            | I64Rotl
            | I32Rotr
            | I64Rotr
            | SignExt(_) => Some(self.constant_cost_rules.instruction_cost(instruction)?),
            _ => None,
        }
    }

    fn memory_grow_cost(&self) -> MemoryGrowCost {
        self.constant_cost_rules.memory_grow_cost()
    }

    fn call_per_local_cost(&self) -> u32 {
        self.constant_cost_rules.call_per_local_cost()
    }
}
