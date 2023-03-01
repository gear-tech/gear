// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! gear api utils
use crate::{metadata::runtime_types::sp_runtime::DispatchError, result::Result, Api};
use parity_scale_codec::Encode;
use subxt::error::{DispatchError as SubxtDispatchError, Error, ModuleError, ModuleErrorData};

impl Api {
    /// compare gas limit
    pub fn cmp_gas_limit(&self, gas: u64) -> Result<u64> {
        if let Ok(limit) = self.gas_limit() {
            Ok(if gas > limit {
                log::warn!("gas limit too high, use {} from the chain config", limit);
                limit
            } else {
                gas
            })
        } else {
            Ok(gas)
        }
    }

    /// Decode `DispatchError` to `subxt::error::Error`.
    pub fn decode_error(&self, dispatch_error: DispatchError) -> Error {
        if let DispatchError::Module(ref err) = dispatch_error {
            if let Ok(error_details) = self.metadata().error(err.index, err.error[0]) {
                return SubxtDispatchError::Module(ModuleError {
                    pallet: error_details.pallet().to_string(),
                    error: error_details.error().to_string(),
                    description: error_details.docs().to_vec(),
                    error_data: ModuleErrorData {
                        pallet_index: err.index,
                        error: err.error,
                    },
                })
                .into();
            }
        }

        SubxtDispatchError::Other(dispatch_error.encode()).into()
    }
}
