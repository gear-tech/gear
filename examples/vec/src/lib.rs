#![no_std]

use gstd::{debug, msg, prelude::*};

static mut MESSAGE_LOG: Vec<String> = vec![];

#[no_mangle]
extern "C" fn handle() {
    let new_msg: i32 = msg::load().expect("Should be i32");
    unsafe { MESSAGE_LOG.push(format!("(vec) New msg: {new_msg:?}")) };
    let v = vec![1u8; new_msg as usize];
    debug!("v.len() = {:?}", v.len());
    debug!(
        "v[{}]: {:p} -> {:#04x}",
        v.len() - 1,
        &v[new_msg as usize - 1],
        v[new_msg as usize - 1]
    );
    msg::send(msg::source(), v.len() as i32, 0).unwrap();
    debug!("{:?} total message(s) stored: ", unsafe {
        MESSAGE_LOG.len()
    });

    // The test idea is to allocate two wasm pages and check this allocation,
    // so we must skip `v` destruction.
    core::mem::forget(v);

    for log in unsafe { MESSAGE_LOG.iter() } {
        debug!(log);
    }
}
