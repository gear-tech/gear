// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use libp2p::{Multiaddr, PeerId};

use crate::config::TransportConfig;

use std::fmt;

/// Result type alias for the network.
pub type Result<T> = std::result::Result<T, Error>;

/// Error type for the network.
#[derive(thiserror::Error)]
pub enum Error {
    /// Io error
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// The same bootnode (based on address) is registered with two different peer ids.
    #[error(
		"The same bootnode (`{address}`) is registered with two different peer ids: `{first_id}` and `{second_id}`"
	)]
    DuplicateBootnode {
        /// The address of the bootnode.
        address: Multiaddr,
        /// The first peer id that was found for the bootnode.
        first_id: PeerId,
        /// The second peer id that was found for the bootnode.
        second_id: PeerId,
    },
    /// The network addresses are invalid because they don't match the transport.
    #[error(
        "The following addresses are invalid because they don't match the transport: {addresses:?}"
    )]
    AddressesForAnotherTransport {
        /// Transport used.
        transport: TransportConfig,
        /// The invalid addresses.
        addresses: Vec<Multiaddr>,
    },
    /// Peer does not exist.
    #[error("Peer `{0}` does not exist.")]
    PeerDoesntExist(PeerId),
    /// Channel closed.
    #[error("Channel closed")]
    ChannelClosed,
    /// Connection closed.
    #[error("Connection closed")]
    ConnectionClosed,
}

// Make `Debug` use the `Display` implementation.
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
