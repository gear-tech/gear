use crate::Program;
use gstd::{any::Any, msg, prelude::*};

pub(crate) struct Ping;

impl Program for Ping {
    fn init(_: Box<dyn Any>) -> Self {
        Ping
    }

    fn handle(&mut self) {
        let payload = msg::load_bytes().expect("Failed to load payload");

        if payload == b"PING" {
            msg::reply_bytes("PONG", 0).expect("Failed to send reply");
        }
    }
}
