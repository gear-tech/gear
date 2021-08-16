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
    to_status: u32,
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub struct Initialization {
    status: u32,
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Request {
    IsReady,
    Begin(Operation),
    Commit,
    Add(u64),
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Reply {
    Yes,
    No,
    NotNeeded,
    Success,
    Failure,
}

#[cfg(not(feature = "std"))]
mod wasm {
    extern crate alloc;

    use alloc::collections::{BTreeMap, BTreeSet};
    use codec::{Decode, Encode};
    use gstd::{ext, msg, prelude::*, MessageId, ProgramId};

    use super::{Initialization, Operation, Reply, Request};

    enum TransitionState {
        Ready,
        NotReady,
        Failed,
    }

    struct Transition {
        to_status: u32,
        origin: ProgramId,
        query_list: Vec<ProgramId>,
        message_id: MessageId,
        last_sent_message_id: MessageId,
        query_index: usize,
        state: TransitionState,
    }

    struct NodeState {
        status: u32,
        sub_nodes: BTreeSet<ProgramId>,
        transition: Option<Transition>,
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

    fn process(request: Request) -> Reply {
        if let Some(ref mut transition) = state().transition {
            if transition.message_id == msg::id() {
                // one of the answers has set failed state
                if let TransitionState::Failed = transition.state {
                    return Reply::Failure;
                }

                // this means that we sent messages to all subnodes
                if transition.query_index == transition.query_list.len() {
                    transition.state = TransitionState::Ready;
                    return Reply::Success;
                }

                // this means we need to send another sub-node query
                let next_sub_node = transition
                    .query_list
                    .get(transition.query_index)
                    .expect("Checked above that it has that number of elements; qed");

                transition.last_sent_message_id =
                    msg::send(*next_sub_node, &request.encode()[..], msg::gas_available() - 25000);

                msg::wait();
            }
        }

        match request {
            Request::IsReady => {
                if state().transition.is_none() {
                    Reply::Yes
                } else {
                    Reply::No
                }
            }
            Request::Begin(Operation { to_status }) => {
                if state().transition.is_some() {
                    Reply::Failure
                } else {
                    let mut transition = Transition {
                        to_status,
                        origin: msg::source(),
                        query_index: 0,
                        query_list: vec![],
                        state: TransitionState::Ready,
                        message_id: msg::id(),
                        last_sent_message_id: MessageId::default(),
                    };

                    if state().sub_nodes.len() > 0 {
                        transition.query_list = state().sub_nodes.iter().cloned().collect();
                        let first_sub_node = *transition
                            .query_list
                            .get(0)
                            .expect("Checked above that sub_nodes is not empty; qed");
                        transition.last_sent_message_id =
                            msg::send(first_sub_node, &request.encode()[..], msg::gas_available() - 25000);
                        state().transition = Some(transition);
                        msg::wait();
                    } else {
                        state().transition = Some(transition);
                        Reply::Success
                    }
                }
            }
            Request::Commit => {
                let (transition, reply) = match state().transition.take() {
                    Some(transition) => {
                        if transition.origin != msg::source() {
                            (Some(transition), Reply::Failure)
                        } else {
                            (None, Reply::Success)
                        }
                    }
                    None => (None, Reply::Failure),
                };

                state().transition = transition;

                reply
            }
            Request::Add(sub_node) => {
                state().sub_nodes.insert((sub_node as u64).into());
                Reply::Success
            }
        }
    }

    #[no_mangle]
    pub unsafe extern "C" fn handle_reply() {
        if let Some(ref mut transition) = state().transition {
            if msg::reply_to() != transition.last_sent_message_id {
                return;
            }

            match Reply::decode(&mut &msg::load()[..]) {
                Ok(reply) => {
                    transition.query_index += 1;
                    if let Reply::Success = reply {} else {
                        transition.state = TransitionState::Failed;
                    }
                    msg::wake(transition.message_id);
                }
                Err(e) => {
                    transition.state = TransitionState::Failed;
                    ext::debug(&format!("Error processing reply: {:?}", e));
                    msg::wake(transition.message_id);
                }
            }
        }
    }

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        let init = Initialization::decode(&mut &msg::load()[..]).expect("Failed to decode init");
        STATE = Some(NodeState {
            status: init.status,
            sub_nodes: BTreeSet::default(),
            transition: None,
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
        env_logger::Builder::from_env(env_logger::Env::default()).init();

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
                    payload: Request::IsReady,
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        assert_eq!(reply, Some(Reply::Yes));
        nonce += 1;

        let (runner, reply) = common::do_reqrep(
            runner,
            MessageDispatchData {
                source_id: 0.into(),
                destination_id: program_id_1,
                data: MessageData {
                    id: nonce.into(),
                    payload: Request::Begin(Operation { to_status: 7 }),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        assert_eq!(reply, Some(Reply::Success));
        nonce += 1;

        let (runner, reply) = common::do_reqrep(
            runner,
            MessageDispatchData {
                source_id: 0.into(),
                destination_id: program_id_1,
                data: MessageData {
                    id: nonce.into(),
                    payload: Request::Commit,
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        assert_eq!(reply, Some(Reply::Success));
        nonce += 1;
    }
}
