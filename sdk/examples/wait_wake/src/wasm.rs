// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::Request;
use gstd::{MessageId, collections::BTreeMap, exec, msg, prelude::*};

static mut ECHOES: Option<BTreeMap<MessageId, u32>> = None;

fn process_request(request: Request) {
    match request {
        Request::EchoWait(n) => {
            unsafe {
                static_mut!(ECHOES)
                    .get_or_insert_with(BTreeMap::new)
                    .insert(msg::id(), n)
            };
            exec::wait();
        }
        Request::Wake(id) => exec::wake(id.into()).unwrap(),
    }
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    msg::reply((), 0).unwrap();
}

#[unsafe(no_mangle)]
extern "C" fn handle() {
    if let Some(reply) = unsafe {
        static_mut!(ECHOES)
            .get_or_insert_with(BTreeMap::new)
            .remove(&msg::id())
    } {
        msg::reply(reply, 0).unwrap();
    } else {
        msg::load::<Request>().map(process_request).unwrap();
    }
}
