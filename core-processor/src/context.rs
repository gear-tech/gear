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

use crate::common::ExecutableActorData;
use gear_core::{
    code::InstrumentedCode,
    gas::{GasAllowanceCounter, GasCounter},
    ids::ProgramId,
    message::IncomingDispatch,
    pages::WasmPage,
    program::Program,
    reservation::GasReserver,
};

pub(crate) struct ContextData {
    pub(crate) gas_counter: GasCounter,
    pub(crate) gas_allowance_counter: GasAllowanceCounter,
    pub(crate) dispatch: IncomingDispatch,
    pub(crate) destination_id: ProgramId,
    pub(crate) actor_data: ExecutableActorData,
}

pub struct ContextChargedForCodeLength {
    pub(crate) data: ContextData,
}

impl ContextChargedForCodeLength {
    /// Returns reference to the ExecutableActorData.
    pub fn actor_data(&self) -> &ExecutableActorData {
        &self.data.actor_data
    }
}

/// The instance returned by `precharge_for_code`.
/// Existence of the instance means that corresponding counters were
/// successfully charged for fetching the binary code from storage.
pub struct ContextChargedForCode {
    pub(crate) data: ContextData,
    pub(crate) code_len_bytes: u32,
}

impl From<(ContextChargedForCodeLength, u32)> for ContextChargedForCode {
    fn from((context, code_len_bytes): (ContextChargedForCodeLength, u32)) -> Self {
        Self {
            data: context.data,
            code_len_bytes,
        }
    }
}

/// The instance returned by `precharge_for_instrumentation`.
/// Existence of the instance means that corresponding counters were
/// successfully charged for reinstrumentation of the code.
pub struct ContextChargedForInstrumentation {
    pub(crate) data: ContextData,
    pub(crate) code_len_bytes: u32,
}

impl From<ContextChargedForCode> for ContextChargedForInstrumentation {
    fn from(context: ContextChargedForCode) -> Self {
        Self {
            data: context.data,
            code_len_bytes: context.code_len_bytes,
        }
    }
}

pub struct ContextChargedForMemory {
    pub(crate) data: ContextData,
    pub(crate) max_reservations: u64,
    pub(crate) memory_size: WasmPage,
}

impl ContextChargedForMemory {
    /// Returns reference to the ExecutableActorData.
    pub fn actor_data(&self) -> &ExecutableActorData {
        &self.data.actor_data
    }

    /// Returns reference to the GasCounter.
    pub fn gas_counter(&self) -> &GasCounter {
        &self.data.gas_counter
    }
}

/// Checked parameters for message execution across processing runs.
pub struct ProcessExecutionContext {
    pub(crate) gas_counter: GasCounter,
    pub(crate) gas_allowance_counter: GasAllowanceCounter,
    pub(crate) gas_reserver: GasReserver,
    pub(crate) dispatch: IncomingDispatch,
    pub(crate) balance: u128,
    pub(crate) program: Program,
    pub(crate) memory_size: WasmPage,
}

impl From<(ContextChargedForMemory, InstrumentedCode, u128)> for ProcessExecutionContext {
    fn from(args: (ContextChargedForMemory, InstrumentedCode, u128)) -> Self {
        let (context, code, balance) = args;

        let ContextChargedForMemory {
            data:
                ContextData {
                    gas_counter,
                    gas_allowance_counter,
                    dispatch,
                    destination_id,
                    actor_data,
                },
            max_reservations,
            memory_size,
        } = context;

        let program = Program::from_parts(
            destination_id,
            actor_data.memory_infix,
            code,
            actor_data.allocations,
            actor_data.initialized,
        );

        // Must be created once per taken from the queue dispatch by contract.
        let gas_reserver =
            GasReserver::new(&dispatch, actor_data.gas_reservation_map, max_reservations);

        Self {
            gas_counter,
            gas_allowance_counter,
            gas_reserver,
            dispatch,
            balance,
            program,
            memory_size,
        }
    }
}

impl ProcessExecutionContext {
    /// Returns ref to program.
    pub fn program(&self) -> &Program {
        &self.program
    }
}

#[derive(Debug, Default)]
pub struct SystemReservationContext {
    /// Reservation created in current execution.
    pub current_reservation: Option<u64>,
    /// Reservation from `ContextStore`.
    pub previous_reservation: Option<u64>,
}

impl SystemReservationContext {
    pub fn from_dispatch(dispatch: &IncomingDispatch) -> Self {
        Self {
            current_reservation: None,
            previous_reservation: dispatch
                .context()
                .as_ref()
                .and_then(|ctx| ctx.system_reservation()),
        }
    }

    pub fn has_any(&self) -> bool {
        self.current_reservation.is_some() || self.previous_reservation.is_some()
    }
}
