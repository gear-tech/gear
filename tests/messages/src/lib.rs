#![cfg_attr(not(feature = "std"), feature(alloc_error_handler))]
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};

#[cfg(feature = "std")]
mod code {
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
                value_ptr: *const u8,
            );
        }
    }

    static mut BUFFER: Vec<u8> = Vec::new();

    fn process_request(request: Request) {
        match request {
            Request::SendOnce => {
                msg::send(ActorId::from(0), "SendOnce", 0);
            }
            Request::SendInf => loop {
                msg::send(ActorId::from(0), "SendInf", 0);
            },
            Request::SendPushAfterCommit => unsafe {
                let handle = gear_sys::gr_send_init();
                let mut message_id = [0u8; 32];
                gear_sys::gr_send_commit(
                    handle,
                    message_id.as_mut_ptr(),
                    ActorId::from(0).as_ref().as_ptr(),
                    0u128.to_le_bytes().as_ptr(),
                );
                let data = "bytes";
                gear_sys::gr_send_push(handle, data.as_ptr(), data.len() as u32);
            },
            Request::ReplyOnce => {
                msg::reply("ReplyOnce", 0);
            }
            Request::ReplyTwice => {
                msg::reply("ReplyTwice1", 0);
                msg::reply("ReplyTwice2", 0);
            }
            Request::ReplyPushAfterReply => {
                msg::reply("ReplyPushAfterReply1", 0);
                msg::reply_push("ReplyPushAfterReply2");
            }
        }
    }

    #[no_mangle]
    pub extern "C" fn init() {
        msg::reply((), 0);
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
    use super::Request;
    use common::{RunResult, RunnerContext};

    fn wasm_code() -> &'static [u8] {
        super::code::WASM_BINARY_OPT
    }

    #[test]
    fn program_can_be_initialized() {
        let mut runner = RunnerContext::default();

        // Assertions are performed when decoding reply
        let _reply: () = runner.init_program_with_reply(wasm_code());
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
    }

    /// Check that wasm export '__gear_stack_end' is used correct by core processor.
    /// In this test we check that only last page == 16 is updated after execution,
    /// all other pages are stack pages, so must be skipped.
    #[test]
    fn check_gear_stack_end() {
        let _ = env_logger::Builder::from_env(env_logger::Env::default()).try_init();
        let mut runner = RunnerContext::default();
        let (_, prog_id) = runner.init_program(wasm_code());
        let actors = runner.get_actors_ref();
        let actor = actors
            .get(&prog_id)
            .expect("Must be in actors")
            .as_ref()
            .expect("Must be some");
        let pages = actor.program.get_pages();

        assert_eq!(
            pages.len(),
            1,
            "Must have only one page - with static data. Stack pages aren't updated"
        );
        let num = pages.iter().next().expect("Must have one page").0.raw();
        assert_eq!(num, 16, "Currently we have 16 pages as stack");
    }
}
