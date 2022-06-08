#![no_std]

use gstd::{prog::program_generator::ProgramGenerator, CodeHash};

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let submitted_code: CodeHash =
        hex_literal::hex!("abf3746e72a6e8740bd9e12b879fbdd59e052cb390f116454e9116c22021ae4a")
            .into();

    ProgramGenerator::create_program_with_gas(submitted_code, b"payload", 10_000_000_000, 0);
}
