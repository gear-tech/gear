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

use alloc::vec;
use alloc::vec::Vec;

use gear_backend_common::{funcs::EXIT_TRAP_STR, Environment};
use gear_core::{
    env::Ext as EnvExt,
    gas::ChargeResult,
    memory::{MemoryContext, PageNumber},
    message::{IncomingMessage, Message, MessageContext, MessageId, MessageState},
    program::{Program, ProgramId},
};

use crate::ext::Ext;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Done,
    Trap(&'static str),
}

impl ExecutionOutcome {
    pub fn was_trap(&self) -> bool {
        if let Self::Trap(_) = *self {
            return !self.wait_interrupt();
        }

        false
    }

    pub fn wait_interrupt(&self) -> bool {
        *self == Self::Trap(EXIT_TRAP_STR)
    }
}

pub struct RunResult {
    pub outcome: ExecutionOutcome,
    pub messages: Vec<Message>,
    pub gas_spent: u64,
    pub awakening: Vec<MessageId>,
}

impl RunResult {
    pub fn fast_drop(trap_explanation: &'static str) -> Self {
        Self {
            outcome: ExecutionOutcome::Trap(trap_explanation),
            messages: Vec::new(),
            gas_spent: 0,
            awakening: Vec::new(),
        }
    }
}

pub struct InitMessage {
    pub program_id: ProgramId,
    pub program_code: Vec<u8>,
    pub message: IncomingMessage,
}

pub struct ActorProcessor;

use crate::configs::{EntryPoint, RunningContext};
use crate::ids::BlakeMessageIdGenerator;

pub struct ExecutionSettings {
    entry: EntryPoint,
    running_context: RunningContext,
    memory_context: MemoryContext,
}

impl ExecutionSettings {
    pub fn new(
        entry: EntryPoint,
        running_context: RunningContext,
        memory_context: MemoryContext,
    ) -> Self {
        Self {
            entry,
            running_context,
            memory_context,
        }
    }
}

impl ActorProcessor {
    pub fn run<E>(
        env: &mut E,
        program: &mut Program,
        message: IncomingMessage,
        mut settings: ExecutionSettings,
    ) -> RunResult
    where
        E: Environment<Ext>,
    {
        let (left_before, burned_before) = (
            settings.running_context.gas_counter().left(),
            settings.running_context.gas_counter().burned(),
        );

        let (charge_amount, trap_explanation) = if settings.entry == EntryPoint::Init {
            (
                settings.running_context.init_cost() * program.static_pages() as u64,
                "Not enough gas for initial memory.",
            )
        } else {
            (
                settings.running_context.load_page_cost() * program.get_pages().len() as u64,
                "Not enough gas for loading memory.",
            )
        };

        if settings.running_context.gas_counter().charge(charge_amount) != ChargeResult::Enough {
            return RunResult::fast_drop(trap_explanation);
        };

        let memory = env.create_memory(program.static_pages());

        let max_page = program.get_pages().iter().next_back();

        if let Some(max_page) = max_page {
            let max_page_num = *max_page.0;
            let mem_size = memory.size();
            if max_page_num >= mem_size {
                let amount = settings.running_context.mem_grow_cost()
                    * ((max_page_num - mem_size).raw() as u64 + 1);

                if settings.running_context.gas_counter().charge(amount) != ChargeResult::Enough {
                    return RunResult::fast_drop("Not enough gas for grow memory size.");
                }
            } else {
                // wtf ?
                assert!(max_page_num.raw() == mem_size.raw() - 1);
            }
        }

        let ext = Ext {
            running_context: settings.running_context,
            memory_context: settings.memory_context,
            message_context: MessageContext::new(
                message.clone(),
                BlakeMessageIdGenerator {
                    program_id: program.id(),
                    nonce: program.message_nonce(),
                },
            ),
            error_explanation: None,
            waited: false,
        };

        let (res, mut ext) = env.setup_and_run(
            ext,
            program.code(),
            program.get_pages(),
            &*memory,
            settings.entry.into(),
        );

        let outcome = if let Err(e) = res {
            let explanation = ext.error_explanation.take().unwrap_or("N/A");
            log::debug!("Trap during execution: {}, explanation: {}", e, explanation);
            ExecutionOutcome::Trap(explanation)
        } else {
            if ext.waited {
                ExecutionOutcome::Trap(EXIT_TRAP_STR)
            } else {
                ExecutionOutcome::Done
            }
        };

        for page in ext.memory_context.allocations().clone() {
            let mut buf = vec![0u8; PageNumber::size()];
            ext.get_mem(page.offset(), &mut buf);
            let _ = program.set_page(page, &buf);
        }

        let mut messages = vec![];

        program.set_message_nonce(ext.message_context.nonce());

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

        let gas_spent = ext.running_context.gas_counter().burned();

        let (left_after, burned_after) = (ext.running_context.gas_counter().left(), gas_spent);

        assert!(left_before >= left_after);
        assert!(burned_after >= burned_before);

        log::debug!(
            "({}) Gas burned: {}; Gas used {}",
            program.id(),
            burned_after - burned_before,
            left_before - left_after
        );

        RunResult {
            messages,
            awakening,
            gas_spent,
            outcome,
        }
    }
}
