// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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
