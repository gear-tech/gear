// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::{decode_log, IMirror};
use alloy::{primitives::B256, rpc::types::eth::Log, sol_types::SolEvent};
use anyhow::Result;
use ethexe_common::events::{MirrorEvent, MirrorRequestEvent};
use signatures::*;

pub mod signatures {
    use super::*;

    crate::signatures_consts! {
        IMirror;
        EXECUTABLE_BALANCE_TOP_UP_REQUESTED: ExecutableBalanceTopUpRequested,
        MESSAGE_QUEUEING_REQUESTED: MessageQueueingRequested,
        MESSAGE: Message,
        MESSAGE_CALL_FAILED: MessageCallFailed,
        REPLY_QUEUEING_REQUESTED: ReplyQueueingRequested,
        REPLY: Reply,
        REPLY_CALL_FAILED: ReplyCallFailed,
        STATE_CHANGED: StateChanged,
        VALUE_CLAIMED: ValueClaimed,
        VALUE_CLAIMING_REQUESTED: ValueClaimingRequested,
    }

    pub const REQUESTS: &[B256] = &[
        EXECUTABLE_BALANCE_TOP_UP_REQUESTED,
        MESSAGE_QUEUEING_REQUESTED,
        REPLY_QUEUEING_REQUESTED,
        VALUE_CLAIMING_REQUESTED,
    ];
}

pub fn try_extract_event(log: &Log) -> Result<Option<MirrorEvent>> {
    let Some(topic0) = log.topic0().filter(|&v| ALL.contains(v)) else {
        return Ok(None);
    };

    let event = match *topic0 {
        EXECUTABLE_BALANCE_TOP_UP_REQUESTED => {
            decode_log::<IMirror::ExecutableBalanceTopUpRequested>(log)?.into()
        }
        MESSAGE_QUEUEING_REQUESTED => decode_log::<IMirror::MessageQueueingRequested>(log)?.into(),
        MESSAGE => decode_log::<IMirror::Message>(log)?.into(),
        MESSAGE_CALL_FAILED => decode_log::<IMirror::MessageCallFailed>(log)?.into(),
        REPLY_QUEUEING_REQUESTED => decode_log::<IMirror::ReplyQueueingRequested>(log)?.into(),
        REPLY => decode_log::<IMirror::Reply>(log)?.into(),
        REPLY_CALL_FAILED => decode_log::<IMirror::ReplyCallFailed>(log)?.into(),
        STATE_CHANGED => decode_log::<IMirror::StateChanged>(log)?.into(),
        VALUE_CLAIMED => decode_log::<IMirror::ValueClaimed>(log)?.into(),
        VALUE_CLAIMING_REQUESTED => decode_log::<IMirror::ValueClaimingRequested>(log)?.into(),
        _ => unreachable!("filtered above"),
    };

    Ok(Some(event))
}

pub fn try_extract_request_event(log: &Log) -> Result<Option<MirrorRequestEvent>> {
    if log.topic0().filter(|&v| REQUESTS.contains(v)).is_none() {
        return Ok(None);
    }

    let request_event = try_extract_event(log)?
        .and_then(|v| v.to_request())
        .expect("filtered above");

    Ok(Some(request_event))
}
