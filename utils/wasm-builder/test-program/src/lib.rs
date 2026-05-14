#![no_std]

include!("rebuild_test.rs");

use gstd::{debug, msg};

#[cfg(feature = "a")]
#[unsafe(no_mangle)]
extern "C" fn handle_reply() {}

#[cfg(feature = "b")]
#[unsafe(no_mangle)]
extern "C" fn handle_signal() {}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    debug!("handle()");
    msg::reply_bytes("Hello world!", 0).unwrap();
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    debug!("init()");
}

#[cfg(test)]
mod gtest_tests {
    extern crate std;

    use gtest::{Log, Program, System, constants::UNITS};

    #[test]
    fn init_self() {
        let system = System::new();
        system.init_logger();
        system.mint_to(123, UNITS * 100);

        let this_program = Program::current(&system);

        this_program.send_bytes(123, "INIT");
        let res = system.run_next_block();
        assert!(res.contains(&Log::builder().source(1).dest(123).payload_bytes([])));

        this_program.send_bytes(123, "Hi");
        let res = system.run_next_block();
        assert!(
            res.contains(
                &Log::builder()
                    .source(1)
                    .dest(123)
                    .payload_bytes("Hello world!")
            )
        );
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
            fs::read("target/wasm32-gear/debug/test_program.wasm").unwrap(),
            code::WASM_BINARY,
        );
        assert_eq!(
            fs::read("target/wasm32-gear/debug/test_program.opt.wasm").unwrap(),
            code::WASM_BINARY_OPT,
        );
        assert!(fs::read("target/wasm32-gear/debug/test_program.meta.wasm").is_err());
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn release_wasm() {
        assert_eq!(
            fs::read("target/wasm32-gear/release/test_program.wasm").unwrap(),
            code::WASM_BINARY,
        );
        assert_eq!(
            fs::read("target/wasm32-gear/release/test_program.opt.wasm").unwrap(),
            code::WASM_BINARY_OPT,
        );
        assert!(fs::read("target/wasm32-gear/release/test_program.meta.wasm").is_err());
    }
}
