use crate::{REPLY_REPLY, SEND_REPLY, SENT_VALUE};
use gstd::msg;

#[no_mangle]
extern "C" fn init() {
    let maybe_to = msg::load_bytes();
    let to = if maybe_to.len() == 32 {
        let mut to = [0; 32];
        to.copy_from_slice(&maybe_to);
        to.into()
    } else {
        msg::source()
    };
    msg::send_bytes(to, [], SENT_VALUE).expect("Failed to send message");
}

#[no_mangle]
extern "C" fn handle() {
    msg::reply(SEND_REPLY, 0).unwrap();
}

#[no_mangle]
extern "C" fn handle_reply() {
    msg::reply(REPLY_REPLY, 0).unwrap();
}
