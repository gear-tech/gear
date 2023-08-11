// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
        ActorExecutionError, ActorExecutionErrorReplyReason, DispatchResult, DispatchResultKind,
        ExecutionError, SystemExecutionError, WasmExecutionContext,
    },
    configs::{BlockInfo, ExecutionSettings},
    ext::{ProcessorContext, ProcessorExternalities},
};
use actor_system_error::{actor_system_error, ResultExt};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    string::{String, ToString},
    vec::Vec,
};
use gear_backend_common::{
    ActorTerminationReason, BackendExternalities, BackendReport, BackendSyscallError, Environment,
    TerminationReason,
};
use gear_core::{
    code::InstrumentedCode,
    env::Externalities,
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    ids::ProgramId,
    memory::{AllocationsContext, Memory, MemoryError, PageBuf},
    message::{
        ContextSettings, DispatchKind, IncomingDispatch, IncomingMessage, MessageContext,
        WasmEntryPoint,
    },
    pages::{GearPage, PageU32Size, WasmPage},
    program::Program,
    reservation::GasReserver,
};
use scale_info::{
    scale::{self, Decode, Encode},
    TypeInfo,
};

actor_system_error! {
    /// Prepare memory error.
    pub type PrepareMemoryError = ActorSystemError<ActorPrepareMemoryError, SystemPrepareMemoryError>;
}

/// Actor's prepare memory error.
#[derive(Encode, Decode, TypeInfo, Debug, PartialEq, Eq, PartialOrd, Ord, derive_more::Display)]
#[codec(crate = scale)]
pub enum ActorPrepareMemoryError {
    /// Stack end page, which value is specified in WASM code, cannot be bigger than static memory size.
    #[display(fmt = "Stack end page {_0:?} is bigger then WASM static memory size {_1:?}")]
    StackEndPageBiggerWasmMemSize(WasmPage, WasmPage),
    /// Stack is not aligned to WASM page size
    #[display(fmt = "Stack end addr {_0:#x} must be aligned to WASM page size")]
    StackIsNotAligned(u32),
    /// Pages error.
    #[display(fmt = "Pages error: {_0}")]
    Pages(String),
}

/// System's prepare memory error.
#[derive(Debug, Eq, PartialEq, derive_more::Display)]
pub enum SystemPrepareMemoryError {
    /// Mem size less then static pages num
    #[display(fmt = "Mem size less then static pages num")]
    InsufficientMemorySize,
    /// Page with data is not allocated for program
    #[display(fmt = "{_0:?} is not allocated for program")]
    PageIsNotAllocated(GearPage),
    /// Cannot write initial data to wasm memory.
    #[display(fmt = "Cannot write initial data for {_0:?}: {_1}")]
    InitialDataWriteFailed(GearPage, MemoryError),
    /// Pages error.
    #[display(fmt = "Pages error: {_0}")]
    Pages(String),
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

/// Writes initial pages data to memory and prepare memory for execution.
fn prepare_memory<Env, EP>(
    env: &mut Env,
    program_id: ProgramId,
    pages_data: &mut BTreeMap<GearPage, PageBuf>,
    static_pages: WasmPage,
) -> Result<(), PrepareMemoryError>
where
    Env: Environment<EP>,
    Env::Ext: ProcessorExternalities + BackendExternalities,
    EP: WasmEntryPoint,
{
    let stack_end = if let Some(stack_end) = env.stack_end() {
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

    let globals_config = env.globals_config();
    let ctx = env.ext().pages_init_context(globals_config);
    let mem = env.memory();

    // Set initial data for pages
    for (page, data) in pages_data.iter_mut() {
        mem.write(page.offset(), data)
            .map_err(|err| SystemPrepareMemoryError::InitialDataWriteFailed(*page, err))?;
    }

    Env::Ext::check_init_pages_data(pages_data)
        .map_err(|err| SystemPrepareMemoryError::Pages(err.to_string()))?;

    Env::Ext::init_pages_for_program(mem, program_id, stack_end, pages_data, static_pages, ctx)
        .map_actor_err(|err| ActorPrepareMemoryError::Pages(err.to_string()))
        .map_system_err(|err| SystemPrepareMemoryError::Pages(err.to_string()))?;

    Ok(())
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
    E::Ext: ProcessorExternalities + BackendExternalities + 'static,
    <E::Ext as Externalities>::UnrecoverableError: BackendSyscallError,
{
    let WasmExecutionContext {
        gas_counter,
        gas_allowance_counter,
        gas_reserver,
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
    let message_context = MessageContext::new(dispatch.clone(), program_id, msg_ctx_settings);

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
        program_id,
        program_candidates_data: Default::default(),
        program_rents: Default::default(),
        host_fn_weights: settings.host_fn_weights,
        forbidden_funcs: settings.forbidden_funcs,
        mailbox_threshold: settings.mailbox_threshold,
        waitlist_cost: settings.waitlist_cost,
        dispatch_hold_cost: settings.dispatch_hold_cost,
        reserve_for: settings.reserve_for,
        reservation: settings.reservation,
        random_data: settings.random_data,
        rent_cost: settings.rent_cost,
    };

    // Creating externalities.
    let ext = E::Ext::new(context);

    let mut env = E::prepare(
        ext,
        program.raw_code(),
        kind,
        program.code().exports().clone(),
        memory_size,
    )
    .map_system_err(|err| SystemExecutionError::Environment(err.to_string()))
    .map_err_into()?;

    prepare_memory(&mut env, program_id, &mut pages_initial_data, static_pages)
        .map_actor_err(|err| ActorExecutionError {
            gas_amount: env.ext().gas_amount(),
            reason: ActorExecutionErrorReplyReason::PrepareMemory(err),
        })
        .map_err_into()?;

    let post_env = env.into();
    let report = E::execute(post_env)
        .map_system_err(|err| SystemExecutionError::Environment(err.to_string()))
        .map_err_into()?;

    let BackendReport {
        termination_reason,
        memory_wrap: mut memory,
        ext,
    } = report;

    let mut termination = match termination_reason {
        TerminationReason::Actor(reason) => reason,
        TerminationReason::System(reason) => return Err(ExecutionError::System(reason.into())),
    };

    E::Ext::pages_post_execution_actions(&mut memory, &mut termination);

    log::debug!("Termination reason: {:?}", termination);

    let info = ext
        .into_ext_info(&memory)
        .map_err(SystemExecutionError::IntoExtInfo)?;

    E::Ext::check_init_pages_data(&pages_initial_data)
        .map_err(|err| SystemExecutionError::CheckInitPagesData(err.to_string()))?;

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
        E::Ext::pages_to_be_updated(pages_initial_data, info.pages_data, static_pages);

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
        reply_deposits: info.reply_deposits,
        program_candidates,
        program_rents: info.program_rents,
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
    E::Ext: ProcessorExternalities + BackendExternalities + 'static,
    <E::Ext as Externalities>::UnrecoverableError: BackendSyscallError,
    EP: WasmEntryPoint,
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
        gas_reserver: GasReserver::new(&Default::default(), Default::default(), Default::default()),
        value_counter: ValueCounter::new(Default::default()),
        allocations_context: AllocationsContext::new(allocations, static_pages, 512.into()),
        message_context: MessageContext::new(
            IncomingDispatch::new(
                DispatchKind::Handle,
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
                None,
            ),
            program.id(),
            ContextSettings::new(0, 0, 0, 0, 0, 0),
        ),
        block_info,
        max_pages: 512.into(),
        page_costs: Default::default(),
        existential_deposit: Default::default(),
        program_id: program.id(),
        program_candidates_data: Default::default(),
        program_rents: Default::default(),
        host_fn_weights: Default::default(),
        forbidden_funcs: Default::default(),
        mailbox_threshold: Default::default(),
        waitlist_cost: Default::default(),
        dispatch_hold_cost: Default::default(),
        reserve_for: Default::default(),
        reservation: Default::default(),
        random_data: Default::default(),
        system_reservation: Default::default(),
        rent_cost: Default::default(),
    };

    // Creating externalities.
    let ext = E::Ext::new(context);

    let mut env = E::prepare(
        ext,
        program.raw_code(),
        function,
        program.code().exports().clone(),
        memory_size,
    )
    .map_err(|err| err.to_string())?;

    prepare_memory(
        &mut env,
        program.id(),
        &mut pages_initial_data,
        static_pages,
    )
    .map_err(|err| err.to_string())?;

    let post_env = env.into();
    let exec_result = E::execute(post_env);

    let (termination, memory, ext) = match exec_result {
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

    log::debug!(
        "[execute_for_reply] Gas burned: {}",
        info.gas_amount.burned()
    );

    for (dispatch, _, _) in info.generated_dispatches {
        if matches!(dispatch.kind(), DispatchKind::Reply) {
            return Ok(dispatch.payload_bytes().to_vec());
        }
    }

    Err("Reply not found".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ext;
    use alloc::vec::Vec;
    use gear_core::{
        memory::PageBufInner,
        pages::{PageNumber, WasmPage},
    };

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
    fn no_pages_to_update() {
        let old_pages = prepare_pages();
        let mut new_pages = old_pages.clone();
        let static_pages = 4;
        let res = Ext::pages_to_be_updated(old_pages, new_pages.clone(), static_pages.into());
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
        let res = Ext::pages_to_be_updated(Default::default(), new_pages, static_pages.into());
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
        let res = Ext::pages_to_be_updated(old_pages, new_pages.clone(), static_pages);
        assert_eq!(res, changes);

        // There was no any old page
        let res = Ext::pages_to_be_updated(Default::default(), new_pages.clone(), static_pages);

        // The result is all pages except the static ones
        for page in static_pages.to_page::<GearPage>().iter_from_zero() {
            new_pages.remove(&page);
        }

        // Remove page with zero data, because it must not be updated.
        new_pages.remove(&page_with_zero_data);

        assert_eq!(res, new_pages);
    }
}
