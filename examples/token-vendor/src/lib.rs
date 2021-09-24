#![no_std]
#![feature(const_btree_new)]

extern crate alloc;

// for panic/oom handlers
extern crate gstd;

use alloc::collections::BTreeSet;
use core::cell::RefCell;
use core::num::ParseIntError;
use gstd::{ext, msg, prelude::*, ProgramId};
use gstd_async::msg as msg_async;
use gstd_meta::meta;
use lite_json::JsonValue;

meta! {
    title: "GEAR Token Vendor",
    // Any hex ProgramId
    input: Vec<u8>,
    output: Vec<u8>,
    // json config
    init_input: Vec<u8>,
    init_output: Vec<u8>
}

// {
//     "members": [
//       "0600000000000000000000000000000000000000000000000000000000000000",
//       "0700000000000000000000000000000000000000000000000000000000000000"
//     ],
//     "reward": 1,
//     "code": "UPDATE",
//     "admins": [
//       "0100000000000000000000000000000000000000000000000000000000000000"
//     ]
// }
fn parse_config(json: &str) -> Config {
    let mut config = Config::default();
    let json = lite_json::json_parser::parse_json(json).expect("Invalid JSON");
    if let lite_json::JsonValue::Object(obj) = json {
        for (name, value) in obj {
            match name.iter().collect::<String>().as_str() {
                "reward" => {
                    if let JsonValue::Number(num) = value {
                        config.reward = num.integer as u128;
                    };
                }
                "members" => {
                    if let JsonValue::Array(members) = value {
                        for member in members {
                            if let JsonValue::String(member) = member {
                                config.members.push(member.iter().collect());
                            }
                        }
                    };
                }
                "code" => {
                    if let JsonValue::String(code) = value {
                        config.code = code.iter().collect();
                    };
                }
                "admins" => {
                    if let JsonValue::Array(admins) = value {
                        for admin in admins {
                            if let JsonValue::String(admin) = admin {
                                config.admins.push(admin.iter().collect());
                            }
                        }
                    };
                }
                _ => (),
            }
        }
    }
    config
}

#[derive(Debug, Clone)]
struct State {
    owner_id: Option<ProgramId>,
    members: BTreeSet<ProgramId>,
    reward: u128,
    code: String,
    admins: BTreeSet<ProgramId>,
}

#[derive(Debug, Default)]
struct Config {
    reward: u128,
    members: Vec<String>,
    code: String,
    admins: Vec<String>,
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

static mut STATE: RefCell<State> = RefCell::new(State {
    owner_id: None,
    members: BTreeSet::new(),
    reward: 10,
    code: String::new(),
    admins: BTreeSet::new(),
});

#[gstd_async::main]
async fn main() {
    let msg = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");

    ext::debug(&format!("msg: {:?}", msg));
    let state = unsafe { STATE.borrow() };

    // Load json config
    if state.admins.contains(&msg::source()) {
        let config = parse_config(&msg);
        let mut new_state = State::new(
            state.owner_id,
            BTreeSet::new(),
            config.reward,
            config.code,
            BTreeSet::from([state.owner_id.unwrap()]),
        );

        for member in config.members {
            new_state.members.insert(ProgramId::from_slice(
                &decode_hex(&member).expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
            ));
        }

        for admin in config.admins {
            new_state.admins.insert(ProgramId::from_slice(
                &decode_hex(&admin).expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
            ));
        }
        drop(state);
        unsafe {
            STATE.replace(new_state);
        }
        ext::debug("CONFIG UPDATED");
        msg::reply(b"CONFIG UPDATED", gstd::exec::gas_available(), 0);
    } else if state.members.contains(&msg::source()) {
        // If msg::source is registered in workshop then we send a message to id from payload
        let id = ProgramId::from_slice(
            &decode_hex(&msg).expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
        );

        drop(state);
        let reply =
            msg_async::send_and_wait_for_reply(id, b"verify", gstd::exec::gas_available() / 2, 0)
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

            msg::send_with_value(member_id, b"success", 0, unsafe { STATE.borrow().reward });
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

    // json config str
    let config_str =
        String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");

    let config = parse_config(&config_str);

    for member in config.members {
        state.members.insert(ProgramId::from_slice(
            &decode_hex(&member).expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
        ));
    }

    for admin in config.admins {
        state.admins.insert(ProgramId::from_slice(
            &decode_hex(&admin).expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
        ));
    }

    state.code = config.code;

    state.reward = config.reward;

    STATE.replace(state);

    msg::reply(b"INIT", 0, 0);
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}
