#![no_std]

include!("rebuild_test.rs");

use gstd::{debug, msg};

#[no_mangle]
extern "C" fn handle() {
    debug!("handle()");
    msg::reply_bytes("Hello world!", 0).unwrap();
}

#[no_mangle]
extern "C" fn init() {
    debug!("init()");
}

#[cfg(test)]
mod gtest_tests {
    extern crate std;

    use gtest::{Log, Program, System};

    #[test]
    fn init_self() {
        let system = System::new();
        system.init_logger();

        let this_program = Program::current(&system);

        let res = this_program.send_bytes(123, "INIT");
        assert!(res.contains(
            &Log::builder()
                .source(1)
                .dest(123)
                .payload_bytes([])
        ));

        let res = this_program.send_bytes(123, "Hi");
        assert!(res.contains(
            &Log::builder()
                .source(1)
                .dest(123)
                .payload_bytes("Hello world!")
        ));
    }
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
        assert!(fs::read("target/wasm32-unknown-unknown/debug/test_program.meta.wasm").is_err());
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn release_wasm() {
        assert_eq!(
            fs::read("target/wasm32-unknown-unknown/release/test_program.wasm").unwrap(),
            code::WASM_BINARY,
        );
        assert_eq!(
            fs::read("target/wasm32-unknown-unknown/release/test_program.opt.wasm").unwrap(),
            code::WASM_BINARY_OPT,
        );
        assert!(fs::read("target/wasm32-unknown-unknown/release/test_program.meta.wasm").is_err());
    }
}
