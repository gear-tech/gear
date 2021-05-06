#![feature(num_as_ne_bytes)]

use gstd::{ext, msg};
use std::convert::TryInto;

static mut MESSAGE_LOG: Vec<String> = vec![];

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg = i32::from_le_bytes(msg::load().try_into().expect("Should be i32"));
    MESSAGE_LOG.push(format!("New msg: {:?}", new_msg));
    let v = vec![1; new_msg as usize];
    ext::debug(&format!("v.len() = {:?}", v.len()));
    ext::debug(&format!(
        "v[{}]: {:p} -> {:#04x}",
        v.len() - 1,
        &v[new_msg as usize - 1],
        v[new_msg as usize - 1]
    ));
    msg::send(msg::source(), (v.len() as i32).as_ne_bytes(), u64::MAX, 0);
    ext::debug(&format!(
        "{:?} total message(s) stored: ",
        MESSAGE_LOG.len()
    ));

    for log in MESSAGE_LOG.iter() {
        ext::debug(log);
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {}

fn main() {}
