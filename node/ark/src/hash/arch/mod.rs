// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

//! Hardware acceleration for certain platforms.
//! Derived from the implementation in the [`plonky2`](https://crates.io/crates/plonky2) crate.

// TODO: x86_64 specific code has been disabled even in the upstream repo due to the MDS
// matrix and round constants change (see https://github.com/0xPolygonZero/plonky2/pull/493).
// Hence not porting the x86_64 specific code for now.

#[cfg(target_arch = "x86_64")]
pub(crate) mod x86_64 {}

#[cfg(target_arch = "aarch64")]
pub(crate) mod aarch64;
