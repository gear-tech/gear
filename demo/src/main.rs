use gstd::{ext, msg};

static mut MESSAGE_LOG: Vec<String> = vec![];

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");
    MESSAGE_LOG.push(new_msg);
    ext::debug(&format!("{:?} total message(s) stored.", MESSAGE_LOG.len()));
}

fn main() {
}
