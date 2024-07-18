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

//! Transport that serves as a common ground for all connections.

use libp2p::{
    core::{muxing::StreamMuxerBox, transport::Boxed},
    dns, identity, quic, PeerId, Transport, TransportExt,
};
use std::sync::Arc;

pub use libp2p::bandwidth::BandwidthSinks;

/// Builds the transport that serves as a common ground for all connections.
///
/// If `memory_only` is true, then only communication within the same process are allowed. Only
/// addresses with the format `/memory/...` are allowed.
///
/// Returns a `BandwidthSinks` object that allows querying the average bandwidth produced by all
/// the connections spawned with this transport.
pub fn build_transport(
    keypair: identity::Keypair,
    _memory_only: bool,
) -> (Boxed<(PeerId, StreamMuxerBox)>, Arc<BandwidthSinks>) {
    // Build the base layer of the transport.
    let main_quic_transport = {
        let quic_config = quic::Config::new(&keypair);
        let quic_trans = quic::tokio::Transport::new(quic_config.clone());
        let dns_init = dns::Transport::system(quic_trans);
        dns_init.expect("err")
    };

    let transport = main_quic_transport.boxed();

    transport.with_bandwidth_logging()
}
