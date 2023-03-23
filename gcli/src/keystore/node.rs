// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Node key
#![cfg(feature = "node-key")]
use crate::result::{Error, Result};
use libp2p::{identity::Keypair, PeerId};

/// Generate node key
pub fn generate() -> (Keypair, PeerId) {
    let pair = Keypair::generate_ed25519();
    let public = pair.public();

    (pair, PeerId::from_public_key(&public))
}

/// Get inspect of node key from secret
pub fn inspect(mut data: Vec<u8>) -> Result<(Keypair, PeerId)> {
    let pair = Keypair::ed25519_from_bytes(&mut data).map_err(|_| Error::BadNodeKey)?;
    let public = pair.public();

    Ok((pair, PeerId::from_public_key(&public)))
}
