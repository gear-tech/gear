#![cfg_attr(not(feature = "std"), feature(alloc_error_handler))]
#![cfg_attr(not(feature = "std"), feature(const_btree_new))]
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};

#[cfg(feature = "std")]
#[cfg(test)]
mod native {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Request {
    EchoWait(u32),
    Wake([u8; 32]),
}

#[cfg(not(feature = "std"))]
mod wasm {
    extern crate alloc;

    use codec::{Decode, Encode};
    use gstd::{exec, msg, prelude::*, ActorId, MessageId};

    use super::Request;

    static mut ECHOES: BTreeMap<MessageId, u32> = BTreeMap::new();

    fn process_request(request: Request) {
        match request {
            Request::EchoWait(n) => {
                unsafe { ECHOES.insert(msg::id(), n) };
                exec::wait();
            }
            Request::Wake(id) => exec::wake(MessageId::new(id)),
        }
    }

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        msg::reply((), 0, 0);
    }

    #[no_mangle]
    pub unsafe extern "C" fn handle() {
        if let Some(reply) = unsafe { ECHOES.remove(&msg::id()) } {
            msg::reply(reply, 0, 0);
        } else {
            msg::load::<Request>().map(process_request);
        }
    }

    #[no_mangle]
    pub unsafe extern "C" fn handle_reply() {}
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    use super::{native, Request};
    use common::*;
    use gear_core::message::MessageId;
    use std::convert::TryInto;

    #[test]
    fn binary_available() {
        assert!(native::WASM_BINARY.is_some());
        assert!(native::WASM_BINARY_BLOATY.is_some());
    }

    fn wasm_code() -> &'static [u8] {
        native::WASM_BINARY_BLOATY.expect("wasm binary exists")
    }

    #[test]
    fn program_can_be_initialized() {
        let mut runner = RunnerContext::default();

        // Assertions are performed when decoding reply
        let _reply: () = runner.init_program_with_reply(InitProgram::from(wasm_code()));
    }

    #[test]
    fn wake_self() {
        let prog_id_1 = 1;

        let mut runner = RunnerContext::default();
        runner.init_program(InitProgram::from(wasm_code()).id(prog_id_1));

        let msg_id_1 = MessageId::from(10);
        let msg_id_2 = MessageId::from(20);

        let reply = runner.try_request::<_, ()>(
            MessageBuilder::from(Request::EchoWait(100))
                .id(msg_id_1)
                .destination(prog_id_1),
        );
        assert_eq!(reply, None);

        let reply = runner.try_request::<_, ()>(
            MessageBuilder::from(Request::EchoWait(200))
                .id(msg_id_2)
                .destination(prog_id_1),
        );
        assert_eq!(reply, None);

        let reply = runner.try_request::<_, ()>(
            MessageBuilder::from(Request::Wake(
                msg_id_1
                    .as_slice()
                    .try_into()
                    .expect("MessageId inner array size is 32"),
            ))
            .destination(prog_id_1),
        );
        assert_eq!(reply, None);

        let reply = runner.try_request::<_, ()>(
            MessageBuilder::from(Request::Wake(
                msg_id_2
                    .as_slice()
                    .try_into()
                    .expect("MessageId inner array size is 32"),
            ))
            .destination(prog_id_1),
        );
        assert_eq!(reply, None);

        let reply: u32 = runner
            .get_response_to(msg_id_1)
            .expect("No response to original message")
            .expect("Unable to parse response to original message");
        assert_eq!(reply, 100);

        let reply: u32 = runner
            .get_response_to(msg_id_2)
            .expect("No response to original message")
            .expect("Unable to parse response to original message");
        assert_eq!(reply, 200);
    }

    #[test]
    fn wake_other() {
        let prog_id_1 = 1;
        let prog_id_2 = 2;

        let mut runner = RunnerContext::default();
        runner.init_program(InitProgram::from(wasm_code()).id(prog_id_1));
        runner.init_program(InitProgram::from(wasm_code()).id(prog_id_2));

        let msg_id_1 = MessageId::from(10);
        let msg_id_2 = MessageId::from(20);

        let reply = runner.try_request::<_, ()>(
            MessageBuilder::from(Request::EchoWait(100))
                .id(msg_id_1)
                .destination(prog_id_1),
        );
        assert_eq!(reply, None);

        let reply = runner.try_request::<_, ()>(
            MessageBuilder::from(Request::EchoWait(200))
                .id(msg_id_2)
                .destination(prog_id_2),
        );
        assert_eq!(reply, None);

        let reply = runner.try_request::<_, ()>(
            MessageBuilder::from(Request::Wake(
                msg_id_1
                    .as_slice()
                    .try_into()
                    .expect("MessageId inner array size is 32"),
            ))
            .destination(prog_id_2),
        );
        assert_eq!(reply, None);

        let reply = runner.try_request::<_, ()>(
            MessageBuilder::from(Request::Wake(
                msg_id_2
                    .as_slice()
                    .try_into()
                    .expect("MessageId inner array size is 32"),
            ))
            .destination(prog_id_1),
        );
        assert_eq!(reply, None);

        let reply = runner.get_response_to::<_, u32>(msg_id_1);
        assert_eq!(reply, None);

        let reply = runner.get_response_to::<_, u32>(msg_id_2);
        assert_eq!(reply, None);

        let reply = runner.try_request::<_, ()>(
            MessageBuilder::from(Request::Wake(
                msg_id_2
                    .as_slice()
                    .try_into()
                    .expect("MessageId inner array size is 32"),
            ))
            .destination(prog_id_2),
        );
        assert_eq!(reply, None);

        let reply = runner
            .get_response_to::<_, u32>(msg_id_2)
            .expect("No response to original message")
            .expect("Unable to parse response to original message");
        assert_eq!(reply, 200);

        let reply = runner.try_request::<_, ()>(
            MessageBuilder::from(Request::Wake(
                msg_id_1
                    .as_slice()
                    .try_into()
                    .expect("MessageId inner array size is 32"),
            ))
            .destination(prog_id_1),
        );
        assert_eq!(reply, None);

        let reply = runner
            .get_response_to::<_, u32>(msg_id_1)
            .expect("No response to original message")
            .expect("Unable to parse response to original message");
        assert_eq!(reply, 100);
    }
}
