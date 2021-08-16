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

    #[derive(Clone)]
    enum TransitionState {
        Ready,
        NotReady,
        Commited,
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

        msg::reply(&reply.encode()[..], msg::gas_available(), 0)
    }

    fn state() -> &'static mut NodeState {
        unsafe { STATE.as_mut().unwrap() }
    }

    fn process(request: Request) -> Reply {
        if let Some(mut transition) = state().transition.take() {
            if transition.message_id == msg::id() {
                // one of the answers has set failed state
                if let TransitionState::Failed = transition.state {
                    return Reply::Failure;
                }

                // this means that we received replies from all subnodes
                if transition.query_index == transition.query_list.len() {
                    match transition.state.clone() {
                        TransitionState::NotReady => {
                            transition.state = TransitionState::Ready;

                            ext::debug("Returning final ready signal");

                            // this is ready to further process with committing
                            state().transition = Some(transition);
                            return Reply::Success;
                        }
                        TransitionState::Ready => {
                            // this means we successfully commited and we can
                            // drop the transition returning success
                            ext::debug("Returning final commit signal");

                            return Reply::Success;
                        }
                        _ => {
                            // this is some invalid state already
                            return Reply::Failure;
                        }
                    }
                }

                // this means we need to send another sub-node query
                let next_sub_node = transition
                    .query_list
                    .get(transition.query_index)
                    .expect("Checked above that it has that number of elements; qed");

                transition.last_sent_message_id = msg::send(
                    *next_sub_node,
                    &request.encode()[..],
                    msg::gas_available() - 2_500_000,
                );

                state().transition = Some(transition);

                msg::wait();
            } else {
                // this is just a new message that should be processed normally, without continuation.
                state().transition = Some(transition);
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
                        state: TransitionState::NotReady,
                        message_id: msg::id(),
                        last_sent_message_id: MessageId::default(),
                    };

                    ext::debug("Transition started");

                    if state().sub_nodes.len() > 0 {
                        ext::debug("Transition started is complex");

                        transition.query_list = state().sub_nodes.iter().cloned().collect();
                        let first_sub_node = *transition
                            .query_list
                            .get(0)
                            .expect("Checked above that sub_nodes is not empty; qed");
                        transition.last_sent_message_id = msg::send(
                            first_sub_node,
                            &request.encode()[..],
                            msg::gas_available() - 2_500_000,
                        );
                        state().transition = Some(transition);
                        msg::wait();
                    } else {
                        transition.state = TransitionState::Ready;
                        state().transition = Some(transition);
                        Reply::Success
                    }
                }
            }
            Request::Commit => {
                if state().sub_nodes.len() == 0 {
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
                } else {
                    if let Some(mut transition) = state().transition.take() {
                        if let TransitionState::Ready = transition.state {
                            let first_sub_node = *transition
                                .query_list
                                .get(0)
                                .expect("Checked above that sub_nodes is not empty; qed");

                            transition.query_index = 0;

                            transition.message_id = msg::id();

                            transition.last_sent_message_id = msg::send(
                                first_sub_node,
                                &request.encode()[..],
                                msg::gas_available() - 2_500_000,
                            );

                            state().transition = Some(transition);

                            msg::wait();
                        } else {
                            ext::debug("Returning failure because current state is not READY");
                            Reply::Failure
                        }
                    } else {
                        ext::debug("Returning failure because there is no transition in process");
                        Reply::Failure
                    }
                }
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
                    if let Reply::Success = reply {
                    } else {
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
        } else {
            ext::debug("Got some reply that can not be processed");
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
    use gear_core_runner::{Config, Runner};

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
        // env_logger::Builder::from_env(env_logger::Env::default()).init();

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

        let (_runner, reply) = common::do_reqrep(
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
    }

    #[test]
    fn multiple_nodes_can_prepare_to_change_status() {
        env_logger::Builder::from_env(env_logger::Env::default()).init();

        let runner = new_test_runner();

        let program_id_1: ProgramId = 1.into();
        let program_id_2: ProgramId = 2.into();
        let program_id_3: ProgramId = 3.into();

        let mut nonce = 1;

        let runner = common::do_init(
            runner,
            InitProgramData {
                new_program_id: program_id_1,
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

        let runner = common::do_init(
            runner,
            InitProgramData {
                new_program_id: program_id_2,
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

        let runner = common::do_init(
            runner,
            InitProgramData {
                new_program_id: program_id_3,
                source_id: 0.into(),
                code: wasm_code().to_vec(),
                message: MessageData {
                    id: nonce.into(),
                    payload: Initialization { status: 9 },
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
                    payload: Request::Add(2),
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
                    payload: Request::Add(3),
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
                    payload: Request::Begin(Operation { to_status: 7 }),
                    gas_limit: u64::MAX,
                    value: 0,
                },
            },
        );
        assert_eq!(reply, Some(Reply::Success));
        nonce += 1;

        let (_runner, reply) = common::do_reqrep(
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
    }
}
