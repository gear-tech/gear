#![no_std]

use gstd::{debug, msg};

#[no_mangle]
pub unsafe extern "C" fn handle() {
    debug!("handle()");
    msg::reply(b"Hello world!", 0, 0);
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    debug!("init()");
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::fs;

    mod code {
        include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
    }

    #[test]
    #[cfg(debug_assertions)]
    fn debug_wasm() {
        assert_eq!(
            fs::read("target/wasm32-unknown-unknown/debug/test_program.wasm").unwrap(),
            code::WASM_BINARY,
        );
        assert_eq!(
            fs::read("target/wasm32-unknown-unknown/debug/test_program.opt.wasm").unwrap(),
            code::WASM_BINARY_OPT,
        );
        assert_eq!(
            fs::read("target/wasm32-unknown-unknown/debug/test_program.meta.wasm").unwrap(),
            code::WASM_BINARY_META,
        );
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn release_wasm_exists() {
        assert_eq!(
            fs::read("target/wasm32-unknown-unknown/release/test_program.wasm").unwrap(),
            code::WASM_BINARY,
        );
        assert_eq!(
            fs::read("target/wasm32-unknown-unknown/release/test_program.opt.wasm").unwrap(),
            code::WASM_BINARY_OPT,
        );
        assert_eq!(
            fs::read("target/wasm32-unknown-unknown/release/test_program.meta.wasm").unwrap(),
            code::WASM_BINARY_META,
        );
    }
}
