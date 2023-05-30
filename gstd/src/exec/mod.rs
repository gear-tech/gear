// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
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

//! Utility functions related to the current execution context or program
//! execution flow.
//!
//! Wraps methods from [`gcore::exec`](https://docs.gear.rs/gcore/exec/)
//! for receiving details about the current execution and controlling it.

pub use basic::*;
pub use gcore::exec::{
    block_height, block_timestamp, gas_available, leave, random, system_reserve_gas,
    value_available, wait, wait_for, wait_up_to,
};
pub use r#async::*;

mod r#async;
mod basic;
