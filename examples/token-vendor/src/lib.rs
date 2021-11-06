#![no_std]
#![feature(const_btree_new)]

extern crate alloc;

use gstd::{debug, exec, msg, prelude::*, ProgramId};
use gstd_async::msg as msg_async;

use alloc::collections::BTreeSet;
use codec::{Decode, Encode};
use primitive_types::H256;
use scale_info::TypeInfo;

const GAS_RESERVE: u64 = 500_000_000;

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

        self.admins.insert(owner_id);

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

            if let Some(owner) = self.owner_id {
                self.admins.insert(owner);
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
    ProgramId(H256),
}

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
        output: String,
}

#[gstd_async::main]
async fn main() {
    let action: Action = msg::load().expect("Unable to decode Action");

    debug!("Got Action: {:?}", action);

    let source = msg::source();

    match action {
        Action::UpdateConfig(config) => {
            if unsafe { !STATE.admins.contains(&source) } {
                debug!("Sender is not an admin of the workshop");
                return;
            }

            if let Err(e) = unsafe { STATE.update(config) } {
                panic!("Failed to update State: {}", e);
            }

            msg::reply("Config updated", exec::gas_available() - GAS_RESERVE, 0);
        }
        Action::ProgramId(hex) => {
            if unsafe { !STATE.members.contains(&source) } {
                debug!("Sender is not a member of the workshop");
                return;
            }

            let id = ProgramId(hex.to_fixed_bytes());

            let response = msg_async::send_and_wait_for_reply(
                id,
                &String::from("ping").encode(),
                GAS_RESERVE,
                0,
            )
            .await;

            let ping = String::decode(&mut response.as_ref())
                .expect("Failed to decode string from pong-response");

            debug!("Got ping-reply: '{}'", ping);

            if ping.to_lowercase() == "pong" {
                let response = msg_async::send_and_wait_for_reply(
                    id,
                    &String::from("success").encode(),
                    GAS_RESERVE,
                    0,
                )
                .await;

                let success = String::decode(&mut response.as_ref())
                    .expect("Failed to decode string from MemberID-response");

                debug!("Got success-reply: '{}'", success);

                let member_id =
                    hex_to_id(success).expect("Failed to decode hex from MemberId-response");

                if source == member_id {
                    debug!(
                        "SUCCESS:\n  member: {:?}\n  contract: {:?}",
                        member_id, source
                    );

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
    let config: InitConfig = msg::load().expect("Unable to decode InitConfig");

    debug!("Got InitConfig: {:?}", config);

    if let Err(e) = STATE.init(msg::source(), config) {
        panic!("Failed to init State: {}", e);
    }

    debug!("Initialized");
    msg::reply("Initialized", exec::gas_available() - GAS_RESERVE, 0);
}
