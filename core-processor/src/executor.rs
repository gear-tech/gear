// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
use actor_system_error::actor_system_error;
use alloc::{
    collections::BTreeSet,
    format,
    string::{String, ToString},
    vec::Vec,
};
use gear_core::{
    code::InstrumentedCode,
    env::Externalities,
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    ids::ProgramId,
    memory::{AllocationsContext, Memory},
    message::{
        ContextSettings, DispatchKind, IncomingDispatch, IncomingMessage, MessageContext,
        WasmEntryPoint,
    },
    pages::{PageU32Size, WasmPage},
    program::{MemoryInfix, Program},
    reservation::GasReserver,
};
use gear_core_backend::{
    env::{BackendReport, Environment, EnvironmentError},
    error::{
        ActorTerminationReason, BackendAllocSyscallError, BackendSyscallError, RunFallibleError,
        TerminationReason,
    },
    memory::MemoryWrap,
    BackendExternalities,
};
use gear_lazy_pages_common::{GlobalsAccessConfig, LazyPagesWeights};
use scale_info::{
    scale::{self, Decode, Encode},
    TypeInfo,
};

actor_system_error! {
    /// Prepare memory error.
    pub type PrepareMemoryError = ActorSystemError<ActorPrepareMemoryError, SystemPrepareMemoryError>;
}

// TODO: add this cases checks to Code::try_new in gear-core #3735
/// Prepare memory error
#[derive(Encode, Decode, TypeInfo, Debug, PartialEq, Eq, PartialOrd, Ord, derive_more::Display)]
#[codec(crate = scale)]
pub enum ActorPrepareMemoryError {
    /// Stack end page, which value is specified in WASM code, cannot be bigger than static memory size.
    #[display(fmt = "Stack end page {_0:?} is bigger then WASM static memory size {_1:?}")]
    StackEndPageBiggerWasmMemSize(WasmPage, WasmPage),
    /// Stack is not aligned to WASM page size
    #[display(fmt = "Stack end addr {_0:#x} must be aligned to WASM page size")]
    StackIsNotAligned(u32),
}

#[derive(Debug, Eq, PartialEq, derive_more::Display)]
pub enum SystemPrepareMemoryError {
    /// Mem size less than static pages num
    #[display(fmt = "Mem size less than static pages num")]
    InsufficientMemorySize,
}

/// Make checks that everything with memory goes well.
fn check_memory(
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

    Ok(())
}

/// Writes initial pages data to memory and prepare memory for execution.
#[allow(clippy::too_many_arguments)]
fn prepare_memory<ProcessorExt: ProcessorExternalities, EnvMem: Memory>(
    mem: &mut EnvMem,
    program_id: ProgramId,
    memory_infix: MemoryInfix,
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

    ProcessorExt::lazy_pages_init_for_program(
        mem,
        program_id,
        memory_infix,
        stack_end,
        globals_config,
        lazy_pages_weights,
    );

    Ok(())
}

/// Execute wasm with dispatch and return dispatch result.
pub fn execute_wasm<Ext>(
    balance: u128,
    dispatch: IncomingDispatch,
    context: WasmExecutionContext,
    settings: ExecutionSettings,
    msg_ctx_settings: ContextSettings,
) -> Result<DispatchResult, ExecutionError>
where
    Ext: ProcessorExternalities + BackendExternalities + 'static,
    <Ext as Externalities>::AllocError:
        BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
    RunFallibleError: From<Ext::FallibleError>,
    <Ext as Externalities>::UnrecoverableError: BackendSyscallError,
{
    let WasmExecutionContext {
        gas_counter,
        gas_allowance_counter,
        gas_reserver,
        program,
        memory_size,
    } = context;

    let program_id = program.id();
    let kind = dispatch.kind();

    log::debug!("Executing program {}", program_id);
    log::debug!("Executing dispatch {:?}", dispatch);

    let static_pages = program.static_pages();
    let allocations = program.allocations();

    check_memory(static_pages, memory_size).map_err(SystemExecutionError::PrepareMemory)?;

    // Creating allocations context.
    let allocations_context =
        AllocationsContext::new(allocations.clone(), static_pages, settings.max_pages);

    // Creating message context.
    let Some(message_context) = MessageContext::new(dispatch.clone(), program_id, msg_ctx_settings)
    else {
        return Err(ActorExecutionError {
            gas_amount: gas_counter.to_amount(),
            reason: ActorExecutionErrorReplyReason::UnsupportedMessage,
        }
        .into());
    };

    // Creating value counter.
    //
    // NOTE: Value available equals free balance with message value if value
    // wasn't transferred to program yet.
    //
    // In case of second execution (between waits) - message value already
    // included in free balance or wasted.
    let value_available = balance.saturating_add(
        dispatch
            .context()
            .is_none()
            .then(|| dispatch.value())
            .unwrap_or_default(),
    );
    let value_counter = ValueCounter::new(value_available);

    let context = ProcessorContext {
        gas_counter,
        gas_allowance_counter,
        gas_reserver,
        system_reservation: None,
        value_counter,
        allocations_context,
        message_context,
        block_info: settings.block_info,
        performance_multiplier: settings.performance_multiplier,
        max_pages: settings.max_pages,
        page_costs: settings.page_costs,
        existential_deposit: settings.existential_deposit,
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
        gas_multiplier: settings.gas_multiplier,
    };

    let lazy_pages_weights = context.page_costs.lazy_pages_weights();

    // Creating externalities.
    let ext = Ext::new(context);

    // Execute program in backend env.
    let execute = || {
        let env = Environment::new(
            ext,
            program.code_bytes(),
            kind,
            program.code().exports().clone(),
            memory_size,
        )
        .map_err(EnvironmentError::from_infallible)?;
        env.execute(|memory, stack_end, globals_config| {
            prepare_memory::<Ext, MemoryWrap<_>>(
                memory,
                program_id,
                program.memory_infix(),
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
            Ext::lazy_pages_post_execution_actions(&mut memory);

            if !Ext::lazy_pages_status().is_normal() {
                termination = ext.current_counter_type().into()
            }

            (termination, memory, ext)
        }
        Err(EnvironmentError::System(e)) => {
            return Err(ExecutionError::System(SystemExecutionError::Environment(e)))
        }
        Err(EnvironmentError::PrepareMemory(gas_amount, PrepareMemoryError::Actor(e))) => {
            return Err(ExecutionError::Actor(ActorExecutionError {
                gas_amount,
                reason: ActorExecutionErrorReplyReason::PrepareMemory(e),
            }))
        }
        Err(EnvironmentError::PrepareMemory(_gas_amount, PrepareMemoryError::System(e))) => {
            return Err(ExecutionError::System(e.into()));
        }
        Err(EnvironmentError::Actor(gas_amount, err)) => {
            log::trace!("ActorExecutionErrorReplyReason::Environment({err}) occurred");
            return Err(ExecutionError::Actor(ActorExecutionError {
                gas_amount,
                reason: ActorExecutionErrorReplyReason::Environment,
            }));
        }
    };

    log::debug!("Termination reason: {:?}", termination);

    let info = ext
        .into_ext_info(&memory)
        .map_err(SystemExecutionError::IntoExtInfo)?;

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

    // With lazy-pages we update some page data in storage,
    // when it has been write accessed, so no need to compare old and new page data.
    let page_update = info.pages_data;

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
        gas_amount: info.gas_amount,
        gas_reserver: Some(info.gas_reserver),
        system_reservation_context: info.system_reservation_context,
        page_update,
        allocations: info.allocations,
        reply_sent: info.reply_sent,
    })
}

/// !!! FOR TESTING / INFORMATIONAL USAGE ONLY
#[allow(clippy::too_many_arguments)]
pub fn execute_for_reply<Ext, EP>(
    function: EP,
    instrumented_code: InstrumentedCode,
    allocations: Option<BTreeSet<WasmPage>>,
    program_info: Option<(ProgramId, MemoryInfix)>,
    payload: Vec<u8>,
    gas_limit: u64,
    block_info: BlockInfo,
) -> Result<Vec<u8>, String>
where
    Ext: ProcessorExternalities + BackendExternalities + 'static,
    <Ext as Externalities>::AllocError:
        BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
    RunFallibleError: From<Ext::FallibleError>,
    <Ext as Externalities>::UnrecoverableError: BackendSyscallError,
    EP: WasmEntryPoint,
{
    let (program_id, memory_infix) = program_info.unwrap_or_default();
    let program = Program::new(program_id, memory_infix, instrumented_code);
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

    let message_context = MessageContext::new(
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
        Default::default(),
    )
    .ok_or("Incorrect message store context: out of outgoing bytes limit")?;

    let context = ProcessorContext {
        gas_counter: GasCounter::new(gas_limit),
        gas_allowance_counter: GasAllowanceCounter::new(gas_limit),
        gas_reserver: GasReserver::new(&Default::default(), Default::default(), Default::default()),
        value_counter: ValueCounter::new(Default::default()),
        allocations_context: AllocationsContext::new(allocations, static_pages, 512.into()),
        message_context,
        block_info,
        performance_multiplier: gsys::Percent::new(100),
        max_pages: 512.into(),
        page_costs: Default::default(),
        existential_deposit: Default::default(),
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
        gas_multiplier: gsys::GasMultiplier::from_value_per_gas(1),
    };

    let lazy_pages_weights = context.page_costs.lazy_pages_weights();

    // Creating externalities.
    let ext = Ext::new(context);

    // Execute program in backend env.
    let f = || {
        let env = Environment::new(
            ext,
            program.code_bytes(),
            function,
            program.code().exports().clone(),
            memory_size,
        )
        .map_err(EnvironmentError::from_infallible)?;
        env.execute(|memory, stack_end, globals_config| {
            prepare_memory::<Ext, MemoryWrap<_>>(
                memory,
                program_id,
                memory_infix,
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

    #[test]
    fn check_memory_insufficient() {
        let res = check_memory(8.into(), 4.into());
        assert_eq!(res, Err(SystemPrepareMemoryError::InsufficientMemorySize));
    }

    #[test]
    fn check_memory_ok() {
        check_memory(4.into(), 8.into()).unwrap();
    }
}
