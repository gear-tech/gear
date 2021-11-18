// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

//! Program execution regulation module.

use crate::MessageId;
pub use gcore::exec::{block_height, block_timestamp, gas_available};

pub fn wait() -> ! {
    gcore::exec::wait()
}

pub fn wake(waker_id: MessageId, gas_limit: u64) {
    gcore::exec::wake(waker_id.into(), gas_limit)
}
