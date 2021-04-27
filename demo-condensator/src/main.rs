extern crate alloc;

use gstd::msg;
use alloc::str::FromStr;

// Begin of demo
static mut CHARGE: u32 = 0;

static mut LIMIT: u32 = 0;

static mut DISCHARGE_HISTORY: Vec<u32> = Vec::new();

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");

    let to_add = u32::from_str(&new_msg).expect("Invalid number");

    CHARGE += to_add;

    if CHARGE >= LIMIT {
        DISCHARGE_HISTORY.push(CHARGE);
        msg::send(0.into(), format!("Discharged: {}", CHARGE).as_bytes(), 1000000000);
        CHARGE = 0;
    }
}
// End of demo

#[no_mangle]
pub unsafe extern "C" fn init() {
    let limit =
        u32::from_str(
            String::from_utf8(msg::load()).expect("Invalid message: should be utf-8").as_ref()
        ).expect("Invalid number");

    LIMIT = limit;
}

fn main() {
}
