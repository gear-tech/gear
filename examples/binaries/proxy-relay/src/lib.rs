// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use gstd::Vec;
use scale_info::TypeInfo;

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct ResendPushData {
    pub destination: gstd::ActorId,
    pub start: Option<u32>,
    // flag indicates if the end index is included
    pub end: Option<(u32, bool)>,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub enum RelayCall {
    Resend(gstd::ActorId),
    ResendWithGas(gstd::ActorId, u64),
    ResendPush(Vec<ResendPushData>),
    Rereply,
    RereplyWithGas(u64),
    RereplyPush,
}

#[cfg(not(feature = "std"))]
mod wasm {
    use super::*;
    use gstd::{msg, ToString};

    static mut RELAY_CALL: Option<RelayCall> = None;

    gstd::metadata! {
        title: "tests-proxy-relay",
        handle:
            input: RelayCall,
    }

    fn resend_push(resend_pushes: &[ResendPushData]) {
        for data in resend_pushes {
            let msg_handle = msg::send_init().expect("Failed to obtain new message handle");

            let ResendPushData {
                destination,
                start,
                end,
            } = data;

            let end = end.map(|(e, flag)| (e as usize, flag));
            match start.map(|s| s as usize) {
                Some(s) => match end {
                    None => {
                        msg::resend_push(msg_handle, s..).expect("Push failed");
                    }
                    Some((e, included @ true)) => {
                        msg::resend_push(msg_handle, s..=e).expect("Push failed");
                    }
                    Some((e, _)) => {
                        msg::resend_push(msg_handle, s..e).expect("Push failed");
                    }
                },
                None => match end {
                    None => {
                        msg::resend_push(msg_handle, ..).expect("Push failed");
                    }
                    Some((e, included @ true)) => {
                        msg::resend_push(msg_handle, ..=e).expect("Push failed");
                    }
                    Some((e, _)) => {
                        msg::resend_push(msg_handle, ..e).expect("Push failed");
                    }
                },
            }

            msg::send_commit(msg_handle, *destination, msg::value()).expect("Commit failed");
        }
    }

    #[no_mangle]
    unsafe extern "C" fn handle() {
        use RelayCall::*;

        match RELAY_CALL.as_ref().expect("Relay call is not initialized") {
            Resend(d) => {
                msg::resend(*d, msg::value(), ..msg::size() as usize).expect("Resend failed");
            }
            ResendWithGas(d, gas) => {
                msg::resend_with_gas(*d, *gas, msg::value(), ..).expect("Resend wgas failed");
            }
            ResendPush(data) => {
                resend_push(data);
            }
            Rereply => {
                msg::rereply(msg::value(), 0..msg::size() as usize).expect("Rereply failed");
            }
            RereplyPush => {
                msg::rereply_push(0..).expect("Push failed");
                msg::reply_commit(msg::value()).expect("Commit failed");
            }
            RereplyWithGas(gas) => {
                msg::rereply_with_gas(*gas, msg::value(), ..).expect("Rereply wgas failed");
            }
        }
    }

    #[no_mangle]
    unsafe extern "C" fn init() {
        RELAY_CALL = Some(msg::load().expect("Failed to decode `RelayCall'"));
    }
}
