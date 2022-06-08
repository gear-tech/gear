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
        ExecutionErrorReason,
    },
    configs::ExecutionSettings,
    ext::ProcessorExt,
};
use alloc::{collections::BTreeMap, string::ToString, vec::Vec};
use gear_backend_common::{BackendReport, Environment, IntoExtInfo, TerminationReason};
use gear_core::{
    env::Ext as EnvExt,
    gas::{ChargeResult, GasAllowanceCounter, GasCounter, ValueCounter},
    memory::{AllocationsContext, PageBuf, WasmPageNumber},
    message::{ContextSettings, IncomingDispatch, MessageContext},
};

/// Execute wasm with dispatch and return dispatch result.
pub fn execute_wasm<A: ProcessorExt + EnvExt + IntoExtInfo + 'static, E: Environment<A>>(
    actor: ExecutableActor,
    dispatch: IncomingDispatch,
    context: ExecutionContext,
    settings: ExecutionSettings,
    msg_ctx_settings: ContextSettings,
) -> Result<DispatchResult, ExecutionError> {
    let ExecutableActor {
        program,
        balance,
        pages_data: mut pages_initial_data,
    } = actor;

    let program_id = program.id();
    let kind = dispatch.kind();

    log::debug!("Executing program {}", program_id);
    log::debug!("Executing dispatch {:?}", dispatch);

    // Creating gas counter.
    let mut gas_counter = GasCounter::new(dispatch.gas_limit());
    let mut gas_allowance_counter = GasAllowanceCounter::new(context.gas_allowance);

    // Checks that lazy pages are enabled in case extension A uses them.
    if !A::check_lazy_pages_consistent_state() {
        return Err(ExecutionError {
            program_id,
            gas_amount: gas_counter.into(),
            reason: ExecutionErrorReason::LazyPagesInconsistentState,
        });
    }

    // Checks that all pages with data is in allocations set.
    for page in pages_initial_data.keys() {
        if !program.get_allocations().contains(&page.to_wasm_page()) {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: ExecutionErrorReason::PageIsNotAllocated(*page),
            });
        }
    }

    // Creating value counter.
    let value_counter = ValueCounter::new(balance + dispatch.value());

    let static_pages = program.static_pages();

    let mem_size = if let Some(max_wasm_page) = program.get_allocations().iter().next_back() {
        // Charging gas for loaded pages
        let amount = settings.load_page_cost() * program.get_allocations().len() as u64;

        if gas_allowance_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: ExecutionErrorReason::LoadMemoryBlockGasExceeded,
            });
        };

        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: ExecutionErrorReason::LoadMemoryGasExceeded,
            });
        };

        // Charging gas for mem size
        let amount =
            settings.mem_grow_cost() * (max_wasm_page.0 as u64 + 1 - static_pages.0 as u64);

        if gas_allowance_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: ExecutionErrorReason::GrowMemoryBlockGasExceeded,
            });
        }

        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: ExecutionErrorReason::GrowMemoryGasExceeded,
            });
        }

        // +1 because pages numeration begins from 0
        *max_wasm_page + 1.into()
    } else {
        // Charging gas for initial pages
        let amount = settings.init_cost() * static_pages.0 as u64;

        if gas_allowance_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: ExecutionErrorReason::GrowMemoryBlockGasExceeded,
            });
        };

        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason: ExecutionErrorReason::InitialMemoryGasExceeded,
            });
        };

        static_pages
    };

    if mem_size < static_pages {
        log::error!(
            "Mem size less then static pages num: mem_size = {:?}, static_pages = {:?}",
            mem_size,
            static_pages
        );
        return Err(ExecutionError {
            program_id,
            gas_amount: gas_counter.into(),
            reason: ExecutionErrorReason::InsufficientMemorySize,
        });
    }

    // Getting wasm pages allocations.
    let (allocations, is_initial) = if program.get_allocations().is_empty() {
        ((0..static_pages.0).map(WasmPageNumber).collect(), true)
    } else {
        (program.get_allocations().clone(), false)
    };

    // Creating allocations context.
    let allocations_context =
        AllocationsContext::new(allocations.clone(), static_pages, settings.max_pages());

    // Creating message context.
    let message_context = MessageContext::new_with_settings(
        dispatch.message().clone(),
        program_id,
        dispatch.context().clone(),
        msg_ctx_settings,
    );

    // Creating externalities.
    let ext = A::new(
        gas_counter,
        gas_allowance_counter,
        value_counter,
        allocations_context,
        message_context,
        settings.block_info,
        settings.allocations_config,
        settings.existential_deposit,
        None,
        context.origin,
        program_id,
        Default::default(),
        settings.host_fn_weights,
        settings.forbidden_funcs,
    );

    let mut env =
        E::new(ext, program.raw_code(), &mut pages_initial_data, mem_size).map_err(|err| {
            log::debug!("Setup instance err = {}", err);
            ExecutionError {
                program_id,
                gas_amount: err.gas_amount.clone(),
                reason: ExecutionErrorReason::Backend(err.to_string()),
            }
        })?;

    log::trace!(
        "initial pages with data = {:?}",
        pages_initial_data
            .iter()
            .map(|(p, _)| p.0)
            .collect::<Vec<_>>()
    );

    if A::is_lazy_pages_enabled() {
        // All program wasm pages, which has no data in actor, is supposed to be lazy page candidate.
        let lazy_pages = allocations
            .iter()
            .flat_map(|page| page.to_gear_pages_iter())
            .filter(|page| !pages_initial_data.contains_key(page))
            .collect();
        if let Err(e) = A::lazy_pages_protect_and_init_info(env.get_mem(), &lazy_pages, program_id)
        {
            return Err(ExecutionError {
                program_id,
                gas_amount: env.into_gas_amount(),
                reason: ExecutionErrorReason::Processor(e.to_string()),
            });
        }
        log::trace!(
            "lazy pages = {:?}",
            lazy_pages.iter().map(|p| p.0).collect::<Vec<_>>()
        );
    }

    // Page which is right after stack last page
    let stack_end_page = env.get_stack_mem_end();
    log::trace!("Stack end page = {:?}", stack_end_page);

    // Execute program in backend env.
    let BackendReport { termination, info } = match env.execute(kind.into_entry(), |mem| {
        // released pages initial data will be added to `pages_intial_data`
        if A::is_lazy_pages_enabled() {
            A::lazy_pages_post_execution_actions(mem, &mut pages_initial_data)
        } else {
            Ok(())
        }
    }) {
        Ok(report) => report,
        Err(e) => {
            return Err(ExecutionError {
                program_id,
                gas_amount: e.gas_amount.clone(),
                reason: ExecutionErrorReason::Backend(e.to_string()),
            })
        }
    };

    log::debug!("term reason = {:?}", termination);

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
                explanation
                    .as_ref()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "None".to_string()),
            );

            DispatchResultKind::Trap(explanation)
        }
        TerminationReason::Wait => DispatchResultKind::Wait,
        TerminationReason::GasAllowanceExceeded => DispatchResultKind::GasAllowanceExceed,
    };

    // changed and new pages will be updated in storage
    let mut page_update = BTreeMap::new();
    for (page, new_data) in info.pages_data {
        // If there are stack memory pages, then
        // we ignore stack pages update, because they are unused after execution,
        // and for next program execution old data in stack it's just garbage.
        if let Some(stack_end_page) = stack_end_page {
            if page.0 < stack_end_page.to_gear_page().0 {
                continue;
            }
        }

        if A::is_lazy_pages_enabled() {
            if let Some(initial_data) = pages_initial_data.remove(&page) {
                if new_data != initial_data {
                    page_update.insert(page, new_data);
                    log::trace!(
                        "Page {} has been changed - will be updated in storage",
                        page.0
                    );
                } else {
                    log::trace!("Page {} is accessed but has not been changed", page.0);
                }
            }
        } else {
            let intial_data = if let Some(initial_data) = pages_initial_data.remove(&page) {
                initial_data
            } else {
                // If page has no data in `pages_intial_data` then data is zeros.
                // Because it's default data for wasm pages which is not static,
                // and for all static pages we save data in `pages_intial_data` in E::new.
                PageBuf::new_zeroed()
            };

            if new_data != intial_data {
                page_update.insert(page, new_data);
                log::trace!(
                    "Page {} has been changed - will be updated in storage",
                    page.0
                );
            }
        }
    }

    // Getting new programs that are scheduled to be initialized (respected messages are in `generated_dispatches` collection)
    let program_candidates = info.program_candidates_data;

    log::trace!(
        "after exec allocations = {:?}",
        info.allocations.iter().map(|p| p.0).collect::<Vec<_>>()
    );

    // Output
    Ok(DispatchResult {
        kind,
        dispatch,
        program_id,
        context_store: info.context_store,
        generated_dispatches: info.generated_dispatches,
        awakening: info.awakening,
        program_candidates,
        gas_amount: info.gas_amount,
        page_update,
        allocations: if !is_initial && info.allocations.eq(&allocations) {
            None
        } else {
            Some(info.allocations)
        },
    })
}
