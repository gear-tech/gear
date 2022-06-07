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

use crate::{
    ids::{MessageId, ProgramId},
    message::{
        Dispatch, HandleMessage, HandlePacket, IncomingMessage, InitMessage, InitPacket, Payload,
        ReplyMessage, ReplyPacket,
    },
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use codec::{Decode, Encode};
use gear_core_errors::MessageError as Error;
use scale_info::TypeInfo;

pub const OUTGOING_LIMIT: u32 = 1024;

/// Context settings.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct ContextSettings {
    sending_fee: u64,
    outgoing_limit: u32,
}

impl ContextSettings {
    /// Create new ContextSettings.
    pub fn new(sending_fee: u64, outgoing_limit: u32) -> Self {
        Self {
            sending_fee,
            outgoing_limit,
        }
    }
}

impl Default for ContextSettings {
    fn default() -> Self {
        Self::new(0, OUTGOING_LIMIT)
    }
}

/// Context outcome.
///
/// Contains all sendings and wakes that should be done after execution.
#[derive(Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo, Clone)]
pub struct ContextOutcome {
    init: Vec<InitMessage>,
    handle: Vec<HandleMessage>,
    reply: Option<ReplyMessage>,
    awakening: Vec<MessageId>,
    // Additional information section
    program_id: ProgramId,
    source: ProgramId,
    origin_msg_id: MessageId,
}

impl ContextOutcome {
    /// Create new ContextOutcome.
    fn new(program_id: ProgramId, source: ProgramId, origin_msg_id: MessageId) -> Self {
        Self {
            program_id,
            source,
            origin_msg_id,
            ..Default::default()
        }
    }

    /// Destructs outcome after execution and returns provided dispatches and awaken message ids.
    pub fn drain(self) -> (Vec<Dispatch>, Vec<MessageId>) {
        let mut dispatches = Vec::new();

        for msg in self.init.into_iter() {
            dispatches.push(msg.into_dispatch(self.program_id));
        }

        for msg in self.handle.into_iter() {
            dispatches.push(msg.into_dispatch(self.program_id));
        }

        if let Some(msg) = self.reply {
            dispatches.push(msg.into_dispatch(self.program_id, self.source, self.origin_msg_id));
        };

        (dispatches, self.awakening)
    }
}

/// Store of previous message execution context.
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct ContextStore {
    outgoing: BTreeMap<u32, Option<Payload>>,
    reply: Option<Payload>,
    initialized: BTreeSet<ProgramId>,
    awaken: BTreeSet<MessageId>,
    reply_sent: bool,
}

/// Context of currently processing incoming message.
#[derive(Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo, Clone)]
pub struct MessageContext {
    current: IncomingMessage,
    outcome: ContextOutcome,
    store: ContextStore,
    settings: ContextSettings,
}

impl MessageContext {
    /// Create new MessageContext with default ContextSettings.
    pub fn new(
        message: IncomingMessage,
        program_id: ProgramId,
        store: Option<ContextStore>,
    ) -> Self {
        Self::new_with_settings(message, program_id, store, Default::default())
    }

    /// Create new MessageContext with given ContextSettings.
    pub fn new_with_settings(
        message: IncomingMessage,
        program_id: ProgramId,
        store: Option<ContextStore>,
        settings: ContextSettings,
    ) -> Self {
        Self {
            outcome: ContextOutcome::new(program_id, message.source(), message.id()),
            current: message,
            store: store.unwrap_or_default(),
            settings,
        }
    }

    /// Send a new program initialization message.
    ///
    /// Generates a new message from provided data packet.
    /// Returns message id and generated program id.
    pub fn init_program(&mut self, packet: InitPacket) -> Result<(ProgramId, MessageId), Error> {
        let program_id = packet.destination();

        if self.store.initialized.contains(&program_id) {
            return Err(Error::DuplicateInit);
        }

        let last = self.store.outgoing.len() as u32;

        if last >= self.settings.outgoing_limit {
            return Err(Error::LimitExceeded);
        }

        let message_id = MessageId::generate_outgoing(self.current.id(), last);
        let message = InitMessage::from_packet(message_id, packet);

        self.store.outgoing.insert(last, None);
        self.store.initialized.insert(program_id);
        self.outcome.init.push(message);

        Ok((program_id, message_id))
    }

    /// Send a new program initialization message.
    ///
    /// Generates message from provided data packet and stored by handle payload.
    /// Returns message id.
    pub fn send_commit(&mut self, handle: u32, packet: HandlePacket) -> Result<MessageId, Error> {
        if let Some(payload) = self.store.outgoing.get_mut(&handle) {
            if let Some(data) = payload.take() {
                let packet = {
                    let mut packet = packet;
                    packet.prepend(data);
                    packet
                };

                let message_id = MessageId::generate_outgoing(self.current.id(), handle);
                let message = HandleMessage::from_packet(message_id, packet);

                self.outcome.handle.push(message);

                Ok(message_id)
            } else {
                Err(Error::LateAccess)
            }
        } else {
            Err(Error::OutOfBounds)
        }
    }

    /// Provide space for storing payload for future message creation.
    ///
    /// Returns it's handle.
    pub fn send_init(&mut self) -> Result<u32, Error> {
        let last = self.store.outgoing.len() as u32;

        if last < self.settings.outgoing_limit {
            self.store.outgoing.insert(last, Some(Default::default()));

            Ok(last)
        } else {
            Err(Error::LimitExceeded)
        }
    }

    /// Pushes payload into stored payload by handle.
    pub fn send_push(&mut self, handle: u32, buffer: &[u8]) -> Result<(), Error> {
        match self.store.outgoing.get_mut(&handle) {
            Some(Some(data)) => {
                data.extend_from_slice(buffer);
                Ok(())
            }
            Some(None) => Err(Error::LateAccess),
            None => Err(Error::OutOfBounds),
        }
    }

    /// Send reply message.
    ///
    /// Generates reply from provided data packet and stored reply payload.
    /// Returns message id.
    pub fn reply_commit(&mut self, packet: ReplyPacket) -> Result<MessageId, Error> {
        if !self.store.reply_sent {
            let data = self.store.reply.take().unwrap_or_default();

            let packet = {
                let mut packet = packet;
                packet.prepend(data);
                packet
            };

            let message_id = MessageId::generate_reply(self.current.id(), packet.exit_code());
            let message = ReplyMessage::from_packet(message_id, packet);

            self.outcome.reply = Some(message);
            self.store.reply_sent = true;

            Ok(message_id)
        } else {
            Err(Error::DuplicateReply)
        }
    }

    /// Pushes payload into stored reply payload.
    pub fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Error> {
        if !self.store.reply_sent {
            let data = self.store.reply.get_or_insert_with(Default::default);
            data.extend_from_slice(buffer);

            Ok(())
        } else {
            Err(Error::LateAccess)
        }
    }

    /// Wake message by it's message id.
    pub fn wake(&mut self, waker_id: MessageId) -> Result<(), Error> {
        if self.store.awaken.insert(waker_id) {
            self.outcome.awakening.push(waker_id);

            Ok(())
        } else {
            Err(Error::DuplicateWaking)
        }
    }

    /// Current processing incoming message.
    pub fn current(&self) -> &IncomingMessage {
        &self.current
    }

    /// Current program's id.
    pub fn program_id(&self) -> ProgramId {
        self.outcome.program_id
    }

    /// Destructs context after execution and returns provided outcome and store.
    pub fn drain(self) -> (ContextOutcome, ContextStore) {
        let Self { outcome, store, .. } = self;

        (outcome, store)
    }
}
