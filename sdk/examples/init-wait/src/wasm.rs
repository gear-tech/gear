// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{MessageId, collections::BTreeMap, exec, msg, prelude::*};

#[derive(PartialEq, Debug)]
enum State {
    NotInited,
    WaitForReply,
    Inited,
}

static mut STATE: State = State::NotInited;
static mut INIT_MESSAGE: MessageId = MessageId::new([0; 32]);
static mut TEST_DYNAMIC_MEMORY: BTreeMap<u32, ()> = BTreeMap::new();

#[unsafe(no_mangle)]
extern "C" fn init() {
    let state = unsafe { static_mut!(STATE) };
    match state {
        State::NotInited => {
            for k in 0..20 {
                unsafe { static_mut!(TEST_DYNAMIC_MEMORY).insert(k, ()) };
            }

            unsafe { INIT_MESSAGE = msg::id() };
            msg::send(msg::source(), b"PING", 0).unwrap();
            *state = State::WaitForReply;
            exec::wait();
        }
        State::WaitForReply => {
            for k in 0..20 {
                unsafe { static_mut!(TEST_DYNAMIC_MEMORY).insert(k, ()) };
            }
            for k in 0..25 {
                let _ = unsafe { static_mut!(TEST_DYNAMIC_MEMORY).remove(&k) };
            }

            *state = State::Inited;
        }
        _ => panic!("unreachable!"),
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    if unsafe { STATE != State::Inited } {
        panic!("not initialized");
    }

    msg::reply(b"Hello, world!", 0).unwrap();
}

#[unsafe(no_mangle)]
extern "C" fn handle_reply() {
    if unsafe { STATE == State::WaitForReply } {
        for k in 20..40 {
            unsafe { static_mut!(TEST_DYNAMIC_MEMORY).insert(k, ()) };
        }
        exec::wake(unsafe { INIT_MESSAGE }).expect("Failed to wake message");
    }
}
