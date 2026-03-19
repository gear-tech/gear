// This file is part of Gear.
//
// Copyright (C) 2024-2026 Gear Technologies Inc.
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

use revm::{
    Database, Inspector,
    context::ContextTr,
    context_interface::cfg::gas,
    inspector::{JournalExt, inspectors::GasInspector},
    interpreter::{
        CallInputs, CallOutcome, CreateInputs, CreateOutcome, Interpreter, InterpreterTypes,
    },
    primitives::{Address, B256, Log},
};

pub struct TopicBoundedGasInspector {
    contract_address: Address,
    start_execution_topic: B256,
    end_execution_topic: B256,
    gas_at_start: Option<u64>,
    gas_diff: Option<u64>,
}

#[derive(Default)]
pub struct SimulationInspector {
    gas_inspector: GasInspector,
    topic_bounded_gas_inspector: Option<TopicBoundedGasInspector>,
}

impl SimulationInspector {
    pub fn topic_bounded_gas_inspector(
        &mut self,
        contract_address: Address,
        start_execution_topic: B256,
        end_execution_topic: B256,
    ) {
        self.topic_bounded_gas_inspector = Some(TopicBoundedGasInspector {
            contract_address,
            start_execution_topic,
            end_execution_topic,
            gas_at_start: None,
            gas_diff: None,
        });
    }

    pub fn gas_diff(&self) -> Option<u64> {
        self.topic_bounded_gas_inspector.as_ref()?.gas_diff
    }
}

impl<CTX, DB, INTR: InterpreterTypes> Inspector<CTX, INTR> for SimulationInspector
where
    DB: Database,
    CTX: ContextTr<Db = DB>,
    CTX::Journal: JournalExt,
{
    fn initialize_interp(&mut self, interp: &mut Interpreter<INTR>, _context: &mut CTX) {
        self.gas_inspector.initialize_interp(&interp.gas);
    }

    fn step(&mut self, interp: &mut Interpreter<INTR>, _context: &mut CTX) {
        // Do not remove, useful for debugging purposes

        /*use revm::{
            bytecode::OpCode,
            context::JournalTr,
            interpreter::interpreter_types::{Jumps, MemoryTr, StackTr},
        };

        let opcode = interp.bytecode.opcode();
        let name = OpCode::name_by_op(opcode);

        let gas_remaining = self.gas_inspector.gas_remaining();
        let memory_size = interp.memory.size();

        println!(
            "depth:{}, PC:{}, gas:{:#x}({}), OPCODE: {:?}({:?})  refund:{:#x}({}) Stack:{:?}, Data size:{}, Memory gas:{}",
            _context.journal().depth(),
            interp.bytecode.pc(),
            gas_remaining,
            gas_remaining,
            name,
            opcode,
            interp.gas.refunded(),
            interp.gas.refunded(),
            interp.stack.data(),
            memory_size,
            interp.gas.memory().expansion_cost,
        );*/

        self.gas_inspector.step(&interp.gas);
    }

    fn step_end(&mut self, interp: &mut Interpreter<INTR>, _context: &mut CTX) {
        self.gas_inspector.step_end(&interp.gas);
    }

    fn call_end(&mut self, _context: &mut CTX, _inputs: &CallInputs, outcome: &mut CallOutcome) {
        self.gas_inspector.call_end(outcome)
    }

    fn create_end(
        &mut self,
        _context: &mut CTX,
        _inputs: &CreateInputs,
        outcome: &mut CreateOutcome,
    ) {
        self.gas_inspector.create_end(outcome)
    }

    fn log_full(&mut self, _interp: &mut Interpreter<INTR>, _context: &mut CTX, log: Log) {
        if let Some(TopicBoundedGasInspector {
            contract_address,
            start_execution_topic,
            end_execution_topic,
            gas_at_start,
            gas_diff,
        }) = &mut self.topic_bounded_gas_inspector
            && log.address == *contract_address
            && let Some(topic0) = log.data.topics().first()
        {
            let gas_remaining = self.gas_inspector.gas_remaining();

            if topic0 == start_execution_topic {
                const LOG_GAS_COST: u64 = gas::LOG + gas::LOGTOPIC;
                let gas = gas_remaining.checked_sub(LOG_GAS_COST).expect("infallible");
                *gas_at_start = Some(gas);
            } else if topic0 == end_execution_topic
                && let Some(gas_at_start) = gas_at_start
            {
                *gas_diff = Some(gas_at_start.checked_sub(gas_remaining).expect("infallible"));
            }
        }
    }
}
