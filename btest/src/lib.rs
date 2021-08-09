#![cfg_attr(not(feature = "std"), feature(alloc_error_handler))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
#[cfg(test)]
mod native {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(not(feature = "std"))]
mod wasm {
    use gstd::{ext, msg, prelude::*};

    #[no_mangle]
    pub unsafe extern "C" fn handle() {}

    #[no_mangle]
    pub unsafe extern "C" fn handle_reply() {}

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        msg::reply(b"CREATED", 0, 0);
    }

    #[panic_handler]
    fn panic(_info: &panic::PanicInfo) -> ! {
        unsafe {
            core::arch::wasm32::unreachable();
        }
    }

    #[alloc_error_handler]
    pub fn oom(_: core::alloc::Layout) -> ! {
        unsafe {
            ext::debug("Runtime memory exhausted. Aborting");
            core::arch::wasm32::unreachable();
        }
    }
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {

    use super::native;

    use gear_core::storage::{
        InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList, Storage,
    };
    use gear_core_runner::{Config, ExtMessage, ProgramInitialization, Runner};

    #[test]
    fn binary_available() {
        assert!(native::WASM_BINARY.is_some());
        assert!(native::WASM_BINARY_BLOATY.is_some());
    }

    fn new_test_runner() -> Runner<InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList> {
        Runner::new(&Config::default(), Default::default())
    }

    fn wasm_code() -> &'static [u8] {
        native::WASM_BINARY.expect("wasm binary exists")
    }

    #[test]
    fn program_can_be_initialized() {
        let mut runner = new_test_runner();

        runner
            .init_program(ProgramInitialization {
                new_program_id: 1.into(),
                source_id: 0.into(),
                code: wasm_code().to_vec(),
                message: ExtMessage {
                    id: 1000001.into(),
                    payload: "init".as_bytes().to_vec(),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            })
            .expect("failed to init program");

        let Storage { message_queue, .. } = runner.complete();

        assert_eq!(
            message_queue.log().last().map(|m| m.payload().to_vec()),
            Some(b"CREATED".to_vec())
        );
    }
}
