// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

//! Gear message processor.
#![no_std]
//#![warn(missing_docs)]
#![cfg_attr(feature = "strict", deny(warnings))]
#![doc(html_logo_url = "https://docs.gear.rs/logo.svg")]

extern crate alloc;

use alloc::{collections::BTreeMap, vec, vec::Vec};
use gear_core::{
    memory::PageNumber,
    message::{Message, MessageId},
    program::ProgramId,
};

pub trait ResourceLimiter {
    fn dispatch_processed(&mut self, msg: &DispatchResult);

    fn can_countinue(&self, msg: &Dispatch) -> bool;
}

pub trait Storage {
    fn new_message(&mut self, origin: MessageId, message: Message);
    fn gas_burned(&mut self, origin: MessageId, amount: u64);
    fn consume_message(&mut self, message_id: MessageId);
    fn wait_dispatch(&mut self, dispatch: Dispatch);
    fn queue_dispatches(&mut self, messages: Vec<Dispatch>);
    fn update_page(&mut self, program_id: ProgramId, page_number: PageNumber, data: Vec<u8>);
}

pub enum DispatchKind {
    Init,
    Handle,
}

pub struct Dispatch {
    message: Message,
    kind: DispatchKind,
}

pub enum DispatchResultKind {
    Ok,
    Trap,
    Wait,
}

pub struct DispatchResult {
    dispatch: Dispatch,
    kind: DispatchResultKind,
    gas_burned: u64,
    outgoing: Vec<Message>,
    page_update: BTreeMap<PageNumber, Vec<u8>>,
}

impl DispatchResult {
    pub fn gas_burned(&self) -> u64 {
        self.gas_burned
    }

    pub fn gas_left(&self) -> u64 {
        let mut gas = self.dispatch.message.gas_limit();
        for outgoing_gas in self.outgoing.iter().map(|m| m.gas_limit) {
            gas = gas.saturating_sub(outgoing_gas);
        }
        gas
    }

    pub fn generate_trap_reply(&self) -> Message {
        unimplemented!()
    }

    pub fn message_id(&self) -> MessageId {
        self.dispatch.message.id()
    }

    pub fn program_id(&self) -> ProgramId {
        self.dispatch.message.dest
    }
}

pub enum ProcessEvent {
    NewMessage {
        origin: MessageId,
        message: Message,
    },
    GasBurned {
        message_id: MessageId,
        amount: u64,
    },
    MessageConsumed(MessageId),
    WaitDispatch(Dispatch),
    PageUpdate {
        program_id: ProgramId,
        page_number: PageNumber,
        data: Vec<u8>,
    },
    NotProcessed(Vec<Dispatch>),
}

pub fn process(
    resource_limiter: &mut dyn ResourceLimiter,
    dispatches: impl IntoIterator<Item = Dispatch>,
    runner: impl Fn(Dispatch) -> DispatchResult,
) -> Vec<ProcessEvent> {
    let mut dispatches = dispatches.into_iter();
    let mut not_processed = vec![];
    let mut events = vec![];

    while let Some(next_dispatch) = dispatches.next() {
        if !resource_limiter.can_countinue(&next_dispatch) {
            not_processed.push(next_dispatch);
            break;
        }

        let dispatch_result = runner(next_dispatch);
        resource_limiter.dispatch_processed(&dispatch_result);

        let program_id = dispatch_result.program_id();

        events.push(ProcessEvent::GasBurned {
            message_id: dispatch_result.message_id(),
            amount: dispatch_result.gas_burned(),
        });

        match dispatch_result.kind {
            DispatchResultKind::Ok => {
                events.push(ProcessEvent::MessageConsumed(dispatch_result.message_id()));
            }
            DispatchResultKind::Wait => {
                events.push(ProcessEvent::WaitDispatch(dispatch_result.dispatch));
            }
            DispatchResultKind::Trap => {
                let trap_reply = dispatch_result.generate_trap_reply();
                events.push(ProcessEvent::NewMessage {
                    origin: dispatch_result.message_id(),
                    message: trap_reply,
                });
                events.push(ProcessEvent::MessageConsumed(dispatch_result.message_id()));
            }
        };

        for (page_number, data) in dispatch_result.page_update {
            events.push(ProcessEvent::PageUpdate {
                program_id,
                page_number,
                data,
            })
        }
    }

    not_processed.extend(dispatches);

    events.push(ProcessEvent::NotProcessed(not_processed));

    events
}

pub fn process_events(events: impl IntoIterator<Item = ProcessEvent>, storage: &mut dyn Storage) {
    use ProcessEvent::*;
    for event in events.into_iter() {
        match event {
            NewMessage { origin, message } => storage.new_message(origin, message),
            GasBurned { message_id, amount } => storage.gas_burned(message_id, amount),
            MessageConsumed(message_id) => storage.consume_message(message_id),
            WaitDispatch(dispatch) => storage.wait_dispatch(dispatch),
            NotProcessed(dispatches) => storage.queue_dispatches(dispatches),
        }
    }
}
