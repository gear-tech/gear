// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use alloc::collections::BTreeSet;
use alloc::vec;
use alloc::vec::Vec;

use gear_backend_common::Environment;
use gear_core::{
    env::Ext as EnvExt,
    gas::{ChargeResult, GasCounter},
    memory::{MemoryContext, PageNumber},
    message::{
        IncomingMessage, Message, MessageContext, MessageId, MessageIdGenerator, MessageState,
    },
    program::{Program, ProgramId},
};

use crate::configs::{AllocationsConfig, BlockInfo, EntryPoint};
use crate::ext::Ext;
use crate::ids::BlakeMessageIdGenerator;

const EXIT_CODE_PANIC: i32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Done { wait: bool },
    Trap(Option<&'static str>),
}

impl ExecutionOutcome {
    pub fn was_trap(&self) -> bool {
        if let Self::Trap(_) = *self {
            return true;
        }

        false
    }

    pub fn wait_interrupt(&self) -> bool {
        *self == Self::Done { wait: true }
    }
}

pub struct RunResult {
    pub outcome: ExecutionOutcome,
    pub program: Program,
    pub messages: Vec<Message>,
    pub gas_spent: u64,
    pub awakening: Vec<MessageId>,
}

impl core::fmt::Debug for RunResult {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RunResult")
            .field("outcome", &self.outcome)
            .field("program_executor", &self.program.id())
            .field("messages", &self.messages)
            .field("gas_spent", &self.gas_spent)
            .field("awakening", &self.awakening)
            .finish()
    }
}

impl RunResult {
    pub fn trap_with(trap_explanation: &'static str, program: Program, gas_spent: u64) -> Self {
        Self {
            outcome: ExecutionOutcome::Trap(Some(trap_explanation)),
            program,
            gas_spent,
            messages: Vec::new(),
            awakening: Vec::new(),
        }
    }
}

pub struct InitMessage {
    pub program_id: ProgramId,
    pub program_code: Vec<u8>,
    pub message: IncomingMessage,
}

pub struct ExecutionSettings {
    pub entry: EntryPoint,
    pub block_info: BlockInfo,
    pub config: AllocationsConfig,
}

impl ExecutionSettings {
    pub fn new(entry: EntryPoint, block_info: BlockInfo) -> Self {
        Self {
            entry,
            block_info,
            config: AllocationsConfig::new(),
        }
    }
}

pub struct CoreRunner;

impl CoreRunner {
    pub fn run<E>(
        env: &mut E,
        mut program: Program,
        message: IncomingMessage,
        instrumented_code: &[u8],
        settings: ExecutionSettings,
    ) -> RunResult
    where
        E: Environment<Ext>,
    {
        // Creating gas counter.
        let mut gas_counter = GasCounter::new(message.gas_limit());

        // Storing gas values.
        let left_before = gas_counter.left();
        let burned_before = gas_counter.burned();

        // Charging for initial or loaded pages.
        if settings.entry == EntryPoint::Init {
            if gas_counter.charge(settings.config.init_cost * program.static_pages() as u64)
                != ChargeResult::Enough
            {
                return RunResult::trap_with(
                    "Not enough gas for initial memory.",
                    program,
                    gas_counter.burned(),
                );
            };
        } else if gas_counter
            .charge(settings.config.load_page_cost * program.get_pages().len() as u64)
            != ChargeResult::Enough
        {
            return RunResult::trap_with(
                "Not enough gas for loading memory.",
                program,
                gas_counter.burned(),
            );
        };

        // Creating memory.
        let memory = env.create_memory(program.static_pages());

        // Charging gas for future growths.
        if let Some(max_page) = program.get_pages().iter().next_back() {
            let max_page_num = *max_page.0;
            let mem_size = memory.size();
            if max_page_num >= mem_size {
                let amount =
                    settings.config.mem_grow_cost * ((max_page_num - mem_size).raw() as u64 + 1);

                if gas_counter.charge(amount) != ChargeResult::Enough {
                    return RunResult::trap_with(
                        "Not enough gas for grow memory size.",
                        program,
                        gas_counter.burned(),
                    );
                }
            } else {
                assert!(max_page_num.raw() == mem_size.raw() - 1);
            }
        }

        // Getting allocations.
        let allocations: BTreeSet<PageNumber> = match settings.entry {
            EntryPoint::Init => (0..program.static_pages())
                .map(|page| page.into())
                .collect(),
            _ => program
                .get_pages()
                .iter()
                .map(|(page_num, _)| *page_num)
                .collect(),
        };

        // Creating memory context.
        let memory_context = MemoryContext::new(
            program.id(),
            memory.clone(),
            allocations,
            program.static_pages().into(),
            settings.config.max_pages,
        );

        // Creating message context.
        let message_context = MessageContext::new(
            message.clone(),
            BlakeMessageIdGenerator {
                program_id: program.id(),
                nonce: program.message_nonce(),
            },
        );

        // Creating externalities.
        let ext = Ext {
            gas_counter,
            memory_context,
            message_context,
            block_info: settings.block_info,
            config: settings.config,
            error_explanation: None,
            waited: false,
        };

        // Running backend.
        let (res, mut ext) = env.setup_and_run(
            ext,
            instrumented_code,
            program.get_pages(),
            &*memory,
            settings.entry.into(),
        );

        // Parsing outcome.
        let outcome = if let Err(e) = res {
            let explanation = ext.error_explanation.take();
            log::debug!(
                "Trap during execution: {}, explanation: {}",
                e,
                explanation.unwrap_or("None")
            );
            ExecutionOutcome::Trap(explanation)
        } else if ext.waited {
            ExecutionOutcome::Done { wait: true }
        } else {
            ExecutionOutcome::Done { wait: false }
        };

        // Updating program memory
        for page in ext.memory_context.allocations().clone() {
            let mut buf = vec![0u8; PageNumber::size()];
            ext.get_mem(page.offset(), &mut buf);
            let _ = program.set_page(page, &buf);
        }

        // Storing outgoing messages from message state.
        let mut messages = Vec::new();

        // We don't generate reply on trap, if we already processing trap message
        let generate_reply_on_trap = if let Some((_, exit_code)) = message.reply() {
            // reply case. generate if not a trap message
            exit_code == 0
        } else {
            // none-reply case. always generate
            true
        };

        let mut nonce = ext.message_context.nonce();

        if outcome.was_trap() && generate_reply_on_trap {
            let program_id = program.id();
            let trap_gas = ext.gas_counter.left();

            let mut id_generator = BlakeMessageIdGenerator { program_id, nonce };

            nonce += 1;

            let trap_message_id = id_generator.next();

            let trap_message = Message {
                id: trap_message_id,
                source: program_id,
                dest: message.source(),
                payload: vec![].into(),
                gas_limit: trap_gas,
                value: 0,
                reply: Some((message.id(), EXIT_CODE_PANIC)),
            };

            messages.push(trap_message);
        }

        // Updating program's message nonce
        program.set_message_nonce(nonce);

        // Storing messages state
        let MessageState {
            outgoing,
            reply,
            awakening,
        } = ext.message_context.into_state();

        for outgoing_msg in outgoing {
            messages.push(outgoing_msg.into_message(program.id()));
        }

        if let Some(reply_message) = reply {
            messages.push(reply_message.into_message(message.id(), program.id(), message.source()));
        }

        // Checking gas that was spent.
        let gas_spent = ext.gas_counter.burned();

        // Storing gas values after execution.
        let left_after = ext.gas_counter.left();
        let burned_after = gas_spent;

        // Checking abnormal cases.
        assert!(left_before >= left_after);
        assert!(burned_after >= burned_before);

        // Debug message with gas.
        log::debug!(
            "({}) Gas burned: {}; Gas used {}",
            program.id(),
            burned_after - burned_before,
            left_before - left_after
        );

        // Output.
        RunResult {
            messages,
            program,
            awakening,
            gas_spent,
            outcome,
        }
    }
}
