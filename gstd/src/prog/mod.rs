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

//! Program creation module

pub mod program_generator;

use crate::{prelude::convert::AsRef, ActorId, CodeHash};

pub fn create_program_with_gas<T1: AsRef<[u8]>, T2: AsRef<[u8]>>(
    code_hash: CodeHash,
    salt: T1,
    payload: T2,
    gas_limit: u64,
    value: u128,
) -> ActorId {
    gcore::prog::create_program_with_gas(
        code_hash.into(),
        salt.as_ref(),
        payload.as_ref(),
        gas_limit,
        value,
    )
    .into()
}
