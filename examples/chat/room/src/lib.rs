#![no_std]

use gstd::{debug, msg, prelude::*, ActorId};

use demo_chat::shared::{MemberMessage, RoomMessage};

#[derive(Debug)]
struct State {
    room_name: &'static str,
    members: Vec<(ActorId, String)>,
}

impl State {
    fn set_room_name(&mut self, name: &'static str) {
        self.room_name = name;
    }
    fn add_member(&mut self, member: (ActorId, String)) {
        self.members.push(member);
    }
    fn get_member(&self, id: ActorId) -> Option<&(ActorId, String)> {
        self.members.iter().find(|(member, _name)| *member == id)
    }
    fn room_name(&self) -> &'static str {
        debug!("room_name ptr -> {:p}", self.room_name);
        self.room_name
    }
}

static mut STATE: State = State {
    room_name: "",
    members: vec![],
};

#[no_mangle]
pub unsafe extern "C" fn handle() {
    room(msg::load().expect("Failed to decode incoming message"));
}

unsafe fn room(room_msg: RoomMessage) {
    use RoomMessage::*;

    match room_msg {
        Join { under_name } => {
            let under_name = String::from_utf8(under_name).expect("Invalid utf-8");

            debug!("ROOM '{}': '{}' joined", STATE.room_name(), under_name);
            STATE.add_member((msg::source(), under_name));
        }
        Yell { text } => {
            debug!("Yell ptr -> {:p}", text.as_ptr());
            for &(id, _) in STATE.members.iter() {
                if id != msg::source() {
                    msg::send(
                        id,
                        MemberMessage::Room(
                            format!(
                                "{}: {}",
                                STATE
                                    .get_member(msg::source())
                                    .unwrap_or(&(ActorId::default(), STATE.room_name().to_string()))
                                    .1,
                                String::from_utf8(text.clone()).expect("Invalid utf-8"),
                            )
                            .into_bytes(),
                        ),
                        0,
                    )
                    .unwrap();
                }
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {
    let s: &'static str = Box::leak(
        String::from_utf8(msg::load_bytes())
            .expect("Invalid message: should be utf-8")
            .into_boxed_str(),
    );
    STATE.set_room_name(s);
    debug!("ROOM '{}' created", STATE.room_name());
}
