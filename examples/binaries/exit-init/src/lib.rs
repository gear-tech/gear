#![cfg_attr(not(feature = "std"), feature(alloc_error_handler))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
mod wasm {
    use gcore::msg::load;
    use gstd::{exec, msg};

    #[no_mangle]
    unsafe extern "C" fn handle() {}

    #[no_mangle]
    unsafe extern "C" fn init() {
        let shall_reply_before_exit: bool = {
            let mut flag = [0u8];
            load(&mut flag);
            u8::from_le_bytes(flag) == 1
        };
        if shall_reply_before_exit {
            msg::reply(b"If you read this, I'm dead", 0).unwrap();
            exec::exit(msg::source());
        } else {
            exec::exit(msg::source());
            // should not be executed
            msg::reply(b"reply", 0).unwrap();
        }
    }
}
