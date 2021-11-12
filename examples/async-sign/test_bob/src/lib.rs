#![no_std]

use gstd::{exec, msg};

static mut SIGN: bool = true;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    // 0x1050494e470101ea2602a9883dc2c6b653c1a616cefec77783d1cbb9a3b59e765f8ab2d640813788318a1659638410a731b8b0f172c366327d1125c564a681de82d2282695a588
    let mut signed_message = [
        16, 80, 73, 78, 71, 1, 1, 234, 38, 2, 169, 136, 61, 194, 198, 182, 83, 193, 166, 22, 206,
        254, 199, 119, 131, 209, 203, 185, 163, 181, 158, 118, 95, 138, 178, 214, 64, 129, 55, 136,
        49, 138, 22, 89, 99, 132, 16, 167, 49, 184, 176, 241, 114, 195, 102, 50, 125, 17, 37, 197,
        100, 166, 129, 222, 130, 210, 40, 38, 149, 165, 135,
    ];
    if SIGN {
        SIGN = false;
        *signed_message.last_mut().unwrap() = 136;
    }
    msg::reply_bytes(signed_message, exec::gas_available() - 100_000_000, 0);
}

#[no_mangle]
pub unsafe extern "C" fn handle_reply() {}

#[no_mangle]
pub unsafe extern "C" fn init() {}
