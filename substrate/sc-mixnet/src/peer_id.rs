// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use mixnet::core::PeerId as CorePeerId;
use sc_network_types::PeerId;

/// Convert a libp2p [`PeerId`] into a mixnet core [`PeerId`](CorePeerId).
///
/// This will succeed only if `peer_id` is an Ed25519 public key ("hashed" using the identity
/// hasher). Returns `None` on failure.
pub fn to_core_peer_id(peer_id: &PeerId) -> Option<CorePeerId> {
    peer_id.into_ed25519()
}

/// Convert a mixnet core [`PeerId`](CorePeerId) into a libp2p [`PeerId`].
///
/// This will succeed only if `peer_id` represents a point on the Ed25519 curve. Returns `None` on
/// failure.
pub fn from_core_peer_id(core_peer_id: &CorePeerId) -> Option<PeerId> {
    PeerId::from_ed25519(core_peer_id)
}
