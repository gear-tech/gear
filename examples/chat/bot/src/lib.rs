#![no_std]

use core::num::ParseIntError;
use gstd::{debug, msg, prelude::*, ActorId};

use core::convert::TryInto;
use demo_chat::shared::{MemberMessage, RoomMessage};

fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}

#[derive(Debug)]
struct State {
    pub name: &'static str,
}

impl State {
    fn set_name(&mut self, name: &'static str) {
        self.name = name;
    }
    fn name(&self) -> &'static str {
        self.name
    }
}

static mut STATE: State = State { name: "" };

#[no_mangle]
extern "C" fn handle() {
    bot(msg::load().expect("Failed to decode incoming message"));
}

fn bot(message: MemberMessage) {
    use MemberMessage::*;
    unsafe {
        match message {
            Private(text) => {
                debug!(
                    "BOT '{}': received private message from #{}: '{}'",
                    STATE.name(),
                    u64::from_le_bytes(msg::source().as_ref()[0..8].try_into().unwrap()),
                    String::from_utf8(text).expect("invalid utf-8")
                );
            }
            Room(text) => {
                debug!(
                    "BOT '{}': received room message from #{}: '{}'",
                    STATE.name(),
                    u64::from_le_bytes(msg::source().as_ref()[0..8].try_into().unwrap()),
                    String::from_utf8(text).expect("invalid utf-8")
                );
            }
        }
    }
}

#[no_mangle]
extern "C" fn init() {
    let input = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message: should be utf-8");

    let split = input.split(' ').collect::<Vec<_>>();
    match split.len() {
        2 => {
            let (name, room_id) = (&split[0], &split[1]);
            let s: &'static str = Box::leak(name.to_string().into_boxed_str());
            unsafe { STATE.set_name(s) };
            let room_id = ActorId::from_slice(
                &decode_hex(room_id).expect("INITIALIZATION FAILED: INVALID ROOM ID"),
            )
            .expect("Unable to create ActorId");
            msg::send(
                room_id,
                RoomMessage::Join {
                    under_name: name.to_string().into_bytes(),
                },
                0,
            )
            .unwrap();
        }
        _ => {
            debug!("INITIALIZATION FAILED");
        }
    }

    debug!("BOT '{}' created", unsafe { STATE.name() });
}
