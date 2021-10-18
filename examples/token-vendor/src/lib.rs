#![no_std]
#![feature(const_btree_new)]

extern crate alloc;

use gstd::{exec, ext, msg, prelude::*, ProgramId};
use gstd_async::msg as msg_async;
use gstd_async::mutex::Mutex;

use alloc::collections::BTreeSet;
use codec::Decode;
use scale_info::TypeInfo;

const GAS_RESERVE: u64 = 10_000_000;

struct State {
    owner_id: Option<ProgramId>,
    code: String,
    reward: u128,
    admins: BTreeSet<ProgramId>,
    members: BTreeSet<ProgramId>,
}

fn hex_to_id(hex: String) -> Result<ProgramId, u8> {
    let hex = hex.strip_prefix("0x").unwrap_or(&hex);

    hex::decode(hex)
        .map(|bytes| ProgramId::from_slice(&bytes))
        .map_err(|_| 0)
}

fn address_to_id(address: String) -> Result<ProgramId, u8> {
    bs58::decode(address)
        .into_vec()
        .map(|v| ProgramId::from_slice(&v[1..v.len() - 2].to_vec()))
        .map_err(|_| 1)
}

impl State {
    fn init(&mut self, owner_id: ProgramId, config: InitConfig) -> Result<(), &'static str> {
        self.owner_id = Some(owner_id);
        self.code = config.code;
        self.reward = config.reward;

        for admin in config.admins {
            let id = address_to_id(admin).map_err(|_| "Invalid admin address")?;
            self.admins.insert(id);
        }

        for member in config.members {
            let id = address_to_id(member).map_err(|_| "Invalid member address")?;
            self.members.insert(id);
        }

        Ok(())
    }

    fn update(&mut self, config: UpdateConfig) -> Result<(), &'static str> {
        if let Some(code) = config.code {
            self.code = code;
        }

        if let Some(reward) = config.reward {
            self.reward = reward;
        }

        if let Some(admins) = config.admins {
            self.admins.clear();

            for admin in admins {
                let id = address_to_id(admin).map_err(|_| "Invalid admin address")?;
                self.admins.insert(id);
            }
        }

        if let Some(members) = config.members {
            self.members.clear();

            for member in members {
                let id = address_to_id(member).map_err(|_| "Invalid member address")?;
                self.members.insert(id);
            }
        }

        Ok(())
    }
}

#[derive(Debug, Decode, TypeInfo)]
struct InitConfig {
    code: String,
    reward: u128,
    admins: Vec<String>,
    members: Vec<String>,
}

#[derive(Debug, Decode, TypeInfo)]
struct UpdateConfig {
    code: Option<String>,
    reward: Option<u128>,
    admins: Option<Vec<String>>,
    members: Option<Vec<String>>,
}

#[derive(Debug, Decode, TypeInfo)]
enum Action {
    UpdateConfig(UpdateConfig),
    ProgramId(String),
}

static MUTEX: Mutex<()> = Mutex::new(());

static mut STATE: State = State {
    owner_id: None,
    code: String::new(),
    reward: 0,
    admins: BTreeSet::new(),
    members: BTreeSet::new(),
};

gstd::metadata! {
    title: "Workshop token vendor contract",
    init:
        input: InitConfig,
        output: String,
    handle:
        input: Action,
        output: String
}

#[gstd_async::main]
async fn main() {
    let action: Action = msg::load().unwrap_or_else(|_| {
        ext::debug("Unable to decode Action");
        panic!()
    });

    ext::debug(&format!("Got Action: {:?}", action));

    match action {
        Action::UpdateConfig(config) => {
            if let Err(e) = unsafe { STATE.update(config) } {
                ext::debug(&format!("Failed to update State: {}", e));
                panic!()
            }

            msg::reply("Config updated", exec::gas_available() - GAS_RESERVE, 0);
        }
        Action::ProgramId(hex) => {
            let _ = MUTEX.lock().await;

            let source = msg::source();

            if unsafe { !STATE.members.contains(&source) } {
                ext::debug("Sender is not a member of the workshop");
                return;
            }

            let id = hex_to_id(hex).unwrap_or_else(|_| {
                ext::debug("Failed to decode hex from input");
                panic!()
            });

            let response = msg_async::send_and_wait_for_reply(id, b"ping", GAS_RESERVE, 0).await;

            let ping = String::decode(&mut response.as_ref()).unwrap_or_else(|_| {
                ext::debug("Failed to decode string from pong-response");
                panic!()
            });

            ext::debug(&format!("Got ping-reply: '{}'", ping));

            if ping.to_lowercase() == "pong" {
                let response =
                    msg_async::send_and_wait_for_reply(id, b"success", GAS_RESERVE, 0).await;

                let success = String::decode(&mut response.as_ref()).unwrap_or_else(|_| {
                    ext::debug("Failed to decode string from MemberID-response");
                    panic!()
                });

                ext::debug(&format!("Got success-reply: '{}'", success));

                let member_id = hex_to_id(success).unwrap_or_else(|_| {
                    ext::debug("Failed to decode hex from MemberId-response");
                    panic!()
                });

                if source == member_id {
                    ext::debug(&format!(
                        "SUCCESS:\n  member: {:?}\n  contract: {:?}",
                        member_id, source
                    ));

                    msg::reply("Success", exec::gas_available() - GAS_RESERVE, unsafe {
                        STATE.reward
                    });

                    unsafe { STATE.members.remove(&member_id) };
                }
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let config: InitConfig = msg::load().unwrap_or_else(|_| {
        ext::debug("Unable to decode InitConfig");
        panic!()
    });

    ext::debug(&format!("Got InitConfig: {:?}", config));

    if let Err(e) = STATE.init(msg::source(), config) {
        ext::debug(&format!("Failed to init State: {}", e));
        panic!()
    }

    msg::reply("Initialized", exec::gas_available() - GAS_RESERVE, 0);
}
