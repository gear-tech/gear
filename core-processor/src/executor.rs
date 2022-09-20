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
        DispatchResult, DispatchResultKind, ExecutionError, ExecutionErrorReason,
        WasmExecutionContext,
    },
    configs::{AllocationsConfig, ExecutionSettings},
    ext::{ProcessorContext, ProcessorExt},
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

/// Make checks that everything with memory goes well.
fn check_memory<'a>(
    allocations: &BTreeSet<WasmPageNumber>,
    pages_with_data: impl Iterator<Item = &'a PageNumber>,
    static_pages: WasmPageNumber,
    memory_size: WasmPageNumber,
) -> Result<(), ExecutionErrorReason> {
    // Checks that all pages with data are in allocations set.
    for page in pages_with_data {
        let wasm_page = page.to_wasm_page();
        if wasm_page >= static_pages && !allocations.contains(&wasm_page) {
            return Err(ExecutionErrorReason::PageIsNotAllocated(*page));
        }
    }

    if memory_size < static_pages {
        log::error!(
            "Mem size less then static pages num: mem_size = {:?}, static_pages = {:?}",
            memory_size,
            static_pages
        );
        return Err(ExecutionErrorReason::InsufficientMemorySize);
    }

    Ok(())
}

/// Charge gas for pages init/load/grow and checks that there is enough gas for that.
/// Returns size of wasm memory buffer which must be created in execution environment.
pub(crate) fn charge_gas_for_pages(
    settings: &AllocationsConfig,
    gas_counter: &mut GasCounter,
    gas_allowance_counter: &mut GasAllowanceCounter,
    allocations: &BTreeSet<WasmPageNumber>,
    static_pages: WasmPageNumber,
    initial_execution: bool,
    subsequent_execution: bool,
) -> Result<WasmPageNumber, ExecutionErrorReason> {
    if !initial_execution {
        let max_wasm_page = if let Some(page) = allocations.iter().next_back() {
            *page
        } else if static_pages != WasmPageNumber(0) {
            static_pages - 1.into()
        } else {
            return Ok(0.into());
        };

        if !subsequent_execution {
            // Charging gas for loaded pages
            let amount =
                settings.load_page_cost * (allocations.len() as u64 + static_pages.0 as u64);
            if gas_allowance_counter.charge(amount) != ChargeResult::Enough {
                return Err(ExecutionErrorReason::LoadMemoryBlockGasExceeded);
            }

            if gas_counter.charge(amount) != ChargeResult::Enough {
                return Err(ExecutionErrorReason::LoadMemoryGasExceeded);
            }
        }

        // Charging gas for mem size
        let amount = settings.mem_grow_cost * (max_wasm_page.0 as u64 + 1 - static_pages.0 as u64);

        if gas_allowance_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionErrorReason::GrowMemoryBlockGasExceeded);
        }

        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionErrorReason::GrowMemoryGasExceeded);
        }

        // +1 because pages numeration begins from 0
        Ok(max_wasm_page + 1.into())
    } else {
        // Charging gas for initial pages
        let amount = settings.init_cost * static_pages.0 as u64;
        log::trace!("Charge {} for initial pages", amount);

        if gas_allowance_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionErrorReason::InitialMemoryBlockGasExceeded);
        }

        if gas_counter.charge(amount) != ChargeResult::Enough {
            return Err(ExecutionErrorReason::InitialMemoryGasExceeded);
        }

        Ok(static_pages)
    }
}

/// Writes initial pages data to memory and prepare memory for execution.
fn prepare_memory<A: ProcessorExt, M: Memory>(
    program_id: ProgramId,
    pages_data: &mut BTreeMap<PageNumber, PageBuf>,
    static_pages: WasmPageNumber,
    stack_end: Option<WasmPageNumber>,
    mem: &mut M,
) -> Result<(), ExecutionErrorReason> {
    if let Some(stack_end) = stack_end {
        if stack_end > static_pages {
            return Err(ExecutionErrorReason::StackEndPageBiggerWasmMemSize(
                stack_end,
                static_pages,
            ));
        }
    }

    // Set initial data for pages
    for (page, data) in pages_data.iter_mut() {
        mem.write(page.offset(), data.as_slice())
            .map_err(|err| ExecutionErrorReason::InitialDataWriteFailed(*page, err))?;
    }

    if A::LAZY_PAGES_ENABLED {
        if !pages_data.is_empty() {
            return Err(ExecutionErrorReason::InitialPagesContainsDataInLazyPagesMode);
        }
        A::lazy_pages_init_for_program(mem, program_id, stack_end)
            .map_err(|err| ExecutionErrorReason::LazyPagesInitFailed(err.to_string()))?;
    } else {
        // If we executes without lazy pages, then we have to save all initial data for static pages,
        // in order to be able to identify pages, which has been changed during execution.
        // Skip stack page if they are specified.
        let begin = stack_end.unwrap_or(WasmPageNumber(0));

        if pages_data.keys().any(|&p| p < begin.to_gear_page()) {
            return Err(ExecutionErrorReason::StackPagesHaveInitialData);
        }

        for page in (begin.0..static_pages.0)
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
    static_pages: WasmPageNumber,
) -> BTreeMap<PageNumber, PageBuf> {
    let mut page_update = BTreeMap::new();
    for (page, new_data) in new_pages_data {
        if A::LAZY_PAGES_ENABLED {
            // In lazy pages mode we update some page data in storage,
            // when it has been write accessed, so no need to compare old and new page data.
            log::trace!("{:?} has been write accessed, update it in storage", page);
            page_update.insert(page, new_data);
        } else {
            let initial_data = if let Some(initial_data) = old_pages_data.remove(&page) {
                initial_data
            } else {
                // If it's static page without initial data,
                // then it's stack page and we skip this page update.
                if page < static_pages.to_gear_page() {
                    continue;
                }

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

#[allow(clippy::result_large_err)]
/// Execute wasm with dispatch and return dispatch result.
pub fn execute_wasm<A: ProcessorExt + EnvExt + IntoExtInfo + 'static, E: Environment<A>>(
    balance: u128,
    dispatch: IncomingDispatch,
    context: WasmExecutionContext,
    settings: ExecutionSettings,
    msg_ctx_settings: ContextSettings,
) -> Result<DispatchResult, ExecutionError> {
    let WasmExecutionContext {
        gas_counter,
        gas_allowance_counter,
        gas_reserver: gas_reservation_map,
        origin,
        program,
        mut pages_initial_data,
        memory_size,
    } = context;

    let program_id = program.id();
    let kind = dispatch.kind();

    log::debug!("Executing program {}", program_id);
    log::debug!("Executing dispatch {:?}", dispatch);

    let static_pages = program.static_pages();
    let allocations = program.get_allocations();

    if let Err(reason) = check_memory(
        allocations,
        pages_initial_data.keys(),
        static_pages,
        memory_size,
    ) {
        return Err(ExecutionError {
            program_id,
            gas_amount: gas_counter.into(),
            reason,
        });
    }

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

    let context = ProcessorContext {
        gas_counter,
        gas_allowance_counter,
        gas_reserver: gas_reservation_map,
        value_counter,
        allocations_context,
        message_context,
        block_info: settings.block_info,
        config: settings.allocations_config,
        existential_deposit: settings.existential_deposit,
        origin,
        program_id,
        program_candidates_data: Default::default(),
        host_fn_weights: settings.host_fn_weights,
        forbidden_funcs: settings.forbidden_funcs,
        mailbox_threshold: settings.mailbox_threshold,
        waitlist_cost: settings.waitlist_cost,
        reserve_for: settings.reserve_for,
    };

    // Creating externalities.
    let mut ext = A::new(context);

    // Execute program in backend env.
    let (termination, memory) = match E::execute(
        &mut ext,
        program.raw_code(),
        program.code().exports().clone(),
        memory_size,
        &kind,
        |memory, stack_end| {
            prepare_memory::<A, E::Memory>(
                program_id,
                &mut pages_initial_data,
                static_pages,
                stack_end,
                memory,
            )
        },
    ) {
        Ok(BackendReport {
            termination_reason: termination,
            memory_wrap: memory,
        }) => {
            // released pages initial data will be added to `pages_initial_data` after execution.
            if A::LAZY_PAGES_ENABLED {
                if let Err(e) = A::lazy_pages_post_execution_actions(&memory) {
                    return Err(ExecutionError {
                        program_id,
                        gas_amount: ext.into_gas_amount(),
                        reason: ExecutionErrorReason::Backend(e.to_string()),
                    });
                }
            }
            (termination, memory)
        }

        Err(e) => {
            return Err(ExecutionError {
                program_id,
                gas_amount: ext.into_gas_amount(),
                reason: ExecutionErrorReason::Backend(e.to_string()),
            })
        }
    };

    log::debug!("Termination reason: {:?}", termination);

    let info = ext
        .into_ext_info(&memory)
        .map_err(|(err, gas_amount)| ExecutionError {
            program_id,
            gas_amount,
            reason: ExecutionErrorReason::Backend(err.to_string()),
        })?;

    if A::LAZY_PAGES_ENABLED && !pages_initial_data.is_empty() {
        return Err(ExecutionError {
            program_id,
            gas_amount: info.gas_amount,
            reason: ExecutionErrorReason::InitialPagesContainsDataInLazyPagesMode,
        });
    }

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
        TerminationReason::Wait(duration) => DispatchResultKind::Wait(duration),
        TerminationReason::GasAllowanceExceeded => DispatchResultKind::GasAllowanceExceed,
    };

    let page_update =
        get_pages_to_be_updated::<A>(pages_initial_data, info.pages_data, static_pages);

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
        gas_reserver: info.gas_reserver,
        page_update,
        allocations: info.allocations,
    })
}
