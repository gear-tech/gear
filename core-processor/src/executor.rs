// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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
    common::{
        DispatchResult, DispatchResultKind, ExecutableActor, ExecutionContext, ExecutionError,
    },
    configs::ExecutionSettings,
    ext::ProcessorExt,
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use gear_backend_common::{BackendReport, Environment, IntoExtInfo, TerminationReason};
use gear_core::{
    env::Ext as EnvExt,
    gas::{ChargeResult, GasAllowanceCounter, GasCounter, ValueCounter},
    memory::{pages_to_wasm_pages_set, AllocationsContext, PageNumber, WasmPageNumber},
    message::{IncomingDispatch, MessageContext},
};

/// Execute wasm with dispatch and return dispatch result.
pub fn execute_wasm<A: ProcessorExt + EnvExt + IntoExtInfo + 'static, E: Environment<A>>(
    actor: ExecutableActor,
    dispatch: IncomingDispatch,
    context: ExecutionContext,
    settings: ExecutionSettings,
) -> Result<DispatchResult, ExecutionError> {
    let ExecutableActor {
        mut program,
        balance,
    } = actor;

    let program_id = program.id();
    let kind = dispatch.kind();

    log::debug!("Executing program {}", program_id);
    log::debug!("Executing dispatch {:?}", dispatch);

    // Creating gas counter.
    let mut gas_counter = GasCounter::new(dispatch.gas_limit());
    let mut gas_allowance_counter = GasAllowanceCounter::new(context.gas_allowance);

    // Creating value counter.
    let value_counter = ValueCounter::new(balance + dispatch.value());

    let code = program.raw_code().to_vec();

    let mem_size = if let Some((max_page, _)) = program.get_pages().iter().next_back() {
        if (max_page.0 + 1) % PageNumber::num_in_one_wasm_page() != 0 {
            log::error!(
                "Program's max page is not last page in wasm page: {}",
                max_page.0
            );
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: "Program's max page is not last page in wasm page.",
                allowance_exceed: false,
            });
        }

        let max_wasm_page = max_page.to_wasm_page();

        // Charging gas for loaded pages
        let amount = settings.load_page_cost() * program.get_pages().len() as u64;

        if gas_allowance_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: "",
                allowance_exceed: true,
            });
        };

        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: "Not enough gas to load memory.",
                allowance_exceed: false,
            });
        };

        // Charging gas for mem size
        let amount = settings.mem_grow_cost()
            * (max_wasm_page.0 as u64 + 1 - program.static_pages().0 as u64);

        if gas_allowance_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: "",
                allowance_exceed: true,
            });
        }

        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: "Not enough gas to grow memory size.",
                allowance_exceed: false,
            });
        }

        // +1 because pages numeration begins from 0
        max_wasm_page + 1.into()
    } else {
        // Charging gas for initial pages
        let amount = settings.init_cost() * program.static_pages().0 as u64;

        if gas_allowance_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: "",
                allowance_exceed: true,
            });
        };

        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: "Not enough gas for initial memory.",
                allowance_exceed: false,
            });
        };

        program.static_pages()
    };

    if mem_size < program.static_pages() {
        log::error!(
            "Mem size less then static pages num: mem_size = {:?}, static_pages = {:?}",
            mem_size,
            program.static_pages()
        );
        return Err(ExecutionError {
            program_id,
            gas_amount: gas_counter.into(),
            reason: "Mem size less then static pages num",
            allowance_exceed: false,
        });
    }

    let initial_pages = program.get_pages();

    // Getting wasm pages allocations.
    let allocations: BTreeSet<WasmPageNumber> = if !initial_pages.is_empty() {
        match pages_to_wasm_pages_set(initial_pages.keys()) {
            Err(e) => {
                return Err(ExecutionError {
                    program_id,
                    gas_amount: gas_counter.into(),
                    reason: e,
                    allowance_exceed: false,
                })
            }
            Ok(res) => res,
        }
    } else {
        (0..program.static_pages().0).map(WasmPageNumber).collect()
    };

    // Creating allocations context.
    let allocations_context =
        AllocationsContext::new(allocations, program.static_pages(), settings.max_pages());

    // Creating message context.
    let message_context = MessageContext::new(
        dispatch.message().clone(),
        program_id,
        dispatch.context().clone(),
    );

    let initial_pages = program.get_pages_mut();

    // Creating externalities.
    let mut ext = A::new(
        gas_counter,
        gas_allowance_counter,
        value_counter,
        allocations_context,
        message_context,
        settings.block_info,
        settings.config,
        settings.existential_deposit,
        None,
        None,
        context.origin,
        program_id,
        Default::default(),
    );

    let lazy_pages_enabled = match ext.try_to_enable_lazy_pages(program_id, initial_pages) {
        Ok(enabled) => enabled,
        Err(e) => {
            return Err(ExecutionError {
                program_id,
                gas_amount: ext.into_gas_amount(),
                reason: e,
                allowance_exceed: false,
            })
        }
    };

    let mut env = E::new(ext, &code, initial_pages, mem_size).map_err(|err| {
        log::error!("Setup instance err = {:?}", err);
        ExecutionError {
            program_id,
            gas_amount: err.gas_amount,
            reason: err.reason,
            allowance_exceed: false,
        }
    })?;

    log::trace!(
        "init memory pages = {:?}",
        initial_pages
            .iter()
            .map(|(a, _b)| a.0)
            .collect::<Vec<u32>>()
    );

    if lazy_pages_enabled {
        A::protect_pages_and_init_info(initial_pages, program_id, env.get_wasm_memory_begin_addr())
            .map_err(|e| ExecutionError {
                program_id,
                gas_amount: env.drop_env(),
                reason: e,
                allowance_exceed: false,
            })?;
    }

    // Page which is right after stack last page
    let stack_end_page = env.get_stack_mem_end();
    log::trace!("Stack end page = {:?}", stack_end_page);

    // Running backend.
    let BackendReport {
        termination,
        wasm_memory_addr,
        info,
    } = match env.execute(kind.into_entry()) {
        Ok(report) => report,
        Err(e) => {
            return Err(ExecutionError {
                program_id,
                gas_amount: e.gas_amount,
                reason: e.reason,
                allowance_exceed: false,
            })
        }
    };

    log::trace!("term reason = {:?}", termination);

    if lazy_pages_enabled {
        // accessed lazy pages old data will be added to `initial_pages`
        // TODO: if post execution actions err is connected, with removing pages protections,
        // then we should panic here, because protected pages may cause UB later, during err handling,
        // if somebody will try to access this pages.
        A::post_execution_actions(initial_pages, wasm_memory_addr).map_err(|e| ExecutionError {
            program_id,
            gas_amount: info.gas_amount.clone(),
            reason: e,
            allowance_exceed: false,
        })?;
    }

    // Parsing outcome.
    let kind = match termination {
        TerminationReason::Exit(value_dest) => DispatchResultKind::Exit(value_dest),
        TerminationReason::Leave | TerminationReason::Success => DispatchResultKind::Success,
        TerminationReason::Trap {
            explanation,
            description,
        } => {
            log::debug!(
                "💥 Trap during execution of {}\n❓ Description: {}\n📔 Explanation: {}",
                program_id,
                description.unwrap_or_else(|| "None".into()),
                explanation.unwrap_or("None"),
            );

            DispatchResultKind::Trap(explanation)
        }
        TerminationReason::Wait => DispatchResultKind::Wait,
        TerminationReason::GasAllowanceExceed => DispatchResultKind::GasAllowanceExceed,
    };

    // changed and new pages will be updated in storage
    let mut page_update = BTreeMap::new();
    for (page, new_data) in info.pages_data {
        // exception is stack memory pages - if there are some
        // we ignore stack pages update, because they are unused after execution is ended,
        // and for next program execution old data in stack it's just garbage.
        if let Some(stack_end_page) = stack_end_page {
            if page.0 < stack_end_page.to_gear_pages().0 {
                continue;
            }
        }

        if let Some(initial_data) = initial_pages.get(&page) {
            let old_data = initial_data.as_ref().ok_or_else(|| ExecutionError {
                program_id,
                gas_amount: info.gas_amount.clone(),
                reason: "RUNTIME ERROR: changed page has no data in initial pages",
                allowance_exceed: false,
            })?;
            if !new_data.eq(old_data.as_ref()) {
                page_update.insert(page, Some(new_data));
                log::trace!(
                    "Page {} has been changed - will be updated in storage",
                    page.0
                );
            }
        } else {
            page_update.insert(page, Some(new_data));
            log::trace!("Page {} is a new page - will be upload to storage", page.0);
        };
    }

    // freed pages will be removed from storage
    let current_pages = &info.pages;
    initial_pages
        .iter()
        .filter(|(page, _)| !current_pages.contains(*page))
        .for_each(|(removed_page, _)| {
            page_update.insert(*removed_page, None);
        });

    // Getting new programs that are scheduled to be initialized (respected messages are in `generated_dispatches` collection)
    let program_candidates = info.program_candidates_data;

    // Output.
    Ok(DispatchResult {
        kind,
        dispatch,
        program_id,
        context_store: info.context_store,
        generated_dispatches: info.generated_dispatches,
        awakening: info.awakening,
        gas_amount: info.gas_amount,
        page_update,
        program_candidates,
    })
}
