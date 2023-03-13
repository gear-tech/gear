use gstd::{msg, MessageId, exec};

static mut MID: Option<MessageId> = None;
static mut DONE: bool = false;

#[no_mangle]
extern "C" fn init() {
    let delay: u32 = msg::load().unwrap();

    msg::reply_bytes_delayed("Delayed hello!", 0, delay).unwrap();
}

#[no_mangle]
extern "C" fn handle() {
    if let Some(message_id) = unsafe { MID.take() } {
        let delay: u32 = msg::load().unwrap();

        unsafe { DONE = true; }

        exec::wake_delayed(message_id, delay);
    } else if unsafe { !DONE } {
        unsafe { MID = Some(msg::id()); }

        exec::wait();
    }
}
