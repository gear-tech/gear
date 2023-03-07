// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Module for signal-management and waking concrete message based on reply
//! received.

use crate::{prelude::Vec, MessageId};
use core::task::{Context, Waker};
use hashbrown::HashMap;

pub type Payload = Vec<u8>;
pub type StatusCode = i32;

#[derive(Debug)]
pub(crate) enum ReplyPoll {
    None,
    Pending,
    Some((Payload, StatusCode)),
}

struct WakeSignal {
    message_id: MessageId,
    payload: Option<(Payload, StatusCode)>,
    waker: Option<Waker>,
}

pub(crate) struct WakeSignals {
    signals: HashMap<MessageId, WakeSignal>,
}

impl WakeSignals {
    pub fn new() -> Self {
        Self {
            signals: HashMap::new(),
        }
    }

    pub fn register_signal(&mut self, waiting_reply_to: MessageId) {
        self.signals.insert(
            waiting_reply_to,
            WakeSignal {
                message_id: crate::msg::id(),
                payload: None,
                waker: None,
            },
        );
    }

    pub fn record_reply(&mut self) {
        if let Some(signal) = self
            .signals
            .get_mut(&crate::msg::reply_to().expect("Shouldn't be called with incorrect context"))
        {
            signal.payload = Some((
                crate::msg::load_bytes().expect("Failed to load bytes"),
                crate::msg::status_code().expect("Shouldn't be called with incorrect context"),
            ));

            if let Some(waker) = &signal.waker {
                waker.wake_by_ref();
            }

            crate::exec::wake(signal.message_id).expect("Failed to wake the message")
        } else {
            crate::debug!("A message has received a reply though it wasn't to receive one, or a processed message has received a reply");
        }
    }

    pub fn waits_for(&self, reply_to: MessageId) -> bool {
        self.signals.contains_key(&reply_to)
    }

    pub fn poll(&mut self, reply_to: MessageId, cx: &mut Context<'_>) -> ReplyPoll {
        match self.signals.remove(&reply_to) {
            None => ReplyPoll::None,
            Some(mut signal @ WakeSignal { payload: None, .. }) => {
                signal.waker = Some(cx.waker().clone());
                self.signals.insert(reply_to, signal);
                ReplyPoll::Pending
            }
            Some(WakeSignal {
                payload: Some(payload),
                ..
            }) => ReplyPoll::Some(payload),
        }
    }
}
