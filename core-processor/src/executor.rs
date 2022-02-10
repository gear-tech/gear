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
    common::{Dispatch, DispatchResult, DispatchResultKind, ExecutionError},
    configs::ExecutionSettings,
    ext::Ext,
    id::BlakeMessageIdGenerator,
    lazy_pages,
};
use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use common::Origin;
use core::convert::TryFrom;
use gear_backend_common::{BackendReport, Environment, TerminationReason};
use gear_core::{
    gas::{self, ChargeResult, GasCounter},
    memory::{MemoryContext, PageBuf, PageNumber, PAGE_SIZE},
    message::MessageContext,
    program::Program,
};

/// Execute wasm with dispatch and return dispatch result.
pub fn execute_wasm<E: Environment<Ext>>(
    mut program: Program,
    dispatch: Dispatch,
    settings: ExecutionSettings,
) -> Result<DispatchResult, ExecutionError> {
    let mut env: E = Default::default();

    let Dispatch { kind, message } = dispatch.clone();

    let program_id = program.id();
    log::debug!("process program {:?}", program_id);

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

    let mem_size = if let Some(max_page) = program.get_pages().iter().next_back() {
        // Charging gas for loaded pages
        let amount = settings.load_page_cost() * program.get_pages().len() as u64;
        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: "Not enough gas for loading memory.",
            });
        };

        let max_page = max_page.0.raw();

        // Charging gas for mem size
        let amount =
            settings.mem_grow_cost() * (max_page as u64 + 1 - program.static_pages() as u64);
        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: "Not enough gas for grow memory size.",
            });
        }

        // +1 because pages numeration begins from 0
        max_page + 1
    } else {
        // Charging gas for initial pages
        let amount = settings.init_cost() * program.static_pages() as u64;
        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: "Not enough gas for initial memory.",
            });
        };

        program.static_pages()
    };
    assert!(
        mem_size >= program.static_pages(),
        "mem_size = {}, static_pages = {}",
        mem_size,
        program.static_pages()
    );

    // Creating memory.
    let memory = match env.create_memory(mem_size) {
        Ok(mem) => mem,
        Err(e) => {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: e,
            })
        }
    };

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

    let initial_pages = program.get_pages_mut();

    let (lazy_pages_enabled, has_no_data_pages) =
        lazy_pages::try_to_enable_lazy_pages(initial_pages);

    if lazy_pages_enabled.is_none() && has_no_data_pages.is_some() {
        // In case we don't enable lazy-pages, then we loads data for all pages, which has no data, now.
        let prog_id_hash = program_id.into_origin();
        initial_pages
            .iter_mut()
            .filter(|(_x, y)| y.is_none())
            .for_each(|(x, y)| {
                let data = common::get_program_page_data(prog_id_hash, x.raw())
                    .expect("Page data must be in storage");
                y.replace(Box::from(PageBuf::try_from(data).expect(
                    "Must be able to convert vec to PageBuf, may be vec has wrong size",
                )));
            });
    }

    // Creating externalities.
    let ext = Ext {
        gas_counter,
        memory_context,
        message_context,
        block_info: settings.block_info,
        config: settings.config,
        lazy_pages_enabled: lazy_pages_enabled.clone(),
        error_explanation: None,
    };

    if let Err(err) = env.setup(ext, &instrumented_code, initial_pages, &*memory) {
        return Err(ExecutionError {
            program_id,
            gas_amount: err.gas_amount,
            reason: err.reason,
        });
    }

    if lazy_pages_enabled.is_some() {
        lazy_pages::protect_pages_and_init_info(
            initial_pages,
            program_id,
            memory.get_wasm_memory_begin_addr(),
        );
    }

    // Running backend.
    let BackendReport { termination, info } = match env.execute(kind.into_entry()) {
        Ok(report) => report,
        Err(e) => {
            return Err(ExecutionError {
                program_id,
                gas_amount: e.gas_amount,
                reason: e.reason,
            })
        }
    };

    if lazy_pages_enabled.is_some() {
        lazy_pages::post_execution_actions(initial_pages, memory.get_wasm_memory_begin_addr());
    }

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
        .expect("Must have some pages")
        .raw();

    for page in info.pages {
        if let Some(initial_data) = initial_pages.get(&page) {
            // if page has no buf in inital pages - means page has not been accessed - no reasons to update.
            if let Some(initial_data) = initial_data {
                let mut data = alloc::vec![0u8; PAGE_SIZE];
                memory.read(page.offset(), &mut data);
                if !data.eq(initial_data.as_ref()) {
                    page_update.insert(page, Some(data));
                    log::trace!(
                        "Page {} has been changed - will be updated in storage",
                        page.raw()
                    );
                }
            }
        } else {
            let mut data = alloc::vec![0u8; PAGE_SIZE];
            memory.read(page.offset(), &mut data);
            page_update.insert(page, Some(data));
            log::trace!(
                "Page {} is a new page - will be upload to storage",
                page.raw()
            );
        };
    }

    for removed_page in (actual_max_page + 1)..=prev_max_page {
        page_update.insert(removed_page.into(), None);
    }

    // Storing outgoing messages.
    let mut outgoing = Vec::new();

    for msg in info.outgoing {
        outgoing.push(msg.into_message(program.id()));
    }

    if let Some(reply_message) = info.reply {
        outgoing.push(reply_message.into_message(message.id(), program.id(), message.source()));
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
