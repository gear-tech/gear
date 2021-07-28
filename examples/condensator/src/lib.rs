#![no_std]
#![feature(default_alloc_error_handler)]

use gstd::{ext, msg, prelude::*};

// Begin of demo
static mut CHARGE: u32 = 0;

static mut LIMIT: u32 = 0;

static mut DISCHARGE_HISTORY: Vec<u32> = Vec::new();

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");

    let to_add = u32::from_str(new_msg.as_ref()).expect("Invalid number");

    CHARGE += to_add;

    ext::debug(&format!(
        "Charge condensator with {}, new charge {}",
        to_add, CHARGE,
    ));

    if CHARGE >= LIMIT {
        ext::debug(&format!("Discharge #{} due to limit {}", CHARGE, LIMIT,));

        msg::send(
            0.into(),
            format!("Discharged: {}", CHARGE).as_bytes(),
            1000000000
            
        );
        DISCHARGE_HISTORY.push(CHARGE);
        CHARGE = 0;
    }
}
// End of demo

#[no_mangle]
pub unsafe extern "C" fn init() {
    let initstr = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");
    let limit = u32::from_str(initstr.as_ref()).expect("Invalid number");

    LIMIT = limit;

    ext::debug(&format!(
        "Init condensator with limit capacity {}, {}",
        LIMIT, initstr,
    ));
}

#[panic_handler]
fn panic(_info: &panic::PanicInfo) -> ! {
    loop {}
}
