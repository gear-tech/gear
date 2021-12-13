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

use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec,
    vec::Vec,
};

use crate::{
    common::{Dispatch, DispatchResult, DispatchResultKind, ExecutionError},
    configs::ExecutionSettings,
    ext::Ext,
    id::BlakeMessageIdGenerator,
};

use gear_backend_common::Environment;

use gear_core::{
    env::Ext as EnvExt,
    gas::{ChargeResult, GasCounter},
    memory::{MemoryContext, PageNumber},
    message::{MessageContext, MessageState},
    program::Program,
};

pub const ERR_EXIT_CODE: i32 = 1;

pub fn execute_wasm<E: Environment<Ext>>(
    mut program: Program,
    dispatch: Dispatch,
    settings: ExecutionSettings,
) -> Result<DispatchResult, ExecutionError> {
    let mut env = E::new();

    let Dispatch { kind, message } = dispatch.clone();
    let entry = kind.into_entry();

    let instrumented_code = match gear_core::gas::instrument(program.code()) {
        Ok(code) => code,
        Err(_) => {
            return Err(ExecutionError {
                program,
                gas_burned: 0,
                reason: "Cannot instrument code with gas-counting instructions.",
            })
        }
    };

    // Creating gas counter.
    let mut gas_counter = GasCounter::new(message.gas_limit());

    // Charging for initial or loaded pages.
    if entry == "init" {
        if gas_counter.charge(settings.init_cost() * program.static_pages() as u64)
            != ChargeResult::Enough
        {
            return Err(ExecutionError {
                program,
                gas_burned: gas_counter.burned(),
                reason: "Not enough gas for initial memory.",
            });
        };
    } else if gas_counter.charge(settings.load_page_cost() * program.get_pages().len() as u64)
        != ChargeResult::Enough
    {
        return Err(ExecutionError {
            program,
            gas_burned: gas_counter.burned(),
            reason: "Not enough gas for loading memory.",
        });
    };

    // Creating memory.
    let memory = env.create_memory(program.static_pages());

    // Charging gas for future growths.
    if let Some(max_page) = program.get_pages().iter().next_back() {
        let max_page_num = *max_page.0;
        let mem_size = memory.size();

        let amount = settings.mem_grow_cost() * ((max_page_num - mem_size).raw() as u64 + 1);

        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program,
                gas_burned: gas_counter.burned(),
                reason: "Not enough gas for grow memory size.",
            });
        }
    }

    // Getting allocations.
    let allocations: BTreeSet<PageNumber> = match entry {
        "init" => (0..program.static_pages())
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
        settings.max_pages(),
    );

    // Creating message context.
    let message_context = MessageContext::new(
        message.clone().into(),
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
        &instrumented_code,
        program.get_pages(),
        &*memory,
        entry,
    );

    // Parsing outcome.
    let kind = if let Err(e) = res {
        let explanation = ext.error_explanation.take();
        log::debug!(
            "Trap during execution: {}, explanation: {}",
            e,
            explanation.unwrap_or("None")
        );
        DispatchResultKind::Trap(explanation)
    } else if ext.waited {
        DispatchResultKind::Wait
    } else {
        DispatchResultKind::Success
    };

    // Updating program memory
    let mut page_update = BTreeMap::<PageNumber, Vec<u8>>::new();

    for page in ext.memory_context.allocations().clone() {
        let mut buf = vec![0u8; PageNumber::size()];
        ext.get_mem(page.offset(), &mut buf);
        page_update.insert(page, buf);
    }

    // Storing outgoing messages from message state.
    let mut messages = Vec::new();

    // Updating program's message nonce
    program.set_message_nonce(ext.message_context.nonce());

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
    let gas_burned = ext.gas_counter.burned();

    // Storing gas values after execution.
    let gas_left = ext.gas_counter.left();

    // Output.
    Ok(DispatchResult {
        kind,
        program,
        dispatch,
        messages,
        awakening,
        gas_left,
        gas_burned,
        page_update,
    })
}
