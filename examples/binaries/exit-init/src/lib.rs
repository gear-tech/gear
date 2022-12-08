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
    use gcore::msg;
    use gstd::exec;

    #[no_mangle]
    extern "C" fn handle() {}

    #[no_mangle]
    extern "C" fn init() {
        let shall_reply_before_exit: bool = {
            let mut flag = [0u8];
            msg::read(&mut flag);
            u8::from_le_bytes(flag) == 1
        };
        if shall_reply_before_exit {
            msg::reply(b"If you read this, I'm dead", 0).unwrap();
            exec::exit(gstd::msg::source());
        } else {
            #[allow(unreachable_code)]
            exec::exit(gstd::msg::source());
            // should not be executed
            msg::reply(b"reply", 0).unwrap();
        }
    }
}
