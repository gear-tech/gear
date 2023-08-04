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

use std::collections::BTreeMap;

use gear_wasm_instrument::{
    parity_wasm::elements::{FunctionType, ValueType},
    syscalls::{SysCallName, SysCallSignature},
};

use crate::config::{process_sys_call_params, ProcessedSysCallParams, SysCallsParamsConfig};

/// Syscall function info and config.
#[derive(Debug)]
pub struct CallInfo {
    /// Syscall signature params.
    pub params: Vec<ValueType>,
    /// Syscall signature results.
    pub results: Vec<ValueType>,
    /// Syscall allowed input values.
    pub(crate) parameter_rules: Vec<ProcessedSysCallParams>,
}

impl CallInfo {
    pub fn new(call_signature: CallSignature, params_config: &SysCallsParamsConfig) -> Self {
        let signature = call_signature.signature();
        Self {
            params: signature.params.iter().copied().map(Into::into).collect(),
            results: signature.results.to_vec(),
            parameter_rules: process_sys_call_params(&signature.params, params_config),
        }
    }

    pub fn func_type(&self) -> FunctionType {
        FunctionType::new(self.params.clone(), self.results.clone())
    }
}

pub enum CallSignature {
    // Derivable signature from `SysCallName` type.
    Standard(SysCallName),
    // Custom sys-call signature.
    Custom(SysCallSignature),
}

impl CallSignature {
    fn signature(&self) -> SysCallSignature {
        match self {
            CallSignature::Standard(name) => name.signature(),
            CallSignature::Custom(signature) => signature.clone(),
        }
    }
}

/// Make syscalls table for given config.
pub(crate) fn sys_calls_table(
    params_config: &SysCallsParamsConfig,
) -> BTreeMap<SysCallName, CallInfo> {
    SysCallName::instrumentable()
        .into_iter()
        .map(|name| {
            (
                name,
                CallInfo::new(CallSignature::Standard(name), params_config),
            )
        })
        .collect()
}
