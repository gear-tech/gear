#![no_std]

use gstd::{debug, exec, msg};

#[no_mangle]
unsafe extern "C" fn handle() {
    match &msg::load_bytes()[..] {
        b"insert" => debug!(
            "inserted: {}, total: {}",
            msg::value(),
            exec::value_available()
        ),
        b"smash" => {
            debug!("smashing, total: {}", exec::value_available());
            msg::send_bytes(msg::source(), b"send", exec::value_available()).unwrap();
        }
        _ => (),
    }
}
