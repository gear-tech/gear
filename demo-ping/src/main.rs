use gstd::{ext, msg};

static mut MESSAGE_LOG: Vec<String> = vec![];

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");

    if &new_msg == "PING" {
        msg::send(msg::source(), b"PONG", u64::MAX);
    }

    MESSAGE_LOG.push(new_msg);

    ext::debug(&format!("{:?} total message(s) stored: ", MESSAGE_LOG.len()));

    for log in MESSAGE_LOG.iter() {
        ext::debug(log);
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {

}

fn main() {
}
