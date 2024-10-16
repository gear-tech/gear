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

//! Module contains context-structures for processing.

use crate::{
    common::Program,
    precharge::{ContextCharged, ForModuleInstantiation},
};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    gas::{GasAllowanceCounter, GasCounter},
    ids::ProgramId,
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
}

impl ProcessExecutionContext {
    /// Returns program id.
    pub fn program_id(&self) -> ProgramId {
        self.program.id
    }

    /// Returns memory infix.
    pub fn memory_infix(&self) -> MemoryInfix {
        self.program.memory_infix
    }
}

impl
    From<(
        ContextCharged<ForModuleInstantiation>,
        InstrumentedCode,
        CodeMetadata,
        u128,
    )> for ProcessExecutionContext
{
    fn from(
        args: (
            ContextCharged<ForModuleInstantiation>,
            InstrumentedCode,
            CodeMetadata,
            u128,
        ),
    ) -> Self {
        let (context, code, code_metadata, balance) = args;

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
            code,
            code_metadata,
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
        }
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
