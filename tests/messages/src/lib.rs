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
    SendOnce,
    SendInf,
    SendPushAfterCommit,
    ReplyOnce,
    ReplyTwice,
    ReplyPushAfterReply,
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Reply {
    Empty,
    Error,
}

#[cfg(not(feature = "std"))]
mod wasm {
    extern crate alloc;

    use codec::{Decode, Encode};
    use gstd::{debug, msg, prelude::*, ActorId};

    use super::Request;

    mod gear_sys {
        extern "C" {
            pub fn gr_send_init() -> u32;
            pub fn gr_send_push(handle: u32, data_ptr: *const u8, data_len: u32);
            pub fn gr_send_commit(
                handle: u32,
                message_id_ptr: *mut u8,
                program: *const u8,
                gas_limit: u64,
                value_ptr: *const u8,
            );
        }
    }

    static mut BUFFER: Vec<u8> = Vec::new();

    fn process_request(request: Request) {
        match request {
            Request::SendOnce => {
                msg::send(ActorId::from(0), "SendOnce", 0, 0);
            }
            Request::SendInf => loop {
                msg::send(ActorId::from(0), "SendInf", 0, 0);
            },
            Request::SendPushAfterCommit => unsafe {
                let handle = gear_sys::gr_send_init();
                let mut message_id = [0u8; 32];
                gear_sys::gr_send_commit(
                    handle,
                    message_id.as_mut_ptr(),
                    ActorId::from(0).as_ref().as_ptr(),
                    0,
                    0u128.to_le_bytes().as_ptr(),
                );
                let data = "bytes";
                gear_sys::gr_send_push(handle, data.as_ptr(), data.len() as u32);
            },
            Request::ReplyOnce => {
                msg::reply("ReplyOnce", 0, 0);
            }
            Request::ReplyTwice => {
                msg::reply("ReplyTwice1", 0, 0);
                msg::reply("ReplyTwice2", 0, 0);
            }
            Request::ReplyPushAfterReply => {
                msg::reply("ReplyPushAfterReply1", 0, 0);
                msg::reply_push("ReplyPushAfterReply2");
            }
        }
    }

    #[no_mangle]
    pub extern "C" fn init() {
        msg::reply((), 0, 0);
    }

    #[no_mangle]
    pub extern "C" fn handle() {
        msg::load::<Request>()
            .map(process_request)
            .unwrap_or_else(|e| {
                debug!("Error processing request: {:?}", e);
            });
    }
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    use super::{native, Request};

    use codec::Encode;
    use common::{Error::Panic, RunResult, RunnerContext};

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
        let _reply: () = runner.init_program_with_reply(wasm_code());
    }

    #[test]
    fn send_normal() {
        let mut runner = RunnerContext::default();
        runner.init_program(wasm_code());

        let report = runner.request_report::<_, ()>(Request::SendOnce);
        assert_eq!(report.result, RunResult::Normal);

        let expected_payload = "SendOnce".encode();
        assert!(runner
            .storage()
            .log
            .get()
            .iter()
            .any(|log| log.payload.as_ref() == &expected_payload))
    }

    #[test]
    fn send_message_limit() {
        let mut runner = RunnerContext::default();
        runner.init_program(wasm_code());

        let report = runner.request_report::<_, ()>(Request::SendInf);
        assert_eq!(
            report.result,
            RunResult::Trap(String::from("Message init error"))
        );
        // TODO: Should log be checked for messages?
        // assert_eq!(runner.storage().log.get().len(), 130)
    }

    #[test]
    fn send_push_after_commit() {
        let mut runner = RunnerContext::default();
        runner.init_program(wasm_code());

        let report = runner.request_report::<_, ()>(Request::SendPushAfterCommit);
        assert_eq!(
            report.result,
            RunResult::Trap(String::from("Payload push error"))
        );
    }

    #[test]
    fn reply_normal() {
        let mut runner = RunnerContext::default();
        runner.init_program(wasm_code());

        let report = runner.request_report::<_, String>(Request::ReplyOnce);
        assert_eq!(report.result, RunResult::Normal);
        assert_eq!(report.response, Some(Ok(String::from("ReplyOnce"))));
    }

    #[test]
    fn reply_twice() {
        let mut runner = RunnerContext::default();
        runner.init_program(wasm_code());

        let report = runner.request_report::<_, String>(Request::ReplyTwice);
        assert_eq!(
            report.result,
            RunResult::Trap(String::from("Reply commit error"))
        );
        assert_eq!(report.response, Some(Err(Panic)));
    }

    #[test]
    fn reply_push_after_reply() {
        let mut runner = RunnerContext::default();
        runner.init_program(wasm_code());

        let report = runner.request_report::<_, String>(Request::ReplyPushAfterReply);
        assert_eq!(
            report.result,
            RunResult::Trap(String::from("Reply payload push error"))
        );
        assert_eq!(report.response, Some(Err(Panic)));
    }
}
