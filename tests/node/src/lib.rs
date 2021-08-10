#![cfg_attr(not(feature = "std"), feature(alloc_error_handler))]
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
#[cfg(not(feature = "std"))]
use gstd::{prelude::*, *};

#[cfg(feature = "std")]
#[cfg(test)]
mod native {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub struct Operation {
    from_status: u32,
    to_status: u32,
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub struct Initialization {
    status: u32,
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Request {
    IsReady(Operation),
    Process(Operation),
    Join(u64),
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Reply {
    Yes,
    No,
    Success,
    Failure,
}

#[cfg(not(feature = "std"))]
mod wasm {
    extern crate alloc;

    use alloc::collections::BTreeSet;
    use codec::{Decode, Encode};
    use gstd::{ext, msg, prelude::*};

    use super::{Initialization, Operation, Reply, Request};

    struct NodeState {
        status: u32,
        sub_nodes: BTreeSet<u64>,
    }

    static mut STATE: Option<NodeState> = None;

    #[no_mangle]
    pub unsafe extern "C" fn handle() {
        let reply = match Request::decode(&mut &msg::load()[..]) {
            Ok(request) => process(request),
            Err(e) => {
                ext::debug(&format!("Error processing request: {:?}", e));
                Reply::Failure
            }
        };

        msg::reply(&reply.encode()[..], 1000000, 0)
    }

    fn state() -> &'static mut NodeState {
        unsafe { STATE.as_mut().unwrap() }
    }

    fn process(request: super::Request) -> Reply {
        unimplemented!()
    }

    #[no_mangle]
    pub unsafe extern "C" fn handle_reply() {}

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        let init = Initialization::decode(&mut &msg::load()[..]).expect("Failed to decode init");
        STATE = Some(NodeState {
            status: init.status,
            sub_nodes: BTreeSet::default(),
        });
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
    use super::{native, Initialization, Operation, Reply, Request};
    use common::*;
    use gear_core::program::ProgramId;
    use gear_core::storage::{
        InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList, Storage,
    };
    use gear_core_runner::{Config, ExtMessage, ProgramInitialization, Runner};

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
        let runner = new_test_runner();
        let runner = common::do_init(
            runner,
            InitProgramData {
                new_program_id: 1.into(),
                source_id: 0.into(),
                code: wasm_code().to_vec(),
                message: MessageData {
                    id: 1.into(),
                    payload: Initialization { status: 5 },
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );

        let Storage { message_queue, .. } = runner.complete();

        assert_eq!(
            message_queue.log().last().map(|m| m.payload().to_vec()),
            Some(b"CREATED".to_vec())
        );
    }

    #[test]
    fn one_node_can_change_status() {
        let runner = new_test_runner();

        let program_id_1: ProgramId = 1.into();

        let mut nonce = 1;

        let runner = common::do_init(
            runner,
            InitProgramData {
                new_program_id: 1.into(),
                source_id: 0.into(),
                code: wasm_code().to_vec(),
                message: MessageData {
                    id: nonce.into(),
                    payload: Initialization { status: 5 },
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );

        nonce += 1;

        let (runner, reply) = common::do_reqrep(
            runner,
            MessageDispatchData {
                source_id: 0.into(),
                destination_id: program_id_1,
                data: MessageData {
                    id: nonce.into(),
                    payload: Request::IsReady(Operation {
                        from_status: 5,
                        to_status: 7,
                    }),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );

        assert_eq!(reply, Some(Reply::Yes));
    }
}
