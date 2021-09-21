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
    // Hex Program Ids coma separated
    init_input: Vec<u8>,
    init_output: Vec<u8>
}

fn parse_config(json: &str) -> Config {
    let mut config = Config::default();
    let json = lite_json::json_parser::parse_json(json).expect("Invalid JSON");
    match json {
        lite_json::JsonValue::Object(obj) => {
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
                    _ => (),
                }
            }
        }
        _ => (),
    }
    config
}

#[derive(Debug, Clone)]
struct State {
    owner_id: Option<ProgramId>,
    members: BTreeSet<ProgramId>,
    reward: u128,
    code: String,
}

#[derive(Debug, Default)]
struct Config {
    reward: u128,
    members: Vec<String>,
    code: String,
}

impl State {
    fn new(
        owner_id: Option<ProgramId>,
        members: BTreeSet<ProgramId>,
        reward: u128,
        code: String,
    ) -> Self {
        Self {
            owner_id,
            members,
            reward,
            code,
        }
    }
}

static STATE: spin::Mutex<RefCell<State>> = spin::Mutex::new(RefCell::new(State {
    owner_id: None,
    members: BTreeSet::new(),
    reward: 10,
    code: String::new(),
}));

#[gstd_async::main]
async fn main() {
    let msg = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    let id =
        ProgramId::from_slice(&decode_hex(&msg).expect("DECODE HEX FAILED: INVALID PROGRAM ID"));

    let state = STATE.lock();
    ext::debug(&format!("members: {:?}", state.borrow().members));

    // If msg::source is registered in workshop then we send a message to id from payload
    if state.borrow().members.contains(&msg::source()) {
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
            let state = STATE.lock();

            state.borrow_mut().members.remove(&member_id);
            msg::send_with_value(member_id, b"success", 0, state.borrow().reward);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let owner_id = msg::source();

    let mut state = State::new(Some(owner_id), BTreeSet::new(), 0, String::from(""));

    // json config str
    let config_str =
        String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");

    let config = parse_config(&config_str);

    for member in config.members {
        state.members.insert(ProgramId::from_slice(
            &decode_hex(&member).expect("DECODE HEX FAILED: INVALID PROGRAM ID"),
        ));
    }

    state.code = config.code;

    state.reward = config.reward;

    STATE.lock().replace(state);

    msg::reply(b"INIT", 0, 0);
}

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}
