// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{RelayCall, ResendPushData};
use gstd::{
    msg::{self, MessageHandle},
    prelude::*,
};

static mut RELAY_CALL: Option<RelayCall> = None;

fn resend_push(resend_pushes: &[ResendPushData], size: usize) {
    for data in resend_pushes {
        let msg_handle = MessageHandle::init().expect("Failed to obtain new message handle");

        let ResendPushData {
            destination,
            start,
            end,
        } = data;

        let end = end.map(|(e, flag)| (e as usize, flag));
        match start.map(|s| s as usize) {
            Some(s) => match end {
                None => {
                    msg_handle.push_input(s..size).expect("Push failed");
                }
                Some((e, true)) => {
                    msg_handle.push_input(s..=e).expect("Push failed");
                }
                Some((e, _)) => {
                    msg_handle.push_input(s..e).expect("Push failed");
                }
            },
            None => match end {
                None => {
                    msg_handle.push_input(..size).expect("Push failed");
                }
                Some((e, true)) => {
                    msg_handle.push_input(..=e).expect("Push failed");
                }
                Some((e, _)) => {
                    msg_handle.push_input(..e).expect("Push failed");
                }
            },
        }

        msg_handle
            .commit(*destination, msg::value())
            .expect("Commit failed");
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    use RelayCall::*;
    let relay_call = unsafe {
        static_ref!(RELAY_CALL)
            .as_ref()
            .expect("Relay call is not initialized")
    };
    let size = msg::size();

    match relay_call {
        Resend(d) => {
            msg::send_input(*d, msg::value(), ..size).expect("Resend failed");
        }
        ResendWithGas(d, gas) => {
            msg::send_input_with_gas(*d, *gas, msg::value(), ..size).expect("Resend wgas failed");
        }
        ResendPush(data) => {
            resend_push(data, size);
        }
        Rereply => {
            msg::reply_input(msg::value(), 0..size).expect("Rereply failed");
        }
        RereplyPush => {
            msg::reply_push_input(0..size).expect("Push failed");
            msg::reply_commit(msg::value()).expect("Commit failed");
        }
        RereplyWithGas(gas) => {
            msg::reply_input_with_gas(*gas, msg::value(), ..size).expect("Rereply wgas failed");
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    unsafe { RELAY_CALL = Some(msg::load().expect("Failed to decode `RelayCall'")) };
}
