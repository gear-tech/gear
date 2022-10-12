#![no_std]

use gstd::{msg, ActorId};

static mut HOST: ActorId = ActorId::new([0u8; 32]);

// It was containing the only handle_reply previously
// what is just for testing and isn't valid at all
// cause we can't receive reply on message if we never send it.
// For this demo in real conditions handle_reply is unreachable.
#[no_mangle]
unsafe extern "C" fn handle() {}

#[no_mangle]
unsafe extern "C" fn handle_reply() {
    msg::send_bytes(HOST, msg::exit_code().unwrap().to_le_bytes(), 0).unwrap();
}
