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
    use gstd::{exec, msg};

    #[no_mangle]
    pub unsafe extern "C" fn handle() {}

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        exec::exit(msg::source());
        // should not be executed
        msg::reply(b"reply", exec::gas_available(), 0);
    }
}
