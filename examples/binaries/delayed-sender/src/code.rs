use gstd::msg;

#[no_mangle]
extern "C" fn init() {
    let delay: u32 = msg::load().unwrap();

    msg::reply_bytes_delayed("Delayed hello!", 0, delay).unwrap();
}
