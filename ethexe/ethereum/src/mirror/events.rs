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

use crate::IMirror;
use alloy::{rpc::types::eth::Log, sol_types::SolEvent};
use anyhow::Result;
use ethexe_common::mirror;
use gprimitives::H256;

pub mod signatures {
    use super::{IMirror, SolEvent, H256};

    pub const EXECUTABLE_BALANCE_TOP_UP_REQUESTED: H256 =
        H256(IMirror::ExecutableBalanceTopUpRequested::SIGNATURE_HASH.0);
    pub const MESSAGE_QUEUEING_REQUESTED: H256 =
        H256(IMirror::MessageQueueingRequested::SIGNATURE_HASH.0);
    pub const MESSAGE: H256 = H256(IMirror::Message::SIGNATURE_HASH.0);
    pub const REPLY_QUEUEING_REQUESTED: H256 =
        H256(IMirror::ReplyQueueingRequested::SIGNATURE_HASH.0);
    pub const REPLY: H256 = H256(IMirror::Reply::SIGNATURE_HASH.0);
    pub const STATE_CHANGED: H256 = H256(IMirror::StateChanged::SIGNATURE_HASH.0);
    pub const VALUE_CLAIMED: H256 = H256(IMirror::ValueClaimed::SIGNATURE_HASH.0);
    pub const VALUE_CLAIMING_REQUESTED: H256 =
        H256(IMirror::ValueClaimingRequested::SIGNATURE_HASH.0);

    pub const ALL: [H256; 8] = [
        EXECUTABLE_BALANCE_TOP_UP_REQUESTED,
        MESSAGE_QUEUEING_REQUESTED,
        MESSAGE,
        REPLY_QUEUEING_REQUESTED,
        REPLY,
        STATE_CHANGED,
        VALUE_CLAIMED,
        VALUE_CLAIMING_REQUESTED,
    ];
}

pub fn try_extract_event(log: &Log) -> Result<Option<mirror::Event>> {
    use crate::decode_log;
    use signatures::*;

    let Some(topic0) = log.topic0().map(|v| H256(v.0)) else {
        return Ok(None);
    };

    // TODO (breathx): pattern matching issue for primitive_types::H256... ????
    let event = match topic0 {
        b if b == EXECUTABLE_BALANCE_TOP_UP_REQUESTED => {
            decode_log::<IMirror::ExecutableBalanceTopUpRequested>(log)?.into()
        }
        b if b == MESSAGE_QUEUEING_REQUESTED => {
            decode_log::<IMirror::MessageQueueingRequested>(log)?.into()
        }
        b if b == MESSAGE => decode_log::<IMirror::Message>(log)?.into(),
        b if b == REPLY_QUEUEING_REQUESTED => {
            decode_log::<IMirror::ReplyQueueingRequested>(log)?.into()
        }
        b if b == REPLY => decode_log::<IMirror::Reply>(log)?.into(),
        b if b == STATE_CHANGED => decode_log::<IMirror::StateChanged>(log)?.into(),
        b if b == VALUE_CLAIMED => decode_log::<IMirror::ValueClaimed>(log)?.into(),
        b if b == VALUE_CLAIMING_REQUESTED => {
            decode_log::<IMirror::ValueClaimingRequested>(log)?.into()
        }
        _ => return Ok(None),
    };

    Ok(Some(event))
}
