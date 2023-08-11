// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

use gsys::{ErrorWithHash, HashWithValue};

pub(crate) struct BackendErrorState;

pub(crate) fn init() -> BackendErrorState {
    // Code below is copied and simplified from `gcore::msg::send`.
    let pid_value = HashWithValue {
        hash: [0; 32],
        value: 0,
    };

    let mut res: ErrorWithHash = Default::default();

    // u32::MAX ptr + 42 len of the payload triggers error of payload read.
    unsafe {
        gsys::gr_send(
            pid_value.as_ptr(),
            u32::MAX as *const u8,
            42,
            0,
            res.as_mut_ptr(),
        )
    };

    assert!(res.error_code != 0);

    BackendErrorState
}
