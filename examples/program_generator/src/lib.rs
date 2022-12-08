#![no_std]

extern crate alloc;

use alloc::{collections::BTreeSet, vec::Vec};
use gstd::{prog::ProgramGenerator, CodeId};

fn salt_uniqueness_test() {
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

    salt_uniqueness_test();

    ProgramGenerator::create_program_with_gas(submitted_code, b"payload", 10_000_000_000, 0)
        .unwrap();
}
