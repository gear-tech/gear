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
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::ToString,
};
use gear_backend_common::{BackendReport, Environment, IntoExtInfo, TerminationReason};
use gear_core::{
    env::Ext as EnvExt,
    gas::{ChargeResult, GasAllowanceCounter, GasCounter, ValueCounter},
    ids::ProgramId,
    memory::{AllocationsContext, Memory, PageBuf, PageNumber, WasmPageNumber},
    message::{ContextSettings, IncomingDispatch, MessageContext},
};

/// Make checks that everything with memory pages go well.
/// Charge gas for pages init/load/grow and checks that there is enough gas for that.
/// Returns size of wasm memory buffer which must be created in execution environment.
fn make_checks_and_charge_gas_for_pages<'a>(
    settings: &ExecutionSettings,
    gas_counter: &mut GasCounter,
    gas_allowance_counter: &mut GasAllowanceCounter,
    allocations: &BTreeSet<WasmPageNumber>,
    pages_with_data: impl Iterator<Item = &'a PageNumber>,
    static_pages: WasmPageNumber,
) -> Result<WasmPageNumber, ExecutionErrorReason> {
    // Checks that all pages with data are in allocations set.
    for page in pages_with_data {
        if !allocations.contains(&page.to_wasm_page()) {
            return Err(ExecutionErrorReason::PageIsNotAllocated(*page));
        }
    }

    let mem_size = if let Some(max_wasm_page) = allocations.iter().next_back() {
        // Charging gas for loaded pages
        let amount = settings.load_page_cost() * allocations.len() as u64;

        if gas_allowance_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionErrorReason::LoadMemoryBlockGasExceeded);
        }

        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionErrorReason::LoadMemoryGasExceeded);
        }

        // Charging gas for mem size
        let amount =
            settings.mem_grow_cost() * (max_wasm_page.0 as u64 + 1 - static_pages.0 as u64);

        if gas_allowance_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionErrorReason::GrowMemoryBlockGasExceeded);
        }

        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionErrorReason::GrowMemoryGasExceeded);
        }

        // +1 because pages numeration begins from 0
        *max_wasm_page + 1.into()
    } else {
        // Charging gas for initial pages
        let amount = settings.init_cost() * static_pages.0 as u64;

        if gas_allowance_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionErrorReason::GrowMemoryBlockGasExceeded);
        }

        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionErrorReason::InitialMemoryGasExceeded);
        }

        static_pages
    };

    if mem_size < static_pages {
        log::error!(
            "Mem size less then static pages num: mem_size = {:?}, static_pages = {:?}",
            mem_size,
            static_pages
        );
        return Err(ExecutionErrorReason::InsufficientMemorySize);
    }

    Ok(mem_size)
}

/// Writes initial pages data to memory and prepare memory for execution.
fn prepare_memory<A: ProcessorExt, M: Memory>(
    program_id: ProgramId,
    pages_data: &mut BTreeMap<PageNumber, PageBuf>,
    allocations: &BTreeSet<WasmPageNumber>,
    static_pages: WasmPageNumber,
    mem: &mut M,
) -> Result<(), ExecutionErrorReason> {
    // Set initial data for pages
    for (page, data) in pages_data.iter_mut() {
        mem.write(page.offset(), data.as_slice())
            .map_err(|err| ExecutionErrorReason::InitialDataWriteFailed(*page, err))?;
    }

    if A::is_lazy_pages_enabled() {
        // All program wasm pages, which has no data in actor, is supposed to be lazy page candidate.
        let lazy_pages = allocations
            .iter()
            .flat_map(|page| page.to_gear_pages_iter())
            .filter(|page| !pages_data.contains_key(page));
        A::lazy_pages_protect_and_init_info(mem, lazy_pages, program_id)
            .map_err(|err| ExecutionErrorReason::LazyPagesInitFailed(err.to_string()))?;
    } else {
        // If we executes without lazy pages, then we have to save all initial data for static pages,
        // in order to be able to identify pages, which has been changed during execution.
        for page in (0..static_pages.0)
            .map(WasmPageNumber)
            .flat_map(|p| p.to_gear_pages_iter())
        {
            if pages_data.contains_key(&page) {
                // This page already has initial data
                continue;
            }
            let mut data = PageBuf::new_zeroed();
            mem.read(page.offset(), data.as_mut_slice())
                .map_err(|err| ExecutionErrorReason::InitialMemoryReadFailed(page, err))?;
            pages_data.insert(page, data);
        }
    }
    Ok(())
}

/// Returns pages and their new data, which must be updated or uploaded to storage.
fn get_pages_to_be_updated<A: ProcessorExt>(
    mut old_pages_data: BTreeMap<PageNumber, PageBuf>,
    new_pages_data: BTreeMap<PageNumber, PageBuf>,
    stack_end_page: Option<WasmPageNumber>,
) -> BTreeMap<PageNumber, PageBuf> {
    let mut page_update = BTreeMap::new();
    for (page, new_data) in new_pages_data {
        // If there are stack memory pages, then
        // we ignore stack pages update, because they are unused after execution,
        // and for next program execution old data in stack it's just garbage.
        if let Some(stack_end_page) = stack_end_page {
            if page.0 < stack_end_page.to_gear_page().0 {
                continue;
            }
        }

        if A::is_lazy_pages_enabled() {
            if let Some(initial_data) = old_pages_data.remove(&page) {
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
            let initial_data = if let Some(initial_data) = old_pages_data.remove(&page) {
                initial_data
            } else {
                // If page has no data in `pages_initial_data` then data is zeros.
                // Because it's default data for wasm pages which is not static,
                // and for all static pages we save data in `pages_initial_data` in E::new.
                PageBuf::new_zeroed()
            };

            if new_data != initial_data {
                page_update.insert(page, new_data);
                log::trace!(
                    "Page {} has been changed - will be updated in storage",
                    page.0
                );
            }
        }
    }
    page_update
}

/// Execute wasm with dispatch and return dispatch result.
pub fn execute_wasm<A: ProcessorExt + EnvExt + IntoExtInfo + 'static, E: Environment<A>>(
    actor: ExecutableActor,
    dispatch: IncomingDispatch,
    context: ExecutionContext,
    settings: ExecutionSettings,
    msg_ctx_settings: ContextSettings,
) -> Result<DispatchResult, ExecutionError> {
    // Checks that lazy pages are enabled in case extension A uses them.
    if !A::check_lazy_pages_consistent_state() {
        // This is a gross violation of the terms of use ext with lazy pages,
        // so we will panic here. This cannot happens unless somebody tries to
        // use lazy-pages ext in executor without lazy-pages env enabled.
        panic!("Cannot use ext with lazy pages without lazy pages env enabled");
    }

    let ExecutableActor {
        program,
        balance,
        pages_data: mut pages_initial_data,
    } = actor;

    let program_id = program.id();
    let kind = dispatch.kind();

    log::debug!("Executing program {}", program_id);
    log::debug!("Executing dispatch {:?}", dispatch);

    // Creating gas counters.
    let mut gas_counter = GasCounter::new(dispatch.gas_limit());
    let mut gas_allowance_counter = GasAllowanceCounter::new(context.gas_allowance);

    let static_pages = program.static_pages();

    let mem_size = match make_checks_and_charge_gas_for_pages(
        &settings,
        &mut gas_counter,
        &mut gas_allowance_counter,
        program.get_allocations(),
        pages_initial_data.keys(),
        static_pages,
    ) {
        Ok(mem_size) => mem_size,
        Err(reason) => {
            return Err(ExecutionError {
                program_id,
                gas_amount: gas_counter.into(),
                reason,
            })
        }
    };

    let is_initial = program.get_allocations().is_empty();

    let exports = program.code().exports();
    if !exports.contains(&dispatch.kind()) {
        return Ok(DispatchResult {
            kind: DispatchResultKind::Success,
            dispatch,
            program_id,
            context_store: Default::default(),
            generated_dispatches: Default::default(),
            awakening: Default::default(),
            program_candidates: Default::default(),
            gas_amount: gas_counter.into(),
            page_update: Default::default(),
            allocations: if is_initial {
                Some((0..static_pages.0).map(WasmPageNumber).collect())
            } else {
                None
            },
        });
    }

    // Getting wasm pages allocations.
    let allocations = if is_initial {
        (0..static_pages.0).map(WasmPageNumber).collect()
    } else {
        program.get_allocations().clone()
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

    // Creating value counter.
    let value_counter = ValueCounter::new(balance + dispatch.value());

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
        context.origin,
        program_id,
        Default::default(),
        settings.host_fn_weights,
        settings.forbidden_funcs,
        settings.mailbox_threshold,
    );

    let mut env = E::new(
        ext,
        program.raw_code(),
        program.code().exports().clone(),
        mem_size,
    )
    .map_err(|err| {
        log::debug!("Setup instance error: {}", err);
        ExecutionError {
            program_id,
            gas_amount: err.gas_amount.clone(),
            reason: ExecutionErrorReason::Backend(err.to_string()),
        }
    })?;

    if let Err(reason) = prepare_memory::<A, E::Memory>(
        program_id,
        &mut pages_initial_data,
        &allocations,
        static_pages,
        env.get_mem_mut(),
    ) {
        return Err(ExecutionError {
            program_id,
            gas_amount: env.into_gas_amount(),
            reason,
        });
    }

    // Page which is right after stack last page
    let stack_end_page = env.get_stack_mem_end();
    log::trace!("Stack end page = {:?}", stack_end_page);

    // Execute program in backend env.
    let BackendReport { termination, info } = match env.execute(&kind, |mem| {
        // released pages initial data will be added to `pages_initial_data` after execution.
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

    log::debug!("Termination reason: {:?}", termination);

    // Parsing outcome.
    let kind = match termination {
        TerminationReason::Exit(value_dest) => DispatchResultKind::Exit(value_dest),
        TerminationReason::Leave | TerminationReason::Success => DispatchResultKind::Success,
        TerminationReason::Trap(explanation) => {
            log::debug!(
                "💥 Trap during execution of {}\n📔 Explanation: {}",
                program_id,
                explanation,
            );

            DispatchResultKind::Trap(explanation)
        }
        TerminationReason::Wait => DispatchResultKind::Wait,
        TerminationReason::GasAllowanceExceeded => DispatchResultKind::GasAllowanceExceed,
    };

    let page_update =
        get_pages_to_be_updated::<A>(pages_initial_data, info.pages_data, stack_end_page);

    // Getting new programs that are scheduled to be initialized (respected messages are in `generated_dispatches` collection)
    let program_candidates = info.program_candidates_data;

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
