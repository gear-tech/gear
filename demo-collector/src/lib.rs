#![no_std]
#![feature(default_alloc_error_handler)]

#[macro_use]
extern crate alloc;

use core::fmt::Write;
use alloc::string::String;

use hashbrown::HashMap;

use gstd::{msg, ProgramId};

static mut MY_COLLECTION: HashMap<ProgramId, String> = HashMap::with_hasher(
    hashbrown::hash_map::DefaultHashBuilder::with_seeds(0, 1, 2, 3)
);

fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b).expect("Format failed")
    }
    s
}

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");
    if new_msg == "log" {
        let collapsed = MY_COLLECTION
            .drain()
            .map(|(program_id, msg)| format!("{}: {};", encode_hex(program_id.as_slice()), msg))
            .fold(String::new(), |mut acc, n| { acc.push_str(&n); acc } );

        msg::send(msg::source(), collapsed.as_bytes(), 1000000000, 0);
    } else {
        MY_COLLECTION.insert(msg::source(), new_msg);
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
