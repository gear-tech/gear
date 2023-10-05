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

use gear_wasm_instrument::syscalls::{ParamType, PtrInfo, PtrType, SysCallName, SysCallSignature};

/// Type of invocable sys-call.
///
/// Basically, there are 2 types of generated sys-calls:
/// 1. Those invocation of which is done regardless of validity of call context (`Loose`).
/// 2. Those which are invoked correctly with implementing all call context (`Precise`).
///
/// Clarifying that, `gr_reservation_send` requires an existing reservation id,
/// which is pretty hard to predict beforehand with a generator. So this call context
/// is created from scratch - first `gr_reserve_gas` is called and then it's result
/// is used for the further `gr_reservation_send` call. Those are `Precise` sys-calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InvocableSysCall {
    Loose(SysCallName),
    Precise(SysCallName),
}

impl InvocableSysCall {
    pub(crate) fn to_str(self) -> &'static str {
        match self {
            InvocableSysCall::Loose(sys_call) => sys_call.to_str(),
            InvocableSysCall::Precise(sys_call) => match sys_call {
                SysCallName::ReservationSend => "precise_gr_reservation_send",
                SysCallName::ReservationReply => "precise_gr_reservation_reply",
                SysCallName::SendCommit => "precise_gr_send_commit",
                SysCallName::SendCommitWGas => "precise_gr_send_commit_wgas",
                _ => unimplemented!(),
            },
        }
    }

    fn into_signature(self) -> SysCallSignature {
        match self {
            InvocableSysCall::Loose(name) => name.signature(),
            InvocableSysCall::Precise(name) => match name {
                SysCallName::ReservationSend => SysCallSignature::gr([
                    // Address of recipient and value (HashWithValue struct)
                    ParamType::Ptr(PtrInfo::new_immutable(PtrType::HashWithValue)),
                    // Pointer to payload
                    ParamType::Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                        length_param_idx: 2,
                    })),
                    // Size of the payload
                    ParamType::Size,
                    // Number of blocks to delay the sending for
                    ParamType::Delay,
                    // Amount of gas to reserve
                    ParamType::Gas,
                    // Duration of the reservation
                    ParamType::Duration,
                    // Address of error returned
                    ParamType::Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash)),
                ]),
                SysCallName::ReservationReply => SysCallSignature::gr([
                    // Address of value
                    ParamType::Ptr(PtrInfo::new_immutable(PtrType::Value)),
                    // Pointer to payload
                    ParamType::Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                        length_param_idx: 2,
                    })),
                    // Size of the payload
                    ParamType::Size,
                    // Amount of gas to reserve
                    ParamType::Gas,
                    // Duration of the reservation
                    ParamType::Duration,
                    // Address of error returned
                    ParamType::Ptr(PtrInfo::new_mutable(PtrType::ErrorWithHash)),
                ]),
                SysCallName::SendCommit => SysCallSignature::gr([
                    // Address of recipient and value (HashWithValue struct)
                    ParamType::Ptr(PtrInfo::new_immutable(PtrType::HashWithValue)),
                    // Address of value
                    ParamType::Ptr(PtrInfo::new_immutable(PtrType::Value)),
                    // Pointer to payload
                    ParamType::Ptr(PtrInfo::new_immutable(PtrType::BufferStart {
                        length_param_idx: 2,
                    })),
                    // Size of the payload
                    ParamType::Size,
                    // Number of blocks to delay the sending for
                    ParamType::Delay,
                    // Address of error returned, `ErrorCode` here because underlying syscalls have different error types
                    ParamType::Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
                ]),
                SysCallName::SendCommitWGas => SysCallSignature::gr([
                    // Address of recipient and value (HashWithValue struct)
                    ParamType::Ptr(PtrInfo::new_immutable(PtrType::HashWithValue)),
                    // Number of blocks to delay the sending for
                    ParamType::Delay,
                    // Amount of gas to reserve
                    ParamType::Gas,
                    // Address of error returned, `ErrorCode` here because underlying syscalls have different error types
                    ParamType::Ptr(PtrInfo::new_mutable(PtrType::ErrorCode)),
                ]),
                _ => unimplemented!(),
            },
        }
    }

    /// Checks whether given sys-call has the precise variant.
    pub(crate) fn has_precise_variant(sys_call: SysCallName) -> bool {
        Self::required_imports_for_sys_call(sys_call).is_some()
    }

    /// Returns the required imports to build precise sys-call.
    pub(crate) fn required_imports_for_sys_call(
        sys_call: SysCallName,
    ) -> Option<&'static [SysCallName]> {
        // NOTE: the last sys-call must be pattern itself
        Some(match sys_call {
            SysCallName::ReservationSend => {
                &[SysCallName::ReserveGas, SysCallName::ReservationSend]
            }
            SysCallName::ReservationReply => {
                &[SysCallName::ReserveGas, SysCallName::ReservationReply]
            }
            SysCallName::SendCommit => &[
                SysCallName::SendInit,
                SysCallName::SendPush,
                SysCallName::SendCommit,
            ],
            SysCallName::SendCommitWGas => &[
                SysCallName::Size,
                SysCallName::SendInit,
                SysCallName::SendPushInput,
                SysCallName::SendCommitWGas,
            ],
            _ => return None,
        })
    }

    /// Returns the required imports to build precise sys-call, but of a fixed size.
    fn required_imports<const N: usize>(sys_call: SysCallName) -> &'static [SysCallName; N] {
        Self::required_imports_for_sys_call(sys_call)
            .expect("failed to find required imports for sys-call")
            .try_into()
            .expect("failed to convert slice")
    }

    /// Returns the index of the destination param.
    fn has_destination_param(&self) -> Option<usize> {
        use InvocableSysCall::*;
        use SysCallName::*;

        match *self {
            Loose(Send | SendWGas | SendInput | SendInputWGas | Exit)
            | Precise(ReservationSend | SendCommit | SendCommitWGas) => Some(0),
            Loose(SendCommit | SendCommitWGas) => Some(1),
            _ => None,
        }
    }

    /// This fn assumes that fallible syscalls will have pointer to the returned error
    /// among their params.
    pub(crate) fn is_fallible(&self) -> bool {
        let params = self.into_signature().params;
        params.into_iter().any(|param| {
            matches!(
                param,
                ParamType::Ptr(PtrInfo {
                    mutable: true,
                    ty
                }) if ty.is_error()
            )
        })
    }
}
