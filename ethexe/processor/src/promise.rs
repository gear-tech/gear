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

//! [`BoundPromiseSink`] — a producer-side wrapper around
//! [`tokio::sync::mpsc::UnboundedSender`] that automatically tags every
//! promise it sends with the MB hash the executor is currently working
//! on. The receiver side gets a stream of `(mb_hash, promise)` and
//! never has to thread the binding through the processor itself.

use ethexe_common::injected::Promise;
use gprimitives::H256;
use tokio::sync::mpsc::{UnboundedSender, error::SendError};

type SinkEvent = (H256, Promise);

/// Wrapper on top of [`UnboundedSender`] that pre-binds every send to
/// a single MB hash. Cloning the sink shares the same channel and
/// binding — used by the processor's worker threads, which all emit
/// promises for the same MB.
#[derive(Clone)]
pub struct BoundPromiseSink {
    sender: UnboundedSender<SinkEvent>,
    mb_hash: H256,
}

impl BoundPromiseSink {
    /// Create a new sink bound to `mb_hash`.
    pub fn new(sender: UnboundedSender<SinkEvent>, mb_hash: H256) -> Self {
        Self { sender, mb_hash }
    }

    /// Send a `Promise`, automatically tagging it with the bound MB
    /// hash. The error path returns the original promise so callers
    /// can inspect it without re-stripping the tag.
    pub fn send(&self, promise: Promise) -> Result<(), SendError<Promise>> {
        let event = (self.mb_hash, promise);
        self.sender.send(event).map_err(|err| SendError(err.0.1))
    }
}
