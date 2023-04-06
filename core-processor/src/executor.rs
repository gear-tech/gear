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
        ActorExecutionError, ActorExecutionErrorReason, DispatchResult, DispatchResultKind,
        ExecutionError, SystemExecutionError, WasmExecutionContext,
    },
    configs::{BlockInfo, ExecutionSettings},
    ext::{ProcessorContext, ProcessorExt},
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    string::{String, ToString},
    vec::Vec,
};
use gear_backend_common::{
    lazy_pages::{GlobalsAccessConfig, LazyPagesWeights, Status},
    ActorTerminationReason, BackendExt, BackendExtError, BackendReport, Environment,
    EnvironmentExecutionError, TerminationReason, TrapExplanation,
};
use gear_core::{
    code::InstrumentedCode,
    env::Ext,
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    ids::ProgramId,
    memory::{AllocationsContext, GearPage, Memory, PageBuf, PageU32Size, WasmPage},
    message::{
        ContextSettings, DispatchKind, IncomingDispatch, IncomingMessage, MessageContext, WasmEntry,
    },
    program::Program,
    reservation::GasReserver,
};
use gear_core_errors::MemoryError;
use scale_info::{
    scale::{self, Decode, Encode},
    TypeInfo,
};

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum PrepareMemoryError {
    #[display(fmt = "{_0}")]
    Actor(ActorPrepareMemoryError),
    #[display(fmt = "{_0}")]
    System(SystemPrepareMemoryError),
}

/// Prepare memory error
#[derive(Encode, Decode, TypeInfo, Debug, PartialEq, Eq, PartialOrd, Ord, derive_more::Display)]
#[codec(crate = scale)]
pub enum ActorPrepareMemoryError {
    /// Stack end page, which value is specified in WASM code, cannot be bigger than static memory size.
    #[display(fmt = "Stack end page {_0:?} is bigger then WASM static memory size {_1:?}")]
    StackEndPageBiggerWasmMemSize(WasmPage, WasmPage),
    /// It's not allowed to set initial data for stack memory pages, if they are specified in WASM code.
    #[display(fmt = "Set initial data for stack pages is restricted")]
    StackPagesHaveInitialData,
    /// Stack is not aligned to WASM page size
    #[display(fmt = "Stack end addr {_0:#x} must be aligned to WASM page size")]
    StackIsNotAligned(u32),
}

#[derive(Debug, Eq, PartialEq, derive_more::Display)]
pub enum SystemPrepareMemoryError {
    /// Mem size less then static pages num
    #[display(fmt = "Mem size less then static pages num")]
    InsufficientMemorySize,
    /// Page with data is not allocated for program
    #[display(fmt = "{_0:?} is not allocated for program")]
    PageIsNotAllocated(GearPage),
    /// Cannot read initial memory data from wasm memory.
    #[display(fmt = "Cannot read data for {_0:?}: {_1}")]
    InitialMemoryReadFailed(GearPage, MemoryError),
    /// Cannot write initial data to wasm memory.
    #[display(fmt = "Cannot write initial data for {_0:?}: {_1}")]
    InitialDataWriteFailed(GearPage, MemoryError),
    /// Initial pages data must be empty in lazy pages mode
    #[display(fmt = "Initial pages data must be empty when execute with lazy pages")]
    InitialPagesContainsDataInLazyPagesMode,
}

/// Make checks that everything with memory goes well.
fn check_memory<'a>(
    allocations: &BTreeSet<WasmPage>,
    pages_with_data: impl Iterator<Item = &'a GearPage>,
    static_pages: WasmPage,
    memory_size: WasmPage,
) -> Result<(), SystemPrepareMemoryError> {
    if memory_size < static_pages {
        log::error!(
            "Mem size less then static pages num: mem_size = {:?}, static_pages = {:?}",
            memory_size,
            static_pages
        );
        return Err(SystemPrepareMemoryError::InsufficientMemorySize);
    }

    // Checks that all pages with data are in allocations set.
    for page in pages_with_data {
        let wasm_page = page.to_page();
        if wasm_page >= static_pages && !allocations.contains(&wasm_page) {
            return Err(SystemPrepareMemoryError::PageIsNotAllocated(*page));
        }
    }

    Ok(())
}

fn lazy_pages_check_initial_data(
    initial_pages_data: &BTreeMap<GearPage, PageBuf>,
) -> Result<(), SystemPrepareMemoryError> {
    initial_pages_data
        .is_empty()
        .then_some(())
        .ok_or(SystemPrepareMemoryError::InitialPagesContainsDataInLazyPagesMode)
}

/// Writes initial pages data to memory and prepare memory for execution.
fn prepare_memory<A: ProcessorExt, M: Memory>(
    mem: &mut M,
    program_id: ProgramId,
    pages_data: &mut BTreeMap<GearPage, PageBuf>,
    static_pages: WasmPage,
    stack_end: Option<u32>,
    globals_config: GlobalsAccessConfig,
    lazy_pages_weights: LazyPagesWeights,
) -> Result<(), PrepareMemoryError> {
    let stack_end = if let Some(stack_end) = stack_end {
        let stack_end = (stack_end % WasmPage::size() == 0)
            .then_some(WasmPage::from_offset(stack_end))
            .ok_or(ActorPrepareMemoryError::StackIsNotAligned(stack_end))?;

        if stack_end > static_pages {
            return Err(ActorPrepareMemoryError::StackEndPageBiggerWasmMemSize(
                stack_end,
                static_pages,
            )
            .into());
        }

        Some(stack_end)
    } else {
        None
    };

    // Set initial data for pages
    for (page, data) in pages_data.iter_mut() {
        mem.write(page.offset(), data)
            .map_err(|err| SystemPrepareMemoryError::InitialDataWriteFailed(*page, err))?;
    }

    if A::LAZY_PAGES_ENABLED {
        lazy_pages_check_initial_data(pages_data)?;

        A::lazy_pages_init_for_program(
            mem,
            program_id,
            stack_end,
            globals_config,
            lazy_pages_weights,
        );
    } else {
        // If we executes without lazy pages, then we have to save all initial data for static pages,
        // in order to be able to identify pages, which has been changed during execution.
        // Skip stack page if they are specified.
        let begin = stack_end.unwrap_or_default();

        if pages_data.keys().any(|&p| p < begin.to_page()) {
            return Err(ActorPrepareMemoryError::StackPagesHaveInitialData.into());
        }

        let non_stack_pages = begin.iter_end(static_pages).unwrap_or_else(|err| {
            unreachable!(
                "We have already checked that `stack_end` is <= `static_pages`, but get: {}",
                err
            )
        });
        for page in non_stack_pages.flat_map(|p| p.to_pages_iter()) {
            if pages_data.contains_key(&page) {
                // This page already has initial data
                continue;
            }
            let mut data = PageBuf::new_zeroed();
            mem.read(page.offset(), &mut data)
                .map_err(|err| SystemPrepareMemoryError::InitialMemoryReadFailed(page, err))?;
            pages_data.insert(page, data);
        }
    }
    Ok(())
}

/// Returns pages and their new data, which must be updated or uploaded to storage.
fn get_pages_to_be_updated<A: ProcessorExt>(
    old_pages_data: BTreeMap<GearPage, PageBuf>,
    new_pages_data: BTreeMap<GearPage, PageBuf>,
    static_pages: WasmPage,
) -> BTreeMap<GearPage, PageBuf> {
    if A::LAZY_PAGES_ENABLED {
        // In lazy pages mode we update some page data in storage,
        // when it has been write accessed, so no need to compare old and new page data.
        new_pages_data.keys().for_each(|page| {
            log::trace!("{:?} has been write accessed, update it in storage", page)
        });
        return new_pages_data;
    }

    let mut page_update = BTreeMap::new();
    let mut old_pages_data = old_pages_data;
    let static_gear_pages = static_pages.to_page();
    for (page, new_data) in new_pages_data {
        let initial_data = if let Some(initial_data) = old_pages_data.remove(&page) {
            initial_data
        } else {
            // If it's static page without initial data,
            // then it's stack page and we skip this page update.
            if page < static_gear_pages {
                continue;
            }

            // If page has no data in `pages_initial_data` then data is zeros.
            // Because it's default data for wasm pages which is not static,
            // and for all static pages we save data in `pages_initial_data` in E::new.
            PageBuf::new_zeroed()
        };

        if new_data != initial_data {
            page_update.insert(page, new_data);
            log::trace!("{page:?} has been changed - will be updated in storage");
        }
    }
    page_update
}

/// Execute wasm with dispatch and return dispatch result.
pub fn execute_wasm<E>(
    balance: u128,
    dispatch: IncomingDispatch,
    context: WasmExecutionContext,
    settings: ExecutionSettings,
    msg_ctx_settings: ContextSettings,
) -> Result<DispatchResult, ExecutionError>
where
    E: Environment,
    E::Ext: ProcessorExt + BackendExt + 'static,
    <E::Ext as Ext>::Error: BackendExtError,
{
    let WasmExecutionContext {
        gas_counter,
        gas_allowance_counter,
        gas_reserver,
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
    let allocations = program.allocations();

    check_memory(
        allocations,
        pages_initial_data.keys(),
        static_pages,
        memory_size,
    )
    .map_err(SystemExecutionError::PrepareMemory)?;

    // Creating allocations context.
    let allocations_context =
        AllocationsContext::new(allocations.clone(), static_pages, settings.max_pages);

    // Creating message context.
    let message_context = MessageContext::new(
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
        gas_reserver,
        system_reservation: None,
        value_counter,
        allocations_context,
        message_context,
        block_info: settings.block_info,
        max_pages: settings.max_pages,
        page_costs: settings.page_costs,
        existential_deposit: settings.existential_deposit,
        origin,
        program_id,
        program_candidates_data: Default::default(),
        host_fn_weights: settings.host_fn_weights,
        forbidden_funcs: settings.forbidden_funcs,
        mailbox_threshold: settings.mailbox_threshold,
        waitlist_cost: settings.waitlist_cost,
        dispatch_hold_cost: settings.dispatch_hold_cost,
        reserve_for: settings.reserve_for,
        reservation: settings.reservation,
        random_data: settings.random_data,
    };

    let lazy_pages_weights = context.page_costs.lazy_pages_weights();

    // Creating externalities.
    let ext = E::Ext::new(context);

    // Execute program in backend env.
    let execute = || {
        let env = E::new(
            ext,
            program.raw_code(),
            kind,
            program.code().exports().clone(),
            memory_size,
        )
        .map_err(EnvironmentExecutionError::from_infallible)?;
        env.execute(|memory, stack_end, globals_config| {
            prepare_memory::<E::Ext, E::Memory>(
                memory,
                program_id,
                &mut pages_initial_data,
                static_pages,
                stack_end,
                globals_config,
                lazy_pages_weights,
            )
        })
    };
    let (termination, memory, ext) = match execute() {
        Ok(report) => {
            let BackendReport {
                termination_reason,
                memory_wrap: mut memory,
                ext,
            } = report;

            let mut termination = match termination_reason {
                TerminationReason::Actor(reason) => reason,
                TerminationReason::System(reason) => {
                    return Err(ExecutionError::System(reason.into()))
                }
            };

            // released pages initial data will be added to `pages_initial_data` after execution.
            if E::Ext::LAZY_PAGES_ENABLED {
                E::Ext::lazy_pages_post_execution_actions(&mut memory);

                match E::Ext::lazy_pages_status() {
                    Status::Normal => (),
                    Status::GasLimitExceeded => {
                        termination =
                            ActorTerminationReason::Trap(TrapExplanation::GasLimitExceeded);
                    }
                    Status::GasAllowanceExceeded => {
                        termination = ActorTerminationReason::GasAllowanceExceeded;
                    }
                }
            }

            (termination, memory, ext)
        }
        Err(EnvironmentExecutionError::System(e)) => {
            return Err(ExecutionError::System(SystemExecutionError::Environment(
                e.to_string(),
            )))
        }
        Err(EnvironmentExecutionError::PrepareMemory(gas_amount, PrepareMemoryError::Actor(e))) => {
            return Err(ExecutionError::Actor(ActorExecutionError {
                gas_amount,
                reason: ActorExecutionErrorReason::PrepareMemory(e),
            }))
        }
        Err(EnvironmentExecutionError::PrepareMemory(
            _gas_amount,
            PrepareMemoryError::System(e),
        )) => {
            return Err(ExecutionError::System(e.into()));
        }
        Err(EnvironmentExecutionError::Actor(gas_amount, err)) => {
            return Err(ExecutionError::Actor(ActorExecutionError {
                gas_amount,
                reason: ActorExecutionErrorReason::Environment(err.into()),
            }))
        }
    };

    log::debug!("Termination reason: {:?}", termination);

    let info = ext
        .into_ext_info(&memory)
        .map_err(SystemExecutionError::IntoExtInfo)?;

    if E::Ext::LAZY_PAGES_ENABLED {
        lazy_pages_check_initial_data(&pages_initial_data)
            .map_err(SystemExecutionError::PrepareMemory)?;
    }

    // Parsing outcome.
    let kind = match termination {
        ActorTerminationReason::Exit(value_dest) => DispatchResultKind::Exit(value_dest),
        ActorTerminationReason::Leave | ActorTerminationReason::Success => {
            DispatchResultKind::Success
        }
        ActorTerminationReason::Trap(explanation) => {
            log::debug!("💥 Trap during execution of {program_id}\n📔 Explanation: {explanation}");
            DispatchResultKind::Trap(explanation)
        }
        ActorTerminationReason::Wait(duration, waited_type) => {
            DispatchResultKind::Wait(duration, waited_type)
        }
        ActorTerminationReason::GasAllowanceExceeded => DispatchResultKind::GasAllowanceExceed,
    };

    let page_update =
        get_pages_to_be_updated::<E::Ext>(pages_initial_data, info.pages_data, static_pages);

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
        gas_reserver: Some(info.gas_reserver),
        system_reservation_context: info.system_reservation_context,
        page_update,
        allocations: info.allocations,
    })
}

/// !!! FOR TESTING / INFORMATIONAL USAGE ONLY
#[allow(clippy::too_many_arguments)]
pub fn execute_for_reply<E, EP>(
    function: EP,
    instrumented_code: InstrumentedCode,
    pages_initial_data: Option<BTreeMap<GearPage, PageBuf>>,
    allocations: Option<BTreeSet<WasmPage>>,
    program_id: Option<ProgramId>,
    payload: Vec<u8>,
    gas_limit: u64,
    block_info: BlockInfo,
) -> Result<Vec<u8>, String>
where
    E: Environment<EP>,
    E::Ext: ProcessorExt + BackendExt + 'static,
    <E::Ext as Ext>::Error: BackendExtError,
    EP: WasmEntry,
{
    let program = Program::new(program_id.unwrap_or_default(), instrumented_code);
    let mut pages_initial_data: BTreeMap<GearPage, PageBuf> =
        pages_initial_data.unwrap_or_default();
    let static_pages = program.static_pages();
    let allocations = allocations.unwrap_or_else(|| program.allocations().clone());

    let memory_size = if let Some(page) = allocations.iter().next_back() {
        page.inc()
            .map_err(|err| err.to_string())
            .expect("Memory size overflow, impossible")
    } else if static_pages != WasmPage::from(0) {
        static_pages
    } else {
        0.into()
    };

    let context = ProcessorContext {
        gas_counter: GasCounter::new(gas_limit),
        gas_allowance_counter: GasAllowanceCounter::new(gas_limit),
        gas_reserver: GasReserver::new(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        ),
        value_counter: ValueCounter::new(Default::default()),
        allocations_context: AllocationsContext::new(allocations, static_pages, 512.into()),
        message_context: MessageContext::new(
            IncomingMessage::new(
                Default::default(),
                Default::default(),
                payload
                    .try_into()
                    .map_err(|e| format!("Failed to create payload: {e:?}"))?,
                gas_limit,
                Default::default(),
                Default::default(),
            ),
            program.id(),
            None,
            ContextSettings::new(0, 0, 0, 0, 0, 0),
        ),
        block_info,
        max_pages: 512.into(),
        page_costs: Default::default(),
        existential_deposit: Default::default(),
        origin: Default::default(),
        program_id: program.id(),
        program_candidates_data: Default::default(),
        host_fn_weights: Default::default(),
        forbidden_funcs: Default::default(),
        mailbox_threshold: Default::default(),
        waitlist_cost: Default::default(),
        dispatch_hold_cost: Default::default(),
        reserve_for: Default::default(),
        reservation: Default::default(),
        random_data: Default::default(),
        system_reservation: Default::default(),
    };

    let lazy_pages_weights = context.page_costs.lazy_pages_weights();

    // Creating externalities.
    let ext = E::Ext::new(context);

    // Execute program in backend env.
    let f = || {
        let env = E::new(
            ext,
            program.raw_code(),
            function,
            program.code().exports().clone(),
            memory_size,
        )
        .map_err(EnvironmentExecutionError::from_infallible)?;
        env.execute(|memory, stack_end, globals_config| {
            prepare_memory::<E::Ext, E::Memory>(
                memory,
                program.id(),
                &mut pages_initial_data,
                static_pages,
                stack_end,
                globals_config,
                lazy_pages_weights,
            )
        })
    };

    let (termination, memory, ext) = match f() {
        Ok(report) => {
            let BackendReport {
                termination_reason,
                memory_wrap,
                ext,
            } = report;

            let termination_reason = match termination_reason {
                TerminationReason::Actor(reason) => reason,
                TerminationReason::System(reason) => {
                    return Err(format!("Backend error: {reason}"))
                }
            };

            (termination_reason, memory_wrap, ext)
        }
        Err(e) => return Err(format!("Backend error: {e}")),
    };

    match termination {
        ActorTerminationReason::Exit(_)
        | ActorTerminationReason::Leave
        | ActorTerminationReason::Wait(_, _) => {
            return Err("Execution has incorrect termination reason".into())
        }
        ActorTerminationReason::Success => (),
        ActorTerminationReason::Trap(explanation) => {
            return Err(format!(
                "Program execution failed with error: {explanation}"
            ));
        }
        ActorTerminationReason::GasAllowanceExceeded => return Err("Unreachable".into()),
    };

    let info = ext
        .into_ext_info(&memory)
        .map_err(|e| format!("Backend postprocessing error: {e:?}"))?;

    for (dispatch, _, _) in info.generated_dispatches {
        if matches!(dispatch.kind(), DispatchKind::Reply) {
            return Ok(dispatch.payload().to_vec());
        }
    }

    Err("Reply not found".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use gear_backend_common::lazy_pages::Status;
    use gear_core::memory::{PageBufInner, WasmPage};

    struct TestExt;
    struct LazyTestExt;

    impl ProcessorExt for TestExt {
        const LAZY_PAGES_ENABLED: bool = false;
        fn new(_context: ProcessorContext) -> Self {
            Self
        }

        fn lazy_pages_init_for_program(
            _mem: &mut impl Memory,
            _prog_id: ProgramId,
            _stack_end: Option<WasmPage>,
            _globals_config: GlobalsAccessConfig,
            _lazy_pages_weights: LazyPagesWeights,
        ) {
        }

        fn lazy_pages_post_execution_actions(_mem: &mut impl Memory) {}
        fn lazy_pages_status() -> Status {
            Status::Normal
        }
    }

    impl ProcessorExt for LazyTestExt {
        const LAZY_PAGES_ENABLED: bool = true;

        fn new(_context: ProcessorContext) -> Self {
            Self
        }

        fn lazy_pages_init_for_program(
            _mem: &mut impl Memory,
            _prog_id: ProgramId,
            _stack_end: Option<WasmPage>,
            _globals_config: GlobalsAccessConfig,
            _lazy_pages_weights: LazyPagesWeights,
        ) {
        }

        fn lazy_pages_post_execution_actions(_mem: &mut impl Memory) {}
        fn lazy_pages_status() -> Status {
            Status::Normal
        }
    }

    fn prepare_pages_and_allocs() -> (Vec<GearPage>, BTreeSet<WasmPage>) {
        let data = [0u16, 1, 2, 8, 18, 25, 27, 28, 93, 146, 240, 518];
        let pages = data.map(Into::into);
        (pages.to_vec(), pages.map(|p| p.to_page()).into())
    }

    fn prepare_pages() -> BTreeMap<GearPage, PageBuf> {
        let mut pages = BTreeMap::new();
        for i in 0..=255 {
            let buffer = PageBufInner::filled_with(i);
            pages.insert((i as u16).into(), PageBuf::from_inner(buffer));
        }
        pages
    }

    #[test]
    fn check_memory_insufficient() {
        let res = check_memory(&[].into(), [].iter(), 8.into(), 4.into());
        assert_eq!(res, Err(SystemPrepareMemoryError::InsufficientMemorySize));
    }

    #[test]
    fn check_memory_not_allocated() {
        let (pages, mut allocs) = prepare_pages_and_allocs();
        let last = *allocs.iter().last().unwrap();
        allocs.remove(&last);
        let res = check_memory(&allocs, pages.iter(), 2.into(), 4.into());
        assert_eq!(
            res,
            Err(SystemPrepareMemoryError::PageIsNotAllocated(
                *pages.last().unwrap()
            ))
        );
    }

    #[test]
    fn check_memory_ok() {
        let (pages, allocs) = prepare_pages_and_allocs();
        check_memory(&allocs, pages.iter(), 4.into(), 8.into()).unwrap();
    }

    #[test]
    fn lazy_pages_to_update() {
        let new_pages = prepare_pages();
        let res =
            get_pages_to_be_updated::<LazyTestExt>(Default::default(), new_pages.clone(), 0.into());
        // All touched pages are to be updated in lazy mode
        assert_eq!(res, new_pages);
    }

    #[test]
    fn no_pages_to_update() {
        let old_pages = prepare_pages();
        let mut new_pages = old_pages.clone();
        let static_pages = 4;
        let res =
            get_pages_to_be_updated::<TestExt>(old_pages, new_pages.clone(), static_pages.into());
        assert_eq!(res, Default::default());

        // Change static pages
        for i in 0..static_pages {
            let buffer = PageBufInner::filled_with(42);
            new_pages.insert(i.into(), PageBuf::from_inner(buffer));
        }
        // Do not include non-static pages
        let new_pages = new_pages
            .into_iter()
            .take(WasmPage::from(static_pages).to_page::<GearPage>().raw() as _)
            .collect();
        let res =
            get_pages_to_be_updated::<TestExt>(Default::default(), new_pages, static_pages.into());
        assert_eq!(res, Default::default());
    }

    #[test]
    fn pages_to_update() {
        let old_pages = prepare_pages();
        let mut new_pages = old_pages.clone();

        let page_with_zero_data = WasmPage::from(30).to_page();
        let changes: BTreeMap<GearPage, PageBuf> = [
            (
                WasmPage::from(1).to_page(),
                PageBuf::from_inner(PageBufInner::filled_with(42u8)),
            ),
            (
                WasmPage::from(5).to_page(),
                PageBuf::from_inner(PageBufInner::filled_with(84u8)),
            ),
            (page_with_zero_data, PageBuf::new_zeroed()),
        ]
        .into_iter()
        .collect();
        new_pages.extend(changes.clone().into_iter());

        // Change pages
        let static_pages = 4.into();
        let res = get_pages_to_be_updated::<TestExt>(old_pages, new_pages.clone(), static_pages);
        assert_eq!(res, changes);

        // There was no any old page
        let res =
            get_pages_to_be_updated::<TestExt>(Default::default(), new_pages.clone(), static_pages);

        // The result is all pages except the static ones
        for page in static_pages.to_page::<GearPage>().iter_from_zero() {
            new_pages.remove(&page);
        }

        // Remove page with zero data, because it must not be updated.
        new_pages.remove(&page_with_zero_data);

        assert_eq!(res, new_pages);
    }
}
