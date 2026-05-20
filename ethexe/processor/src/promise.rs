// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::injected::Promise;
use gprimitives::H256;
use tokio::sync::mpsc::{UnboundedSender, error::SendError};

type SinkEvent = (H256, Promise);

/// Wrapper on top of [tokio::sync::mpsc::UnboundedSender].
/// [BoundPromiseSink] is responsible for sending the promises with
/// MB hash it belongs to.
#[derive(Clone)]
pub struct BoundPromiseSink {
    sender: UnboundedSender<SinkEvent>,
    mb_hash: H256,
}

impl BoundPromiseSink {
    /// Creates new instance of [BoundPromiseSink].
    pub fn new(sender: UnboundedSender<SinkEvent>, mb_hash: H256) -> Self {
        Self { sender, mb_hash }
    }

    /// Sends [Promise] to outer service.
    /// Internally wraps result into `(H256, Promise)`.
    pub fn send(&self, promise: Promise) -> Result<(), SendError<Promise>> {
        let event = (self.mb_hash, promise);
        self.sender.send(event).map_err(|err| SendError(err.0.1))
    }
}
