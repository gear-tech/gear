// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! Default salt creation module

use codec::alloc::vec::Vec;
use gcore::exec::block_height;

static mut DEFAULT_SALT_COUNTER: u32 = 0;

pub fn create_default_salt() -> Vec<u8> {
    unsafe {
        DEFAULT_SALT_COUNTER += 1;
    }
    [
        crate::msg::id().inner() as &[u8],
        &block_height().to_be_bytes(),
        &unsafe { DEFAULT_SALT_COUNTER }.to_be_bytes(),
    ]
    .concat()
}
