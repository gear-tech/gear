#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use gstd::{msg, prelude::*};

static mut MY_COLLECTION: BTreeMap<usize, String> = BTreeMap::new();

static mut COUNTER: usize = 0;

#[no_mangle]
unsafe extern "C" fn handle() {
    let new_msg = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    if new_msg == "log" {
        let collapsed = mem::take(&mut MY_COLLECTION)
            .into_iter()
            .map(|(number, msg)| format!("{}: {};", number, msg))
            .fold(String::new(), |mut acc, n| {
                acc.push_str(&n);
                acc
            });

        msg::send_bytes(msg::source(), collapsed, 0).unwrap();

        COUNTER = 0;
    } else {
        COUNTER += 1;
        MY_COLLECTION.insert(COUNTER, new_msg);
    }
}
