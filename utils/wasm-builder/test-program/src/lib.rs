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
    use std::path::Path;

    #[test]
    #[cfg(not(debug_assertions))]
    fn release_wasm_exists() {
        assert!(Path::new("target/wasm32-unknown-unknown/release/test_program.wasm").exists());
        assert!(Path::new("target/wasm32-unknown-unknown/release/test_program.opt.wasm").exists());
        assert!(Path::new("target/wasm32-unknown-unknown/release/test_program.meta.wasm").exists());
    }

    #[test]
    #[cfg(debug_assertions)]
    fn debug_wasm_exists() {
        assert!(Path::new("target/wasm32-unknown-unknown/debug/test_program.wasm").exists());
        assert!(Path::new("target/wasm32-unknown-unknown/debug/test_program.opt.wasm").exists());
        assert!(Path::new("target/wasm32-unknown-unknown/debug/test_program.meta.wasm").exists());
    }
}
