// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
    nonce.to_big_endian(&mut nonce_bytes);

    let bytes = [
        nonce_bytes.as_ref(),
        source.as_bytes(),
        destination.as_bytes(),
        payload,
    ]
    .concat();

    hashing_fn(&bytes)
}

/// Computes the Keccak-256 (SHA-3) hash of the input data.
pub fn keccak256_hash(data: &[u8]) -> H256 {
    use sha3::Digest;
    let hash: [u8; 32] = sha3::Keccak256::digest(data).into();

    hash.into()
}
