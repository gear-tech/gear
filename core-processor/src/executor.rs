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

//! WASM execution and dispatch result processing.
//!
//! This module provides the core execution functionality for running WASM programs:
//!
//! # Single Execution
//!
//! For executing a single dispatch, use [`execute_wasm`]:
//! ```ignore
//! let result = execute_wasm(
//!     balance, dispatch, context, settings, msg_ctx_settings,
//! )?;
//! ```
//!
//! # Sequential Execution
//!
//! For executing multiple dispatches with environment reuse, use [`execute_wasm_step`]
//! with [`SequenceState`]:
//!
//! ```ignore
//! let mut state = SequenceState::new();
//! let mut snapshot = Ext::memory_snapshot();
//!
//! for dispatch in queue {
//!     let result = execute_wasm_step(
//!         &mut state, &mut snapshot,
//!         balance, dispatch, context, settings, msg_ctx_settings,
//!     )?;
//!     // process result...
//! }
//! ```
//!
//! Sequential execution reuses the WASM environment between dispatches, avoiding
//! the overhead of instance creation. Memory snapshots are used to restore state
//! after failed executions.
//!
//! # Key Types
//!
//! - [`SequenceState`] - Holds the reusable environment between sequential executions
//! - [`ExecutionStep`] - Result of a single execution step with gas info
//! - [`ExecutionSequenceError`] - Error type for sequential execution (alias for [`ExecutionError`])

use crate::{
    common::{
        ActorExecutionError, ActorExecutionErrorReplyReason, DispatchResult, DispatchResultKind,
        ExecutableActorData, ExecutionError, Program, ReservationsAndMemorySize,
        SystemExecutionError, WasmExecutionContext,
    },
    configs::{BlockInfo, ExecutionSettings},
    ext::{ProcessorContext, ProcessorExternalities},
};
use alloc::{format, string::String, vec::Vec};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    env::{Externalities, WasmEntryPoint},
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    ids::ActorId,
    memory::AllocationsContext,
    message::{ContextSettings, DispatchKind, IncomingDispatch, IncomingMessage, MessageContext},
    pages::{WasmPage, WasmPagesAmount, numerated::tree::IntervalsTree},
    program::MemoryInfix,
    reservation::GasReserver,
};
use gear_core_backend::{
    BackendExternalities, MemorySnapshot, MemorySnapshotStrategy, NoopSnapshot,
    env::{
        BackendReport, Environment, EnvironmentError, ExecutedEnvironment, ExecutionReport,
        PostExecution, ReadyToExecute,
    },
    error::{
        ActorTerminationReason, BackendAllocSyscallError, BackendSyscallError, RunFallibleError,
        TerminationReason,
    },
};

// Keep the actor/system logging consistent across call sites to aid debugging.
fn log_actor_environment_error(message: &str) {
    log::trace!("ActorExecutionErrorReplyReason::Environment({message}) occurred");
}

// Convert backend-specific errors into execution errors with consistent logging semantics.
fn execution_error_from_environment(err: EnvironmentError) -> ExecutionError {
    match err {
        EnvironmentError::System(system_err) => {
            ExecutionError::System(SystemExecutionError::Environment(system_err))
        }
        EnvironmentError::Actor(gas_amount, err_msg) => {
            log_actor_environment_error(&err_msg);
            ExecutionError::Actor(ActorExecutionError {
                gas_amount,
                reason: ActorExecutionErrorReplyReason::Environment,
            })
        }
    }
}

// Provide a human-readable error message when bubbling backend failures to callers.
fn backend_error_message(err: EnvironmentError) -> String {
    match err {
        EnvironmentError::System(system_err) => format!("Backend error: {system_err}"),
        EnvironmentError::Actor(gas_amount, err_msg) => {
            log_actor_environment_error(&err_msg);
            format!("Backend actor error (gas_amount={gas_amount:?}): {err_msg}")
        }
    }
}

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

    let context = build_processor_context(
        balance,
        dispatch,
        &program,
        memory_size,
        program.allocations.clone(),
        &settings,
        msg_ctx_settings,
        gas_counter,
        gas_allowance_counter,
        gas_reserver,
    )?;

    let env = create_environment::<Ext>(context, &program, memory_size, &settings)?;

    let mut memory_snapshot = Ext::memory_snapshot();
    let execution_result = execute_environment(
        env,
        kind,
        MemorySnapshotStrategy::enabled(&mut memory_snapshot),
    )?;

    finalize_execution::<Ext>(execution_result, &program, initial_gas_reserver)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_processor_context(
    balance: u128,
    dispatch: IncomingDispatch,
    program: &Program,
    memory_size: WasmPagesAmount,
    allocations: IntervalsTree<WasmPage>,
    settings: &ExecutionSettings,
    msg_ctx_settings: ContextSettings,
    gas_counter: GasCounter,
    gas_allowance_counter: GasAllowanceCounter,
    gas_reserver: GasReserver,
) -> Result<ProcessorContext, ExecutionError> {
    // Creating allocations context.
    let allocations_context = AllocationsContext::try_new(
        memory_size,
        allocations,
        program.code_metadata.static_pages(),
        program.code_metadata.stack_end(),
        settings.max_pages,
    )
    .map_err(SystemExecutionError::from)?;

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

    // Creating message context.
    let message_context = MessageContext::new(dispatch, program.id, msg_ctx_settings);

    Ok(ProcessorContext {
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
        forbidden_funcs: settings.forbidden_funcs.clone(),
        reserve_for: settings.reserve_for,
        random_data: settings.random_data.clone(),
        gas_multiplier: settings.gas_multiplier,
        existential_deposit: settings.existential_deposit,
        mailbox_threshold: settings.mailbox_threshold,
        costs: settings.ext_costs.clone(),
    })
}

pub(crate) fn create_environment<'a, Ext>(
    context: ProcessorContext,
    program: &'a Program,
    memory_size: WasmPagesAmount,
    settings: &ExecutionSettings,
) -> Result<Environment<'a, Ext, ReadyToExecute>, ExecutionError>
where
    Ext: ProcessorExternalities + BackendExternalities + Send + 'static,
    <Ext as Externalities>::AllocError:
        BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
    RunFallibleError: From<Ext::FallibleError>,
    <Ext as Externalities>::UnrecoverableError: BackendSyscallError,
{
    let ext = Ext::new(context);
    let lazy_pages_costs = settings.lazy_pages_costs.clone();
    Environment::new(
        ext,
        program.instrumented_code.bytes(),
        program.code_metadata.exports().clone(),
        memory_size,
        |ctx, memory, globals_config| {
            Ext::lazy_pages_init_for_program(
                ctx,
                memory,
                program.id,
                program.memory_infix,
                program.code_metadata.stack_end(),
                globals_config,
                lazy_pages_costs,
            )
        },
    )
    .map_err(execution_error_from_environment)
}

pub(crate) fn execute_environment<'a, Ext, M: MemorySnapshot>(
    env: Environment<'a, Ext, ReadyToExecute>,
    kind: DispatchKind,
    memory_snapshot: MemorySnapshotStrategy<'_, M>,
) -> Result<ExecutedEnvironment<'a, Ext>, ExecutionError>
where
    Ext: ProcessorExternalities + BackendExternalities + Send + 'static,
    <Ext as Externalities>::AllocError:
        BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
    RunFallibleError: From<Ext::FallibleError>,
    <Ext as Externalities>::UnrecoverableError: BackendSyscallError,
{
    env.execute(kind, memory_snapshot)
        .map_err(execution_error_from_environment)
}

pub(crate) fn finalize_execution<Ext>(
    execution_result: ExecutedEnvironment<'_, Ext>,
    program: &Program,
    initial_gas_reserver: GasReserver,
) -> Result<DispatchResult, ExecutionError>
where
    Ext: ProcessorExternalities + BackendExternalities + Send + 'static,
    <Ext as Externalities>::AllocError:
        BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
    RunFallibleError: From<Ext::FallibleError>,
    <Ext as Externalities>::UnrecoverableError: BackendSyscallError,
{
    let BackendReport {
        termination_reason,
        mut store,
        mut memory,
        ext,
    } = execution_result
        .report()
        // Propagate backend issues into the processor domain so the caller gets full context.
        .map_err(execution_error_from_environment)?;

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

/// Execution step input for sequence processing.
#[derive(Debug)]
pub struct ExecutionStep {
    /// Program balance before execution.
    pub balance: u128,
    /// Incoming dispatch to execute.
    pub dispatch: IncomingDispatch,
    /// Allocations snapshot for the dispatch.
    pub allocations: IntervalsTree<WasmPage>,
    /// Gas counter for the dispatch.
    pub gas_counter: GasCounter,
    /// Gas allowance counter for the dispatch.
    pub gas_allowance_counter: GasAllowanceCounter,
    /// Gas reserver for the dispatch.
    pub gas_reserver: GasReserver,
}

/// Data cached after first successful dispatch execution.
///
/// This data is reused for subsequent dispatches in the same sequence,
/// avoiding repeated DB reads and gas charges.
#[derive(Clone, Debug)]
pub struct CachedExecutionData {
    /// Actor data (allocations, memory infix, gas reservations).
    pub actor_data: ExecutableActorData,
    /// Memory size configuration.
    pub reservations_and_memory_size: ReservationsAndMemorySize,
}

/// Persistent state between sequence steps.
pub struct SequenceState<'a, Ext: BackendExternalities> {
    post_env: Option<Environment<'a, Ext, PostExecution>>,
    has_snapshot: bool,
    /// Cached execution data for subsequent dispatches.
    /// `None` indicates this is the first dispatch in the sequence.
    cached: Option<CachedExecutionData>,
}

impl<'a, Ext: BackendExternalities> SequenceState<'a, Ext> {
    /// Creates a new sequence state.
    pub fn new() -> Self {
        Self {
            post_env: None,
            has_snapshot: false,
            cached: None,
        }
    }

    /// Returns `true` if this is the first dispatch in the sequence.
    #[inline]
    pub fn is_first_dispatch(&self) -> bool {
        self.cached.is_none()
    }

    /// Returns cached execution data if available.
    pub fn cached_data(&self) -> Option<&CachedExecutionData> {
        self.cached.as_ref()
    }

    /// Caches execution data after a successful first dispatch.
    ///
    /// This should be called after the first dispatch completes successfully,
    /// so subsequent dispatches can reuse the data without charging.
    pub fn cache_execution_data(&mut self, data: CachedExecutionData) {
        self.cached = Some(data);
    }

    /// Updates cached allocations after a dispatch that modified them.
    pub fn update_cached_allocations(&mut self, allocations: IntervalsTree<WasmPage>) {
        if let Some(ref mut cached) = self.cached {
            cached.actor_data.allocations = allocations;
        }
    }
}

impl<'a, Ext: BackendExternalities> Default for SequenceState<'a, Ext> {
    fn default() -> Self {
        Self::new()
    }
}

/// Sequence execution error.
pub type ExecutionSequenceError = ExecutionError;

fn dispatch_result_from_report<Ext>(
    report: ExecutionReport<Ext>,
    env: &mut Environment<'_, Ext, PostExecution>,
    program: &Program,
    initial_gas_reserver: GasReserver,
) -> Result<DispatchResult, ExecutionError>
where
    Ext: ProcessorExternalities + BackendExternalities + Send + 'static,
    <Ext as Externalities>::AllocError:
        BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
    RunFallibleError: From<Ext::FallibleError>,
    <Ext as Externalities>::UnrecoverableError: BackendSyscallError,
{
    let ExecutionReport {
        termination_reason,
        ext,
    } = report;

    let mut termination = match termination_reason {
        TerminationReason::Actor(reason) => reason,
        TerminationReason::System(reason) => {
            return Err(ExecutionError::System(reason.into()));
        }
    };

    env.with_store_and_memory_mut(|store, memory| {
        Ext::lazy_pages_post_execution_actions(store, memory);
    });

    if !Ext::lazy_pages_status().is_normal() {
        termination = ext.current_counter_type().into()
    }

    log::debug!("Termination reason: {termination:?}");

    let info = env.with_store_and_memory_mut(|store, memory| {
        ext.into_ext_info(store, memory)
            .map_err(SystemExecutionError::IntoExtInfo)
    })?;

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

    let page_update = info.pages_data;
    let program_candidates = info.program_candidates_data;

    let gas_reserver = initial_gas_reserver
        .ne(&info.gas_reserver)
        .then_some(info.gas_reserver);

    Ok(DispatchResult {
        kind,
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

/// Executes a single step, reusing environment state from previous steps.
///
/// Memory snapshots are always enabled to ensure correct state restoration
/// after failed dispatches in sequential execution.
#[allow(clippy::too_many_arguments)]
pub fn execute_wasm_step<'a, Ext, M: MemorySnapshot>(
    step: ExecutionStep,
    program: &'a Program,
    memory_size: WasmPagesAmount,
    settings: &ExecutionSettings,
    msg_ctx_settings: ContextSettings,
    state: &mut SequenceState<'a, Ext>,
    snapshot: &mut M,
) -> Result<DispatchResult, ExecutionSequenceError>
where
    Ext: ProcessorExternalities + BackendExternalities + Send + 'static,
    <Ext as Externalities>::AllocError:
        BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
    RunFallibleError: From<Ext::FallibleError>,
    <Ext as Externalities>::UnrecoverableError: BackendSyscallError,
{
    let ExecutionStep {
        balance,
        dispatch,
        allocations,
        gas_counter,
        gas_allowance_counter,
        gas_reserver,
    } = step;
    let kind = dispatch.kind();
    let initial_gas_reserver = gas_reserver.clone();
    let context = build_processor_context(
        balance,
        dispatch,
        program,
        memory_size,
        allocations,
        settings,
        msg_ctx_settings,
        gas_counter,
        gas_allowance_counter,
        gas_reserver,
    )?;

    let env = if let Some(post_env) = state.post_env.take() {
        // Set lazy pages protection for the new dispatch.
        // This clears page tracking and re-protects pages without losing the
        // in-memory data, avoiding unnecessary DB fetches.
        Ext::lazy_pages_set_protection();

        let ext = Ext::new(context);
        post_env
            .set_ext(ext)
            .map_err(execution_error_from_environment)?
    } else {
        create_environment::<Ext>(context, program, memory_size, settings)?
    };

    let execution = execute_environment(env, kind, MemorySnapshotStrategy::enabled(snapshot))?;

    let wasm_succeeded = matches!(execution, ExecutedEnvironment::SuccessExecution(_));
    if wasm_succeeded {
        state.has_snapshot = true;
    }

    let (report, mut post_env) = execution
        .into_report_and_env()
        .map_err(execution_error_from_environment)?;
    let result = dispatch_result_from_report(report, &mut post_env, program, initial_gas_reserver)?;

    if !wasm_succeeded && state.has_snapshot {
        post_env
            .restore_snapshot(snapshot)
            .map_err(|err| ExecutionError::System(SystemExecutionError::IntoExtInfo(err)))?;
    }

    state.post_env = Some(post_env);

    Ok(result)
}

/// !!! FOR TESTING / INFORMATIONAL USAGE ONLY
#[allow(clippy::too_many_arguments)]
pub fn execute_for_reply<Ext, EP>(
    function: EP,
    instrumented_code: InstrumentedCode,
    code_metadata: CodeMetadata,
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
        instrumented_code,
        code_metadata,
        allocations: allocations.unwrap_or_default(),
    };
    let static_pages = program.code_metadata.static_pages();
    let memory_size = program
        .allocations
        .end()
        .map(|p| p.inc())
        .unwrap_or(static_pages);

    let incoming_dispatch = IncomingDispatch::new(
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
    );

    let gas_reserver = GasReserver::new(&incoming_dispatch, Default::default(), Default::default());

    let message_context = MessageContext::new(incoming_dispatch, program.id, Default::default());

    let context = ProcessorContext {
        gas_counter: GasCounter::new(gas_limit),
        gas_allowance_counter: GasAllowanceCounter::new(gas_limit),
        gas_reserver,
        value_counter: ValueCounter::new(Default::default()),
        allocations_context: AllocationsContext::try_new(
            memory_size,
            program.allocations,
            static_pages,
            program.code_metadata.stack_end(),
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

    let env = Environment::new(
        ext,
        program.instrumented_code.bytes(),
        program.code_metadata.exports().clone(),
        memory_size,
        |ctx, memory, globals_config| {
            Ext::lazy_pages_init_for_program(
                ctx,
                memory,
                program_id,
                program.memory_infix,
                program.code_metadata.stack_end(),
                globals_config,
                Default::default(),
            )
        },
    )
    .map_err(backend_error_message)?;

    let execution_result = env
        .execute(function, MemorySnapshotStrategy::<NoopSnapshot>::disabled())
        .map_err(backend_error_message)?;

    let BackendReport {
        termination_reason,
        mut store,
        memory,
        ext,
    } = execution_result
        .report()
        // Keep the error surface identical to the other executor path.
        .map_err(backend_error_message)?;

    let termination = match termination_reason {
        TerminationReason::Actor(reason) => reason,
        TerminationReason::System(reason) => {
            return Err(format!("Backend error: {reason}"));
        }
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
