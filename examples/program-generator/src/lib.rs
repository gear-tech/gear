// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

#![no_std]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

pub const CHILD_WAT: &str = r#"
(module
    (type (;0;) (func))
    (import "env" "memory" (memory (;0;) 1))
    (func (;0;) (type 0))
    (func (;1;) (type 0))
    (export "handle" (func 0))
    (export "init" (func 1))
)
"#;

#[cfg(not(feature = "std"))]
mod wasm {
    use gstd::{collections::BTreeSet, prelude::*, prog::ProgramGenerator, CodeId};

    fn check_salt_uniqueness() {
        let salts: Vec<_> = (0..10).map(|_| ProgramGenerator::get_salt()).collect();
        let salts_len = salts.len();

        // The set's length should be equal to the vector's one
        // if there are no repetitive values.
        let salts_set: BTreeSet<_> = salts.into_iter().collect();
        assert_eq!(salts_len, salts_set.len());
    }

    #[no_mangle]
    extern "C" fn handle() {
        let submitted_code: CodeId =
            hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a")
                .into();

        ProgramGenerator::create_program_with_gas(submitted_code, b"payload", 10_000_000_000, 0)
            .unwrap();

        check_salt_uniqueness();
    }
}
