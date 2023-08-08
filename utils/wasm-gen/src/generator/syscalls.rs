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

//! Sys-calls generators entities.
//!
//! Generators from this module form a state machine:
//! ```text
//! # Zero sys-calls generators nesting level.
//! SysCallsImport--->DisabledSysCallsImport--->ModuleWithCallIndexes--->WasmModule
//!
//! # First sys-calls generators nesting level.
//! SysCallsImport--->DisabledSysCallsImport--(SysCallsImportsGenerationProof)-->AdditionalDataInjector---\
//! |--->DisabledAdditionalDataInjector--->ModuleWithCallIndexes--->WasmModule
//!
//! # Third sys-calls generators nesting level
//! SysCallsImport--->DisabledSysCallsImport--(SysCallsImportsGenerationProof)-->AdditionalDataInjector---\
//! |--->DisabledAdditionalDataInjector--(AddressesInjectionOutcome)-->SysCallsInvocator--->DisabledSysCallsInvocator--->ModuleWithCallIndexes--->WasmModule
//! ```
//! Entities in curly brackets are those, which are required for the next transition.
//! Also all transitions require previous entity to be disabled.

mod additional_data;
mod imports;
mod invocator;

pub use additional_data::*;
pub use imports::*;
pub use invocator::*;

use gear_wasm_instrument::syscalls::{ParamType, SysCallName, SysCallSignature};

/// Type of invocable sys-call.
///
/// Basically, there are 2 types of generated sys-calls:
/// 1. Those invocation of which is done regardless of validity of call context (`Loose`).
/// 2. Those which are invoked correctly with implenting all call context (`Precise`).
///
/// Clarifying that, `gr_reservation_send` requires an existing reservation id,
/// which is pretty hard to predict beforehand with a generator. So this call context
/// is created from scratch - first `gr_reserve_gas` is called and then it's result
/// is used for the further `gr_reservation_send` call. Those are `Precise` sys-calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum InvocableSysCall {
    Loose(SysCallName),
    Precise(SysCallName),
}

impl InvocableSysCall {
    fn into_signature(self) -> SysCallSignature {
        match self {
            InvocableSysCall::Loose(name) => name.signature(),
            InvocableSysCall::Precise(name) => match name {
                SysCallName::ReservationSend => SysCallSignature::gr([
                    ParamType::Ptr(None),    // Address of recipient and value (HashWithValue struct)
                    ParamType::Ptr(Some(2)), // Pointer to payload
                    ParamType::Size,         // Size of the payload
                    ParamType::Delay,        // Number of blocks to delay the sending for
                    ParamType::Gas,          // Amount of gas to reserve
                    ParamType::Duration,     // Duration of the reservation
                ]),
                _ => panic!("unknown implementation"),
            },
        }
    }
}
