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
        ExecutionError, MemoryParamsError, SystemExecutionError, WasmExecutionContext,
    },
    configs::{BlockInfo, ExecutionSettings},
    ext::{ProcessorContext, ProcessorExternalities},
};
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
    memory::AllocationsContext,
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
    BackendExternalities,
};

fn validate_memory_params(
    memory_size: WasmPage,
    static_pages: WasmPage,
    stack_end: Option<WasmPage>,
    allocations: &BTreeSet<WasmPage>,
    max_pages: WasmPage,
) -> Result<(), MemoryParamsError> {
    if memory_size > max_pages {
        return Err(MemoryParamsError::MemorySizeExceedsMaxPages(
            memory_size,
            max_pages,
        ));
    }

    if static_pages > memory_size {
        return Err(MemoryParamsError::InsufficientMemorySize(
            memory_size,
            static_pages,
        ));
    }

    if let Some(stack_end) = stack_end {
        if stack_end > static_pages {
            return Err(MemoryParamsError::StackEndOutOfStaticMemory(
                stack_end,
                static_pages,
            ));
        }
    }

    if let Some(&page) = allocations.last() {
        if page >= memory_size {
            return Err(MemoryParamsError::AllocatedPageOutOfAllowedInterval(
                page,
                static_pages,
                memory_size,
            ));
        }
    }
    if let Some(&page) = allocations.first() {
        if page < static_pages {
            return Err(MemoryParamsError::AllocatedPageOutOfAllowedInterval(
                page,
                static_pages,
                memory_size,
            ));
        }
    }

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

    // TODO: move to `AllocationContext::new` #3813
    validate_memory_params(
        memory_size,
        program.static_pages(),
        program.stack_end(),
        program.allocations(),
        settings.max_pages,
    )
    .map_err(SystemExecutionError::from)?;

    // Creating allocations context.
    let allocations_context = AllocationsContext::new(
        program.allocations().clone(),
        program.static_pages(),
        settings.max_pages,
    );

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
        Environment::new(
            ext,
            program.code_bytes(),
            kind,
            program.code().exports().clone(),
            memory_size,
        )?
        .execute(|memory, globals_config| {
            Ext::lazy_pages_init_for_program(
                memory,
                program_id,
                program.memory_infix(),
                program.stack_end(),
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
    let execute = || {
        Environment::new(
            ext,
            program.code_bytes(),
            function,
            program.code().exports().clone(),
            memory_size,
        )?
        .execute(|memory, globals_config| {
            Ext::lazy_pages_init_for_program(
                memory,
                program_id,
                program.memory_infix(),
                program.stack_end(),
                globals_config,
                lazy_pages_weights,
            )
        })
    };

    let (termination, memory, ext) = match execute() {
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
    use core::iter;

    #[test]
    fn memory_params_validation() {
        assert_eq!(
            validate_memory_params(
                4.into(),
                2.into(),
                Some(2.into()),
                &iter::once(2.into()).collect(),
                4.into(),
            ),
            Ok(())
        );

        assert_eq!(
            validate_memory_params(
                4.into(),
                2.into(),
                Some(2.into()),
                &BTreeSet::new(),
                3.into(),
            ),
            Err(MemoryParamsError::MemorySizeExceedsMaxPages(
                4.into(),
                3.into()
            ))
        );

        assert_eq!(
            validate_memory_params(
                1.into(),
                2.into(),
                Some(1.into()),
                &BTreeSet::new(),
                4.into(),
            ),
            Err(MemoryParamsError::InsufficientMemorySize(
                1.into(),
                2.into()
            ))
        );

        assert_eq!(
            validate_memory_params(
                4.into(),
                2.into(),
                Some(3.into()),
                &BTreeSet::new(),
                4.into(),
            ),
            Err(MemoryParamsError::StackEndOutOfStaticMemory(
                3.into(),
                2.into()
            ))
        );

        assert_eq!(
            validate_memory_params(
                4.into(),
                2.into(),
                Some(2.into()),
                &iter::once(1.into()).collect(),
                4.into(),
            ),
            Err(MemoryParamsError::AllocatedPageOutOfAllowedInterval(
                1.into(),
                2.into(),
                4.into()
            ))
        );

        assert_eq!(
            validate_memory_params(
                4.into(),
                2.into(),
                Some(2.into()),
                &iter::once(4.into()).collect(),
                4.into(),
            ),
            Err(MemoryParamsError::AllocatedPageOutOfAllowedInterval(
                4.into(),
                2.into(),
                4.into()
            ))
        );
    }
}
