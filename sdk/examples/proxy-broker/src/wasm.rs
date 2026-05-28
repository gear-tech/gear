// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Basic implementation of the proxy-broker for demo purpose only.

use gbuiltin_proxy::Request;
use gstd::{ActorId, actor_id, debug, errors::Error, msg};

// Proxy builtin actor program id (hardcoded for all runtimes);
//
// Calculated as hash((b"built/in", 3u64).encode())
const BUILTIN_ADDRESS: ActorId =
    actor_id!("0x8263cd9fc648e101f1cd8585dc0b193445c3750a63bf64a39cdf58de14826299");

#[gstd::async_main]
async fn main() {
    let request: Request = msg::load().expect("handle: invalid payload received");
    match request {
        add_proxy @ Request::AddProxy { .. } => {
            debug!(
                "handle: Sending `add_proxy` request with data {:?}",
                add_proxy
            );

            send_request(add_proxy).await;
        }
        remove_proxy @ Request::RemoveProxy { .. } => {
            debug!(
                "handle: Sending `remove_proxy` request with data {:?}",
                remove_proxy
            );

            send_request(remove_proxy).await;
        }
    }
}

async fn send_request(req: Request) {
    let res = msg::send_for_reply(BUILTIN_ADDRESS, req, 0, 0)
        .expect("handle::send_request: failed sending message for reply")
        .await;
    match res {
        Ok(_) => {
            debug!("handle::send_request: Success reply from builtin actor received");
            msg::reply_bytes(b"Success", 0).expect("Failed to send reply");
        }
        Err(e) => {
            debug!("handle::send_request: Error reply from builtin actor received: {e:?}");
            match e {
                Error::ErrorReply(payload, _) => {
                    panic!("{}", payload);
                }
                _ => panic!("Error in upstream program"),
            }
        }
    }
}
