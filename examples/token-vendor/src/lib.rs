#![no_std]
#![feature(const_btree_new)]

extern crate alloc;

use alloc::collections::BTreeSet;
use alloc::str;
use codec::{Decode, Encode};
use core::cell::RefCell;
use core::num::ParseIntError;
use gstd::{ext, msg, prelude::*, ProgramId};
use gstd_async::msg as msg_async;
use scale_info::TypeInfo;

gstd::metadata! {
    title: "GEAR Token Vendor",
    init:
        input: Config,
        output: Vec<u8>,
    handle:
        input: Action,
        output: Vec<u8>
}

#[derive(Debug, Clone)]
struct State {
    owner_id: Option<ProgramId>,
    members: BTreeSet<ProgramId>,
    reward: u128,
    code: String,
    admins: BTreeSet<ProgramId>,
}

impl State {
    fn new(
        owner_id: Option<ProgramId>,
        members: BTreeSet<ProgramId>,
        reward: u128,
        code: String,
        admins: BTreeSet<ProgramId>,
    ) -> Self {
        Self {
            owner_id,
            members,
            reward,
            code,
            admins,
        }
    }
}

#[derive(Debug, TypeInfo, Encode, Decode)]
enum Action {
    UpdateConfig(Config),
    ProgramId(Vec<u8>),
}

#[derive(Debug, TypeInfo, Encode, Decode)]
struct Config {
    reward: u128,
    members: Vec<Vec<u8>>,
    code: Vec<u8>,
    admins: Vec<Vec<u8>>,
}

static mut STATE: RefCell<State> = RefCell::new(State {
    owner_id: None,
    members: BTreeSet::new(),
    reward: 10,
    code: String::new(),
    admins: BTreeSet::new(),
});

#[gstd_async::main]
async fn main() {
    let msg: Action = msg::load().expect("Invalid message: should be utf-8");

    // ext::debug(&format!("msg: {:?}", msg));
    let state = unsafe { STATE.borrow().clone() };

    match msg {
        Action::UpdateConfig(config) => {
            if state.admins.contains(&msg::source()) {
                unsafe {
                    STATE.replace_with(|mut state| {
                        state.reward = config.reward;
                        state.code = String::from_utf8(config.code).unwrap();
                        state.members.clear();
                        state.admins = [state.owner_id.unwrap()].into();

                        for member in config.members {
                            state.members.insert(ProgramId::from_slice(
                                &decode_hex(str::from_utf8(&member).unwrap())
                                    .expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
                            ));
                        }

                        for admin in config.admins {
                            state.admins.insert(ProgramId::from_slice(
                                &decode_hex(str::from_utf8(&admin).unwrap())
                                    .expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
                            ));
                        }

                        state.clone()
                    });
                }
                ext::debug("CONFIG UPDATED");
                msg::reply(b"CONFIG UPDATED", gstd::exec::gas_available(), 0);
            }
        }
        Action::ProgramId(program_id) => {
            if state.members.contains(&msg::source()) {
                // If msg::source is registered in workshop then we send a message to id from payload
                let id = ProgramId::from_slice(
                    &decode_hex(
                        str::from_utf8(&program_id)
                            .expect("Invalid ProgramId: should be utf-8 hex string"),
                    )
                    .expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
                );

                let reply = msg_async::send_and_wait_for_reply(
                    id,
                    b"verify",
                    gstd::exec::gas_available() / 2,
                    0,
                )
                .await;

                let reply = String::from_utf8(reply).expect("Invalid message: should be utf-8");

                let member_id = ProgramId::from_slice(
                    &decode_hex(&reply).expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
                );

                if msg::source() == member_id {
                    ext::debug(&format!(
                        "SUCCESS:\nmember: {:?}\ncontract: {:?}",
                        id,
                        msg::source()
                    ));

                    unsafe { STATE.borrow_mut().members.remove(&member_id) };
                    msg::send(member_id, b"success", 0, state.reward);
                }
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let owner_id = msg::source();

    let mut state = State::new(
        Some(owner_id),
        BTreeSet::new(),
        0,
        String::from(""),
        BTreeSet::from([owner_id]),
    );

    let config: Config = msg::load().expect("INVALID INIT PAYLOAD");

    ext::debug("config loaded");
    for member in config.members {
        state.members.insert(ProgramId::from_slice(
            &decode_hex(str::from_utf8(&member).unwrap())
                .expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
        ));
    }

    for admin in config.admins {
        state.admins.insert(ProgramId::from_slice(
            &decode_hex(str::from_utf8(&admin).unwrap())
                .expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
        ));
    }

    state.code = String::from_utf8(config.code).unwrap();

    state.reward = config.reward;

    STATE.replace(state);
    ext::debug("state updated");

    msg::reply(b"INIT", 0, 0);
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}
