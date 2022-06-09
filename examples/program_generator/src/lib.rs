#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use gstd::{prog::ProgramGenerator, CodeHash};

fn salt_uniqueness_test() {
    let n = 10;
    let salts: Vec<Vec<u8>> = (0..n).map(|_| ProgramGenerator::get_salt()).collect();

    for i in 0..n {
        for j in (i + 1)..n {
            assert_ne!(salts[i], salts[j]);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let submitted_code: CodeHash =
        hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a")
            .into();

    salt_uniqueness_test();

    ProgramGenerator::create_program_with_gas(submitted_code, b"payload", 10_000_000_000, 0)
        .unwrap();
}
