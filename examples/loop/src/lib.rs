#![no_std]

#[no_mangle]
extern "C" fn handle() {
    gstd::debug!("Start loop");

    #[allow(clippy::empty_loop)]
    loop {}
}
