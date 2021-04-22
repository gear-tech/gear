extern crate alloc;

use gstd::msg;
use alloc::str::FromStr;

static mut CHARGE: u32 = 0;

const LIMIT: u32 = 1000;

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");

    let to_add = u32::from_str(&new_msg).expect("Invalid number");

    CHARGE += to_add;

    if CHARGE >= LIMIT {
        msg::send(0.into(), format!("Discharged: {}", CHARGE).as_bytes(), 1000000000);
        CHARGE = 0;
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {
}

fn main() {
}
