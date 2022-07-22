// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use sp_runtime::{DispatchError, ModuleError};

/// Handle the errors from the `GasTree` operations
///
/// # NOTE
///
/// Only handles `NodeAlreadyExists` for now.
pub fn gas_tree(e: DispatchError) {
    if let DispatchError::Module(ModuleError {
        // The index of `pallet_gear_gas` in runtime.
        index: 12,
        error,
        ..
    }) = e
    {
        match error {
            // `pallet_gear_gas::Error::NodeAlreadyExists`
            //
            // # TODO
            //
            // provide tests for this is reachable. for examples `send_wgas` to a samein
            // message serveral times in a message.
            [1, 0, 0, 0] => {}
            _ => unreachable!(
                "GasTree corrupted! unreachable since these have been checked before: {:?}",
                e
            ),
        }
    } else {
        unreachable!("This implementation of `GasProvider` in `pallet_gear` is `pallet_gear_gas` which uses module Error as InternalError.");
    }
}
