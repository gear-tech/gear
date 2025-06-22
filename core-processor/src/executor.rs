// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
        ExecutionError, Program, SystemExecutionError, WasmExecutionContext,
    },
    configs::{BlockInfo, ExecutionSettings},
    ext::{ProcessorContext, ProcessorExternalities},
};
use alloc::{format, string::String, vec::Vec};
use gear_core::{
    code::InstrumentedCode,
    env::{Externalities, WasmEntryPoint},
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    ids::ActorId,
    memory::AllocationsContext,
    message::{ContextSettings, DispatchKind, IncomingDispatch, IncomingMessage, MessageContext},
    pages::{numerated::tree::IntervalsTree, WasmPage},
    program::MemoryInfix,
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

/// Execute wasm with dispatch and return dispatch result.
pub(crate) fn execute_wasm<Ext>(
    balance: u128,
    dispatch: IncomingDispatch,
    context: WasmExecutionContext,
    settings: ExecutionSettings,
    msg_ctx_settings: ContextSettings,
) -> Result<DispatchResult, ExecutionError>
where
    Ext: ProcessorExternalities + BackendExternalities + Send + 'static,
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

    // TODO: consider avoiding cloning here.
    let initial_gas_reserver = gas_reserver.clone();

    let kind = dispatch.kind();

    log::debug!("Executing program {}", program.id);
    log::debug!("Executing dispatch {dispatch:?}");

    // Creating allocations context.
    let allocations_context = AllocationsContext::try_new(
        memory_size,
        program.allocations,
        program.code.static_pages(),
        program.code.stack_end(),
        settings.max_pages,
    )
    .map_err(SystemExecutionError::from)?;

    // Creating message context.
    let message_context = MessageContext::new(dispatch.clone(), program.id, msg_ctx_settings);

    // Creating value counter.
    //
    // NOTE: Value available equals free balance with message value if value
    // wasn't transferred to program yet.
    //
    // In case of second execution (between waits) - message value already
    // included in free balance or wasted.
    let value_available = balance.saturating_add(if dispatch.context().is_none() {
        dispatch.value()
    } else {
        Default::default()
    });
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
        program_id: program.id,
        program_candidates_data: Default::default(),
        forbidden_funcs: settings.forbidden_funcs,
        reserve_for: settings.reserve_for,
        random_data: settings.random_data,
        gas_multiplier: settings.gas_multiplier,
        existential_deposit: settings.existential_deposit,
        mailbox_threshold: settings.mailbox_threshold,
        costs: settings.ext_costs,
    };

    // Creating externalities.
    let ext = Ext::new(context);

    // Execute program in backend env.
    let execute = || {
        let env = Environment::new(
            ext,
            program.code.code(),
            kind,
            program.code.exports().clone(),
            memory_size,
        )?;
        env.execute(|ctx, memory, globals_config| {
            Ext::lazy_pages_init_for_program(
                ctx,
                memory,
                program.id,
                program.memory_infix,
                program.code.stack_end(),
                globals_config,
                settings.lazy_pages_costs,
            )
        })
    };

    let (termination, mut store, memory, ext) = match execute() {
        Ok(report) => {
            let BackendReport {
                termination_reason,
                mut store,
                mut memory,
                ext,
            } = report;

            let mut termination = match termination_reason {
                TerminationReason::Actor(reason) => reason,
                TerminationReason::System(reason) => {
                    return Err(ExecutionError::System(reason.into()));
                }
            };

            // released pages initial data will be added to `pages_initial_data` after execution.
            Ext::lazy_pages_post_execution_actions(&mut store, &mut memory);

            if !Ext::lazy_pages_status().is_normal() {
                termination = ext.current_counter_type().into()
            }

            (termination, store, memory, ext)
        }
        Err(EnvironmentError::System(e)) => {
            return Err(ExecutionError::System(SystemExecutionError::Environment(e)));
        }
        Err(EnvironmentError::Actor(gas_amount, err)) => {
            log::trace!("ActorExecutionErrorReplyReason::Environment({err}) occurred");
            return Err(ExecutionError::Actor(ActorExecutionError {
                gas_amount,
                reason: ActorExecutionErrorReplyReason::Environment,
            }));
        }
    };

    log::debug!("Termination reason: {termination:?}");

    let info = ext
        .into_ext_info(&mut store, &memory)
        .map_err(SystemExecutionError::IntoExtInfo)?;

    // Parsing outcome.
    let kind = match termination {
        ActorTerminationReason::Exit(value_dest) => DispatchResultKind::Exit(value_dest),
        ActorTerminationReason::Leave | ActorTerminationReason::Success => {
            DispatchResultKind::Success
        }
        ActorTerminationReason::Trap(explanation) => {
            log::debug!(
                "💥 Trap during execution of {}\n📔 Explanation: {explanation}",
                program.id
            );
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

    // Triggering update of gas reserver only in case of it changed.
    let gas_reserver = initial_gas_reserver
        .ne(&info.gas_reserver)
        .then_some(info.gas_reserver);

    // Output
    Ok(DispatchResult {
        kind,
        dispatch,
        program_id: program.id,
        context_store: info.context_store,
        generated_dispatches: info.generated_dispatches,
        awakening: info.awakening,
        reply_deposits: info.reply_deposits,
        program_candidates,
        gas_amount: info.gas_amount,
        gas_reserver,
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
    allocations: Option<IntervalsTree<WasmPage>>,
    program_info: Option<(ActorId, MemoryInfix)>,
    payload: Vec<u8>,
    gas_limit: u64,
    block_info: BlockInfo,
) -> Result<Vec<u8>, String>
where
    Ext: ProcessorExternalities + BackendExternalities + Send + 'static,
    <Ext as Externalities>::AllocError:
        BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
    RunFallibleError: From<Ext::FallibleError>,
    <Ext as Externalities>::UnrecoverableError: BackendSyscallError,
    EP: WasmEntryPoint,
{
    let (program_id, memory_infix) = program_info.unwrap_or_default();
    let program = Program {
        id: program_id,
        memory_infix,
        code: instrumented_code,
        allocations: allocations.unwrap_or_default(),
    };
    let static_pages = program.code.static_pages();
    let memory_size = program
        .allocations
        .end()
        .map(|p| p.inc())
        .unwrap_or(static_pages);

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
        program.id,
        Default::default(),
    );

    let context = ProcessorContext {
        gas_counter: GasCounter::new(gas_limit),
        gas_allowance_counter: GasAllowanceCounter::new(gas_limit),
        gas_reserver: GasReserver::new(&Default::default(), Default::default(), Default::default()),
        value_counter: ValueCounter::new(Default::default()),
        allocations_context: AllocationsContext::try_new(
            memory_size,
            program.allocations,
            static_pages,
            program.code.stack_end(),
            512.into(),
        )
        .map_err(|e| format!("Failed to create alloc ctx: {e:?}"))?,
        message_context,
        block_info,
        performance_multiplier: gsys::Percent::new(100),
        program_id: program.id,
        program_candidates_data: Default::default(),
        forbidden_funcs: Default::default(),
        reserve_for: Default::default(),
        random_data: Default::default(),
        system_reservation: Default::default(),
        gas_multiplier: gsys::GasMultiplier::from_value_per_gas(100),
        existential_deposit: Default::default(),
        mailbox_threshold: Default::default(),
        costs: Default::default(),
    };

    // Creating externalities.
    let ext = Ext::new(context);

    // Execute program in backend env.
    let execute = || {
        let env = Environment::new(
            ext,
            program.code.code(),
            function,
            program.code.exports().clone(),
            memory_size,
        )?;
        env.execute(|ctx, memory, globals_config| {
            Ext::lazy_pages_init_for_program(
                ctx,
                memory,
                program_id,
                program.memory_infix,
                program.code.stack_end(),
                globals_config,
                Default::default(),
            )
        })
    };

    let (termination, mut store, memory, ext) = match execute() {
        Ok(report) => {
            let BackendReport {
                termination_reason,
                store,
                memory,
                ext,
            } = report;

            let termination_reason = match termination_reason {
                TerminationReason::Actor(reason) => reason,
                TerminationReason::System(reason) => {
                    return Err(format!("Backend error: {reason}"));
                }
            };

            (termination_reason, store, memory, ext)
        }
        Err(e) => return Err(format!("Backend error: {e}")),
    };

    match termination {
        ActorTerminationReason::Exit(_)
        | ActorTerminationReason::Leave
        | ActorTerminationReason::Wait(_, _) => {
            return Err("Execution has incorrect termination reason".into());
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
        .into_ext_info(&mut store, &memory)
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
