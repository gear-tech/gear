use gstd_async::msg;

mod sys {
    use super::*;

    #[no_mangle]
    unsafe extern "C" fn gr_send(
        _program: *const u8,
        _data_ptr: *const u8,
        _data_len: u32,
        _gas_limit: u64,
        _value_ptr: *const u8,
    ) {
    }

    #[no_mangle]
    unsafe extern "C" fn gr_size() -> u32 {
        0
    }

    #[no_mangle]
    unsafe extern "C" fn gr_read(_at: u32, _len: u32, _dest: *mut u8) {
    }

    #[no_mangle]
    unsafe extern "C" fn gr_source(_program: *mut u8) {
    }

    #[no_mangle]
    unsafe extern "C" fn gr_value(_val: *mut u8) {
    }

    #[no_mangle]
    unsafe extern "C" fn gr_charge(_gas: u64) {
    }
}

async fn handle_async() {
    msg::send_and_wait(1.into(), b"hi", u64::MAX, 0).await;
}


#[test]
fn async_send() {
    println!("Yes!");
    gstd_async::block_on(handle_async());
}
