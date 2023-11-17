use crate::Program;
use gstd::{any::Any, msg, prelude::*};

pub(crate) struct PiggyBank;

impl Program for Ping {
    fn init(_: Box<dyn Any>) -> Self {
        PiggyBank
    }

    fn handle(&mut self) {
        msg::with_read_on_stack(|msg| {
            let available_value = exec::value_available();
            let value = msg::value();
            debug!("inserted: {value}, total: {available_value}");

            if msg.expect("Failed to load payload bytes") == b"smash" {
                debug!("smashing, total: {available_value}");
                msg::reply_bytes(b"send", available_value).unwrap();
            }
        });
    }
}
