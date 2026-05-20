// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
