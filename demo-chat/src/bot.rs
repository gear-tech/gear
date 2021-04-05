use gstd::{ext, msg, ProgramId};

mod shared;

use codec::{Decode as _, Encode as _};
use shared::{MemberMessage, RoomMessage};
use core::convert::TryInto;

#[derive(Debug)]
struct State {
    pub name: &'static str,
}

impl State {
    fn set_name(&mut self, name: &'static str) {
        self.name = &name;
    }
    fn name(&self) -> &'static str {
        self.name
    }
}

static mut STATE: State = State { name: "" };

#[no_mangle]
pub unsafe extern "C" fn handle() {
    bot(MemberMessage::decode(&mut &msg::load()[..]).expect("Failed to decode incoming message"));
}

fn bot(message: MemberMessage) {
    use shared::MemberMessage::*;
    unsafe {
        match message {
            Private(text) => {
                ext::debug(&format!(
                    "BOT '{}': received private message from #{}: '{}'",
                    STATE.name(),
                    u64::from_le_bytes(msg::source().as_slice()[0..8].try_into().unwrap()),
                    text
                ));
            }
            Room(text) => {
                ext::debug(&format!(
                    "BOT '{}': received room message from #{}: '{}'",
                    STATE.name(),
                    u64::from_le_bytes(msg::source().as_slice()[0..8].try_into().unwrap()),
                    text
                ));
            }
        }
    }
}

pub fn send_room(id: u64, msg: RoomMessage) {
    let mut encoded = vec![];
    msg.encode_to(&mut encoded);
    msg::send(ProgramId::from(id), &encoded, u64::MAX);
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let input = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");

    let split = input.split(' ').collect::<Vec<_>>();
    match split.len() {
        2 => {
            let (name, room_id) = (&split[0], &split[1]);
            let s: &'static str = Box::leak(name.to_string().into_boxed_str());
            STATE.set_name(s);
            let room_id = room_id
                .parse::<u64>()
                .expect("INTIALIZATION FAILED: INVALID ROOM ID");
            send_room(
                room_id,
                RoomMessage::Join {
                    under_name: name.to_string(),
                },
            );
        }
        _ => {
            ext::debug("INITLAIZATION FAILED");
        }
    }

    ext::debug(&format!("BOT '{}' created", STATE.name()));
}

fn main() {}
