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

use crate::node::result::Error as NodeError;
use anyhow::Error as AError;
use std::{io::Error as IOError, result::Result as StdResult};
use subxt::error::Error as SubxtError;

/// `Result` type with a predefined error type ([`Error`]).
pub type Result<T, E = Error> = StdResult<T, E>;

/// Common error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A wrapper around [`anyhow::Error`].
    #[error(transparent)]
    Anyhow(#[from] AError),
    /// A wrapper around [`gsdk::Error`].
    #[error(transparent)]
    GearSDK(#[from] gsdk::result::Error),
    /// Occurs when attempting to iterate events without a subscription.
    #[error("An attempt to iter events without subscription")]
    EventsSubscriptionNotFound,
    /// Occurs when events are stopped. Unreachable.
    #[error("Events stopped (unreachable")]
    EventsStopped,
    /// A wrapper around [`subxt::error::Error`].
    #[error(transparent)]
    Subxt(#[from] SubxtError),
    /// Occurs when an event of the expected type cannot be found.
    #[error("Expected event wasn't found")]
    EventNotFound,
    /// A wrapper around [`std::io::Error`].
    #[error(transparent)]
    IO(#[from] IOError),
    /// Occurs when trying to upload a Wasm binary with the wrong file extension
    /// (other than `.wasm`).
    #[error("An attempt to upload invalid binary")]
    WrongBinaryExtension,
    /// Occurs when the balance reaches its maximum value.
    #[error("Funds overcame u128::MAX")]
    BalanceOverflow,
    /// Occurs when a block identified by the specified hash cannot be found.
    #[error("Block data not found")]
    BlockDataNotFound,
    /// Occurs when a hash of a block identified by the specified hash cannot be
    /// found.
    #[error("Block hash not found")]
    BlockHashNotFound,
    /// Occurs when the batch hasn't been fully processed.
    #[error("Some of extrinsics wasn't processed within the batch")]
    IncompleteBatchResult(usize, usize),
    /// Occurs when a block cannot be found within the specified depth.
    #[error("Max depth reached, but queried block wasn't found")]
    MaxDepthReached,
    /// Occurs when an event cannot be found in pre-queried events.
    #[error("Event not found in pre-queried events")]
    EventNotFoundInIterator,
    /// Occurs when a storage entry with a specified address cannot be found.
    #[error("Storage not found.")]
    StorageNotFound,
    /// Occurs when a timestamp record cannot be found in the storage.
    #[error("Timestamp not found in storage.")]
    TimestampNotFound,
    /// A wrapper around [`parity_scale_codec::Error`].
    #[error(transparent)]
    Codec(#[from] parity_scale_codec::Error),
    /// Occurs when decoding hex string failed.
    #[error(transparent)]
    Hex(#[from] hex::FromHexError),
    /// Occurs when being migrated program already exists in destination node.
    #[error("Program {0} already exists")]
    ProgramAlreadyExists(String),
    /// Occurs when node spawining failed.
    #[error(transparent)]
    Node(#[from] NodeError),
}
