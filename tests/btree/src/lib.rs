#![cfg_attr(not(feature = "std"), feature(alloc_error_handler))]
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
#[cfg(not(feature = "std"))]
use gstd::prelude::*;

#[cfg(feature = "std")]
#[cfg(test)]
mod native {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Request {
    Insert(u32, u32),
    Remove(u32),
    List,
    Clear,
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Reply {
    Error,
    None,
    Value(Option<u32>),
    List(Vec<(u32, u32)>),
}

#[cfg(not(feature = "std"))]
mod wasm {
    extern crate alloc;

    use alloc::collections::BTreeMap;
    use codec::{Decode, Encode};
    use gstd::{ext, msg, prelude::*};

    use super::{Reply, Request};

    static mut STATE: Option<BTreeMap<u32, u32>> = None;

    #[no_mangle]
    pub unsafe extern "C" fn handle() {
        let reply = match Request::decode(&mut &msg::load()[..]) {
            Ok(request) => process(request),
            Err(e) => {
                ext::debug(&format!("Error processing request: {:?}", e));
                Reply::Error
            }
        };

        msg::reply(&reply.encode()[..], 1000000, 0)
    }

    fn state() -> &'static mut BTreeMap<u32, u32> {
        unsafe { STATE.as_mut().unwrap() }
    }

    fn process(request: super::Request) -> Reply {
        use super::Request::*;
        match request {
            Insert(key, value) => Reply::Value(state().insert(key, value)),
            Remove(key) => Reply::Value(state().remove(&key)),
            List => Reply::List(state().iter().map(|(k, v)| (*k, *v)).collect()),
            Clear => {
                state().clear();
                Reply::None
            }
        }
    }

    #[no_mangle]
    pub unsafe extern "C" fn handle_reply() {}

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        STATE = Some(BTreeMap::new());
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
    use gear_core_runner::{Config, ExtMessage, InitializeProgramInfo, Runner};

    #[test]
    fn binary_available() {
        assert!(native::WASM_BINARY.is_some());
        assert!(native::WASM_BINARY_BLOATY.is_some());
    }

    pub type LocalRunner = Runner<InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList>;

    fn new_test_runner() -> LocalRunner {
        Runner::new(&Config::default(), Default::default())
    }

    fn wasm_code() -> &'static [u8] {
        native::WASM_BINARY.expect("wasm binary exists")
    }

    #[test]
    fn program_can_be_initialized() {
        let mut runner = new_test_runner();

        runner
            .init_program(InitializeProgramInfo {
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

    #[test]
    fn simple() {
        use super::{
            Reply,
            Request::{self, *},
        };
        assert_eq!(
            common::do_requests_in_order::<Request, Reply>(
                new_test_runner(),
                wasm_code().to_vec(),
                vec![
                    Insert(0, 1),
                    Insert(0, 2),
                    Insert(1, 3),
                    Insert(2, 5),
                    Remove(1),
                    List,
                    Clear,
                    List,
                ]
            ),
            vec![
                Reply::Value(None),
                Reply::Value(Some(1)),
                Reply::Value(None),
                Reply::Value(None),
                Reply::Value(Some(3)),
                Reply::List(vec![(0, 2), (2, 5)]),
                Reply::None,
                Reply::List(vec![]),
            ],
        )
    }
}
