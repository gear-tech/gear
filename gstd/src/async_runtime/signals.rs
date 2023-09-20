// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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
use gear_core_errors::ReplyCode;
use hashbrown::HashMap;

pub type Payload = Vec<u8>;

#[derive(Debug)]
pub(crate) enum ReplyPoll {
    None,
    Pending,
    Some((Payload, ReplyCode)),
}

struct WakeSignal {
    message_id: MessageId,
    payload: Option<(Payload, ReplyCode)>,
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
        crate::log!("register_signal({waiting_reply_to:.2?})");

        let message_id = crate::msg::id();

        crate::log!(
            "register_signal({waiting_reply_to:.2?}): inserting signal for {message_id:.2?}"
        );

        self.signals.insert(
            waiting_reply_to,
            WakeSignal {
                message_id,
                payload: None,
                waker: None,
            },
        );
    }

    pub fn record_reply(&mut self) {
        let reply_to = crate::msg::reply_to().expect("Shouldn't be called with incorrect context");

        crate::log!("record_reply({reply_to:.2?})");

        let Some(signal) = self.signals.get_mut(&reply_to) else {
            crate::log!("record_reply({reply_to:.2?}): signal is NOT FOUND");
            return;
        };

        crate::log!("record_reply({reply_to:.2?}): signal is FOUND");

        crate::log!("record_reply({reply_to:.2?}): querying data");

        let payload = crate::msg::load_bytes().expect("Failed to load bytes");
        let reply_code =
            crate::msg::reply_code().expect("Shouldn't be called with incorrect context");

        crate::log!("record_reply({reply_to:.2?}): setting data to signal");

        signal.payload = Some((payload, reply_code));

        crate::log!("record_reply({reply_to:.2?}): trying to touch waker");

        if let Some(waker) = &signal.waker {
            crate::log!("record_reply({reply_to:.2?}): waker is FOUND");
            crate::log!("record_reply({reply_to:.2?}): waking waker by ref");

            waker.wake_by_ref();
        } else {
            crate::log!("record_reply({reply_to:.2?}): waker is NOT FOUND");
        }

        crate::log!(
            "record_reply({reply_to:.2?}): waking signal message {:.2?}",
            signal.message_id
        );

        crate::exec::wake(signal.message_id).expect("Failed to wake the message")
    }

    pub fn waits_for(&self, reply_to: MessageId) -> bool {
        crate::log!("waits_for({reply_to:.2?})");

        let res = self.signals.contains_key(&reply_to);

        crate::log!("waits_for({reply_to:.2?}): {res}");

        res
    }

    pub fn poll(&mut self, reply_to: MessageId, cx: &mut Context<'_>) -> ReplyPoll {
        crate::log!("signals_poll({reply_to:.2?})");

        crate::log!("signals_poll({reply_to:.2?}): removing signal");

        match self.signals.remove(&reply_to) {
            None => {
                crate::log!("signals_poll({reply_to:.2?}): signal is NOT FOUND");

                ReplyPoll::None
            }
            Some(mut signal @ WakeSignal { payload: None, .. }) => {
                crate::log!("signals_poll({reply_to:.2?}): signal is FOUND but PENDING");

                crate::log!("signals_poll({reply_to:.2?}): updating waker");

                signal.waker = Some(cx.waker().clone());

                crate::log!("signals_poll({reply_to:.2?}): inserting signal");

                self.signals.insert(reply_to, signal);

                ReplyPoll::Pending
            }
            Some(WakeSignal {
                payload: Some(payload),
                ..
            }) => {
                crate::log!("signals_poll({reply_to:.2?}): signal is FOUND and READY");

                ReplyPoll::Some(payload)
            }
        }
    }
}
