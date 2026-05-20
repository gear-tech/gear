// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::{Announce, HashOf, injected::Promise};
use tokio::sync::mpsc::{UnboundedSender, error::SendError};

type SinkEvent = (HashOf<Announce>, Promise);

/// Wrapper on top of [tokio::sync::mpsc::UnboundedSender].
/// [BoundPromiseSink] is responsible for sending the promises with
/// announce hash it belongs to.
#[derive(Clone)]
pub struct BoundPromiseSink {
    sender: UnboundedSender<SinkEvent>,
    announce_hash: HashOf<Announce>,
}

impl BoundPromiseSink {
    /// Creates new instance of [BoundPromiseSink].
    pub fn new(sender: UnboundedSender<SinkEvent>, announce_hash: HashOf<Announce>) -> Self {
        Self {
            sender,
            announce_hash,
        }
    }

    /// Sends [Promise] to outer service.
    /// Internally wraps result into `(HashOf<Announce>, Promise)`.
    pub fn send(&self, promise: Promise) -> Result<(), SendError<Promise>> {
        let event = (self.announce_hash, promise);
        self.sender.send(event).map_err(|err| SendError(err.0.1))
    }
}
