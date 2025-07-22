// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use gstd::{CodeId, collections::BTreeSet, prelude::*, prog::ProgramGenerator};

fn check_salt_uniqueness() {
    let salts: Vec<_> = (0..10).map(|_| ProgramGenerator::get_salt()).collect();
    let salts_len = salts.len();

    // The set's length should be equal to the vector's one
    // if there are no repetitive values.
    let salts_set: BTreeSet<_> = salts.into_iter().collect();
    assert_eq!(salts_len, salts_set.len());
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    let submitted_code: CodeId =
        hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a")
            .into();

    ProgramGenerator::create_program_bytes_with_gas(submitted_code, b"payload", 10_000_000_000, 0)
        .unwrap();

    check_salt_uniqueness();
}
