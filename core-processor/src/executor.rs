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

use crate::{
    common::{DispatchResult, DispatchResultKind, ExecutionError},
    configs::ExecutionSettings,
    ext::Ext,
    id::BlakeMessageIdGenerator,
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use gear_backend_common::{BackendReport, Environment, TerminationReason};
use gear_core::{
    gas::{self, ChargeResult, GasCounter},
    memory::{MemoryContext, PageNumber},
    message::{Dispatch, MessageContext, DispatchKind},
    program::Program,
};

/// Execute wasm with dispatch and return dispatch result.
pub fn execute_wasm<E: Environment<Ext>>(
    program: Program,
    dispatch: Dispatch,
    settings: ExecutionSettings,
) -> Result<DispatchResult, ExecutionError> {
    let mut env: E = Default::default();

    let Dispatch { kind, message } = dispatch.clone();

    let program_id = program.id();

    // Creating gas counter.
    let mut gas_counter = GasCounter::new(message.gas_limit());

    let instrumented_code = match gas::instrument(program.code()) {
        Ok(code) => code,
        _ => {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: "Cannot instrument code with gas-counting instructions.",
            })
        }
    };

    // Charging for initial or loaded pages.
    if program.get_pages().is_empty() {
        let amount = settings.init_cost() * program.static_pages() as u64;

        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: "Not enough gas for initial memory.",
            });
        };
    } else {
        let amount = settings.load_page_cost() * program.get_pages().len() as u64;

        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: "Not enough gas for loading memory.",
            });
        };
    }

    // Creating memory.
    let memory = match env.create_memory(program.static_pages()) {
        Ok(mem) => mem,
        Err(e) => {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: e,
            })
        }
    };

    // Charging gas for future growths.
    if let Some(max_page) = program.get_pages().iter().next_back() {
        let max_page_num = *max_page.0;
        let mem_size = memory.size();

        if max_page_num >= mem_size {
            let amount = settings.mem_grow_cost() * ((max_page_num - mem_size).raw() as u64 + 1);

            if gas_counter.charge(amount) != ChargeResult::Enough {
                return Err(ExecutionError {
                    program_id,
                    gas_amount: gas_counter.into(),
                    reason: "Not enough gas for grow memory size.",
                });
            }
        } else {
            assert!(max_page_num.raw() == mem_size.raw() - 1);
        }
    }

    let initial_pages = program.get_pages();

    // Getting allocations.
    let allocations: BTreeSet<PageNumber> = if !initial_pages.is_empty() {
        initial_pages.keys().cloned().collect()
    } else {
        (0..program.static_pages()).map(Into::into).collect()
    };

    let prev_max_page = allocations.iter().last().expect("Can't fail").raw();

    // Creating memory context.
    let memory_context = MemoryContext::new(
        program_id,
        memory.clone(),
        allocations,
        program.static_pages().into(),
        settings.max_pages(),
    );

    // Creating message context.
    let message_context = MessageContext::new(
        message.clone().into(),
        BlakeMessageIdGenerator {
            program_id,
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
    };

    // Running backend.
    let BackendReport { termination, info } = match env.setup_and_execute(
        ext,
        &instrumented_code,
        initial_pages,
        &*memory,
        kind.into_entry(),
    ) {
        Ok(report) => report,
        Err(e) => {
            return Err(ExecutionError {
                program_id,
                gas_amount: e.gas_amount,
                reason: e.reason,
            })
        }
    };

    // Parsing outcome.
    let kind = match termination {
        TerminationReason::Success | TerminationReason::Manual { wait: false } => {
            DispatchResultKind::Success
        }
        TerminationReason::Manual { wait: true } => DispatchResultKind::Wait,
        TerminationReason::Trap {
            explanation,
            description,
        } => {
            log::debug!(
                "💥 Trap during execution of {}\n❓ Description: {}📔 Explanation: {}",
                program_id,
                description.unwrap_or_else(|| "None".into()),
                explanation.unwrap_or("None"),
            );

            DispatchResultKind::Trap(explanation)
        }
    };

    // Updating program memory
    let mut page_update = BTreeMap::new();

    let actual_max_page = info
        .pages
        .iter()
        .last()
        .map(|(page, _)| page.raw())
        .expect("Can't fail");

    for (page, data) in info.pages {
        let mut need_update = true;
        if let Some(initial_data) = initial_pages.get(&page) {
            need_update = *initial_data.to_vec() != data;
        }
        if need_update {
            page_update.insert(page, Some(data));
        }
    }

    for removed_page in (actual_max_page + 1)..=prev_max_page {
        page_update.insert(removed_page.into(), None);
    }

    // Storing outgoing dispatches
    let mut outgoing = Vec::new();

    for msg in info.outgoing {
        outgoing.push(Dispatch {
            kind: DispatchKind::Handle,
            message: msg.into_message(program.id())
        });
    }

    if let Some(reply_message) = info.reply {
        outgoing.push(Dispatch {
            kind: DispatchKind::HandleReply,
            message: reply_message.into_message(message.id(), program.id(), message.source()),
        });
    }

    // Output.
    Ok(DispatchResult {
        kind,
        dispatch,
        outgoing,
        awakening: info.awakening,
        gas_amount: info.gas_amount,
        page_update,
        nonce: info.nonce,
    })
}
