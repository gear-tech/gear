// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

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
