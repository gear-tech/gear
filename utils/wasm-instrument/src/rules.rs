use core::num::NonZeroU32;
use gwasm_instrument::{
    gas_metering::{MemoryGrowCost, Rules},
    parity_wasm::elements::{self, Instruction},
};

pub struct CustomConstantCostRules {
    instruction_cost: u32,
    memory_grow_cost: u32,
    call_per_local_cost: u32,
}

impl CustomConstantCostRules {
    pub fn new(instruction_cost: u32, memory_grow_cost: u32, call_per_local_cost: u32) -> Self {
        Self {
            instruction_cost,
            memory_grow_cost,
            call_per_local_cost,
        }
    }
}

impl Default for CustomConstantCostRules {
    fn default() -> Self {
        Self {
            instruction_cost: 1,
            memory_grow_cost: 0,
            call_per_local_cost: 1,
        }
    }
}

impl Rules for CustomConstantCostRules {
    fn instruction_cost(&self, instruction: &Instruction) -> Option<u32> {
        use self::elements::Instruction::*;

        // list of allowed instructions from `ScheduleRules::<T>::instruction_cost(...)` method
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
            | SignExt(_) => Some(self.instruction_cost),
            _ => None,
        }
    }

    fn memory_grow_cost(&self) -> MemoryGrowCost {
        NonZeroU32::new(self.memory_grow_cost).map_or(MemoryGrowCost::Free, MemoryGrowCost::Linear)
    }

    fn call_per_local_cost(&self) -> u32 {
        self.call_per_local_cost
    }
}
