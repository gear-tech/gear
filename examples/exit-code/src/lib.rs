#![no_std]

use gstd::{msg, prelude::ToString, ActorId};

static mut HOST: ActorId = ActorId::new([0u8; 32]);

// It was containing the only handle_reply previously
// what is just for testing and isn't valid at all
// cause we can't receive reply on message if we never send it.
// For this demo in real conditions handle_reply is unreachable.
#[no_mangle]
pub unsafe extern "C" fn handle() {}

#[no_mangle]
pub unsafe extern "C" fn handle_reply() {
    msg::send_bytes(HOST, msg::exit_code().to_string(), 0).unwrap();
}
