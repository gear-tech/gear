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

//! This module contains pallet tests usually defined under "std" feature in the separate `tests` module.
//! The reason of moving them here is an ability to run these tests with different execution environments
//! (native or wasm, i.e. using wasmi or sandbox executors). When "std" is enabled we can run them on wasmi,
//! when it's not (only "runtime-benchmarks") - sandbox will be turned on.

use super::*;

pub mod syscalls_integrity;
mod utils;

#[cfg(feature = "lazy-pages")]
pub mod lazy_pages;
