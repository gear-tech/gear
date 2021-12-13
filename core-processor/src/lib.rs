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

mod code;
pub mod configs;
mod ext;
mod id;

// pub enum DispatchKind {
//     Init,
//     Handle,
//     HandleReply,
// }

// pub struct Dispatch {
//     message: Message,
//     kind: DispatchKind,
// }

// impl Dispatch {
//     pub fn entry(&self) -> &'static str {
//         use DispatchKind::*;

//         match self.kind {
//             Init => "init",
//             Handle => "handle",
//             HandleReply => "handle_reply",
//         }
//     }
// }

// pub enum DispatchResultKind {
//     Ok,
//     Trap,
//     Wait,
// }

// pub struct DispatchResult {
//     dispatch: Dispatch,
//     kind: DispatchResultKind,
//     gas_burned: u64,
//     outgoing: Vec<Message>,
//     awakening: Vec<MessageId>,
//     page_update: BTreeMap<PageNumber, Vec<u8>>,
// }

// impl DispatchResult {
//     pub fn program_id(&self) -> ProgramId {
//         self.dispatch.message.dest
//     }

//     pub fn message_id(&self) -> MessageId {
//         self.dispatch.message.id()
//     }

//     pub fn outgoing(&self) -> Vec<Message> {
//         self.outgoing.clone()
//     }

//     pub fn awakening(&self) -> Vec<MessageId> {
//         self.awakening.clone()
//     }

//     pub fn gas_burned(&self) -> u64 {
//         self.gas_burned
//     }

//     pub fn gas_left(&self) -> u64 {
//         let mut gas = self.dispatch.message.gas_limit();

//         for outgoing_gas in self.outgoing.iter().map(|m| m.gas_limit()) {
//             gas = gas.saturating_sub(outgoing_gas);
//         }

//         gas
//     }

//     pub fn generate_trap_reply(&self) -> bool {
//         if let Some((_, exit_code)) = self.dispatch.message.reply() {
//             if exit_code != 0 {
//                 return false;
//             }
//         };

//         true
//     }
// }

// pub trait ResourceLimiter {
//     fn can_continue(&self, dispatch: &Dispatch) -> bool;

//     fn dispatch_processed(&mut self, result: &DispatchResult);
// }

// pub trait ProcessorStorage {
//     fn new_message(&mut self, origin: MessageId, message: Message);
//     fn trap_reply(&mut self, origin: MessageId);
//     fn gas_burned(&mut self, origin: MessageId, amount: u64);
//     fn consume_message(&mut self, message_id: MessageId);
//     fn wake_message(&mut self, origin: MessageId, target: MessageId);
//     fn wait_dispatch(&mut self, dispatch: Dispatch);
//     fn queue_dispatches(&mut self, messages: Vec<Dispatch>);
//     fn update_page(&mut self, program_id: ProgramId, page_number: PageNumber, data: Vec<u8>);
// }

// pub enum ProcessEvent {
//     NewMessage {
//         origin: MessageId,
//         message: Message,
//     },
//     TrapReply {
//         origin: MessageId,
//     },
//     GasBurned {
//         origin: MessageId,
//         amount: u64,
//     },
//     MessageConsumed(MessageId),
//     WakeMessage {
//         origin: MessageId,
//         target: MessageId,
//     },
//     WaitDispatch(Dispatch),
//     UpdatePage {
//         program_id: ProgramId,
//         page_number: PageNumber,
//         data: Vec<u8>,
//     },
//     NotProcessed(Vec<Dispatch>),
// }

// pub fn process(
//     resource_limiter: &mut dyn ResourceLimiter,
//     dispatches: impl IntoIterator<Item = impl Into<Dispatch>>,
//     runner: impl Fn(Dispatch) -> DispatchResult,
// ) -> Vec<ProcessEvent> {
//     let mut dispatches = dispatches.into_iter();
//     let mut not_processed = vec![];
//     let mut events = vec![];

//     while let Some(next_dispatch) = dispatches.next() {
//         let next_dispatch = next_dispatch.into();

//         if !resource_limiter.can_continue(&next_dispatch) {
//             not_processed.push(next_dispatch);
//             break;
//         }

//         let dispatch_result = runner(next_dispatch);
//         resource_limiter.dispatch_processed(&dispatch_result);

//         let program_id = dispatch_result.program_id();

//         events.push(ProcessEvent::GasBurned {
//             origin: dispatch_result.message_id(),
//             amount: dispatch_result.gas_burned(),
//         });

//         for message in dispatch_result.outgoing() {
//             events.push(ProcessEvent::NewMessage {
//                 origin: dispatch_result.message_id(),
//                 message,
//             })
//         }

//         for target in dispatch_result.awakening() {
//             events.push(ProcessEvent::WakeMessage {
//                 origin: dispatch_result.message_id(),
//                 target,
//             })
//         }

//         match dispatch_result.kind {
//             DispatchResultKind::Ok => {
//                 events.push(ProcessEvent::MessageConsumed(dispatch_result.message_id()))
//             }
//             DispatchResultKind::Wait => {
//                 events.push(ProcessEvent::WaitDispatch(dispatch_result.dispatch))
//             }
//             DispatchResultKind::Trap => {
//                 if dispatch_result.generate_trap_reply() {
//                     events.push(ProcessEvent::TrapReply {
//                         origin: dispatch_result.message_id(),
//                     });
//                 }

//                 events.push(ProcessEvent::MessageConsumed(dispatch_result.message_id()));
//             }
//         };

//         for (page_number, data) in dispatch_result.page_update {
//             events.push(ProcessEvent::UpdatePage {
//                 program_id,
//                 page_number,
//                 data,
//             })
//         }
//     }

//     let dispatches: Vec<Dispatch> = dispatches.into_iter().map(Into::into).collect();

//     not_processed.extend(dispatches);

//     events.push(ProcessEvent::NotProcessed(not_processed));

//     events
// }

// pub fn process_events(
//     events: impl IntoIterator<Item = ProcessEvent>,
//     storage: &mut dyn ProcessorStorage,
// ) {
//     use ProcessEvent::*;

//     for event in events.into_iter() {
//         match event {
//             NewMessage { origin, message } => storage.new_message(origin, message),
//             GasBurned { origin, amount } => storage.gas_burned(origin, amount),
//             WakeMessage { origin, target } => storage.wake_message(origin, target),
//             TrapReply { origin } => storage.trap_reply(origin),
//             UpdatePage {
//                 program_id,
//                 page_number,
//                 data,
//             } => storage.update_page(program_id, page_number, data),
//             MessageConsumed(message_id) => storage.consume_message(message_id),
//             WaitDispatch(dispatch) => storage.wait_dispatch(dispatch),
//             NotProcessed(dispatches) => storage.queue_dispatches(dispatches),
//         }
//     }
// }
