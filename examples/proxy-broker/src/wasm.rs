// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Basic implementation of the proxy-broker for demo purpose only.

use gbuiltin_proxy::Request;
use gstd::{debug, errors::Error, msg, ActorId};
use hex_literal::hex;

// Proxy builtin actor program id (hardcoded for all runtimes);
//
// Calculated as hash((b"built/in", 3u64).encode())
const BUILTIN_ADDRESS: ActorId = ActorId::new(hex!(
    "f2816ced0b15749595392d3a18b5a2363d6fefe5b3b6153739f218151b7acdbf"
));

#[gstd::async_main]
async fn main() {
    let request: Request = msg::load().expect("handle: invalid payload received");
    match request {
        add_proxy @ Request::AddProxy { .. } => {
            debug!(
                "[ProxyBroker]: Sending `add_proxy` request with data {:?}",
                add_proxy
            );
            let res = msg::send_for_reply(BUILTIN_ADDRESS, add_proxy, 0, 0)
                .expect("handle::add_proxy: failed sending message for reply")
                .await;
            match res {
                Ok(_) => {
                    debug!("[ProxyBroker] Success reply from builtin actor received");
                    msg::reply_bytes(b"Success", 0).expect("Failed to send reply");
                }
                Err(e) => {
                    debug!("[ProxyBroker] Error reply from builtin actor received: {e:?}");
                    match e {
                        Error::ErrorReply(payload, _) => {
                            panic!("{}", payload);
                        }
                        _ => panic!("Error in upstream program"),
                    }
                }
            }
        }
    }
}
