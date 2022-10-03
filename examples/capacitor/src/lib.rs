#![no_std]

use gstd::{debug, msg, prelude::*};

// Begin of demo
static mut CHARGE: u32 = 0;

static mut LIMIT: u32 = 0;

static mut DISCHARGE_HISTORY: Vec<u32> = Vec::new();

#[no_mangle]
unsafe extern "C" fn handle() {
    let new_msg =
        String::from_utf8(msg::load_bytes().unwrap()).expect("Invalid message: should be utf-8");

    let to_add = u32::from_str(new_msg.as_ref()).expect("Invalid number");

    CHARGE += to_add;

    debug!("Charge capacitor with {}, new charge {}", to_add, CHARGE);

    if CHARGE >= LIMIT {
        debug!("Discharge #{} due to limit {}", CHARGE, LIMIT);

        msg::send_bytes(msg::source(), format!("Discharged: {}", CHARGE), 0).unwrap();
        DISCHARGE_HISTORY.push(CHARGE);
        CHARGE = 0;
    }
}
// End of demo

#[no_mangle]
unsafe extern "C" fn init() {
    let initstr =
        String::from_utf8(msg::load_bytes().unwrap()).expect("Invalid message: should be utf-8");
    let limit = u32::from_str(initstr.as_ref()).expect("Invalid number");

    LIMIT = limit;

    debug!("Init capacitor with limit capacity {}, {}", LIMIT, initstr);
}
