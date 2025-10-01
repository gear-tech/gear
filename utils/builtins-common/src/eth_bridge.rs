// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

pub use gbuiltin_eth_bridge::{Request, Response};

use gprimitives::{H160, H256, U256};

/// The function computes bridging call hash used by `pallet_gear_eth_bridge`
/// to be later transmitted to Ethereum.
pub fn bridge_call_hash(
    nonce: U256,
    source: H256,
    destination: H160,
    payload: &[u8],
    hashing_fn: impl Fn(&[u8]) -> H256,
) -> H256 {
    let mut nonce_bytes = [0; 32];
    nonce.to_little_endian(&mut nonce_bytes);

    let bytes = [
        nonce_bytes.as_ref(),
        source.as_ref(),
        destination.as_bytes(),
        payload,
    ]
    .concat();

    hashing_fn(&bytes)
}

pub fn keccak256_hash(data: &[u8]) -> H256 {
    use sha3::Digest;
    let hash: [u8; 32] = sha3::Keccak256::digest(data).into();

    hash.into()
}
