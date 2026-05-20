// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Module contains context-structures for processing.

use crate::{
    common::Program,
    precharge::{ContextCharged, ForModuleInstantiation},
};
use gear_core::{
    code::{InstrumentedCodeAndMetadata, SyscallKind},
    gas::{GasAllowanceCounter, GasCounter},
    ids::ActorId,
    message::IncomingDispatch,
    pages::WasmPagesAmount,
    program::MemoryInfix,
    reservation::GasReserver,
};

/// Checked parameters for message execution across processing runs.
pub struct ProcessExecutionContext {
    pub(crate) gas_counter: GasCounter,
    pub(crate) gas_allowance_counter: GasAllowanceCounter,
    pub(crate) gas_reserver: GasReserver,
    pub(crate) dispatch: IncomingDispatch,
    pub(crate) balance: u128,
    pub(crate) program: Program,
    pub(crate) memory_size: WasmPagesAmount,
    pub(crate) syscall_kind: SyscallKind,
}

impl ProcessExecutionContext {
    /// Creates a new instance of the process execution context.
    pub fn new(
        context: ContextCharged<ForModuleInstantiation>,
        instrumented_code_and_metadata: InstrumentedCodeAndMetadata,
        balance: u128,
        syscall_kind: SyscallKind,
    ) -> Self {
        let (
            destination_id,
            dispatch,
            gas_counter,
            gas_allowance_counter,
            actor_data,
            allocations_data,
        ) = context.into_final_parts();

        let program = Program {
            id: destination_id,
            memory_infix: actor_data.memory_infix,
            instrumented_code: instrumented_code_and_metadata.instrumented_code,
            code_metadata: instrumented_code_and_metadata.metadata,
            allocations: actor_data.allocations,
        };

        // Must be created once per taken from the queue dispatch by program.
        let gas_reserver = GasReserver::new(
            &dispatch,
            actor_data.gas_reservation_map,
            allocations_data.max_reservations,
        );

        Self {
            gas_counter,
            gas_allowance_counter,
            gas_reserver,
            dispatch,
            balance,
            program,
            memory_size: allocations_data.memory_size,
            syscall_kind,
        }
    }

    /// Returns program id.
    pub fn program_id(&self) -> ActorId {
        self.program.id
    }

    /// Returns memory infix.
    pub fn memory_infix(&self) -> MemoryInfix {
        self.program.memory_infix
    }
}

/// System reservation context.
#[derive(Debug, Default)]
pub struct SystemReservationContext {
    /// Reservation created in current execution.
    pub current_reservation: Option<u64>,
    /// Reservation from `ContextStore`.
    pub previous_reservation: Option<u64>,
}

impl SystemReservationContext {
    /// Extracts reservation context from dispatch.
    pub fn from_dispatch(dispatch: &IncomingDispatch) -> Self {
        Self {
            current_reservation: None,
            previous_reservation: dispatch
                .context()
                .as_ref()
                .and_then(|ctx| ctx.system_reservation()),
        }
    }

    /// Checks if there are any reservations.
    pub fn has_any(&self) -> bool {
        self.current_reservation.is_some() || self.previous_reservation.is_some()
    }
}
