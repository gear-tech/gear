// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::{Config, Error, MessageNonce};
use binary_merkle_tree::MerkleProof;
use common::Origin;
use frame_support::{ensure, traits::Get};
use gprimitives::{ActorId, H160, H256, U256};
use pallet_gear_eth_bridge_primitives::EthMessage;
use sp_runtime::traits::{Hash, Keccak256};
use sp_std::vec::Vec;

/// Extension trait for [`EthMessage`] that provides additional functionality.
pub trait EthMessageExt: Sized {
    fn try_new<T: Config>(
        source: ActorId,
        destination: H160,
        payload: Vec<u8>,
    ) -> Result<Self, Error<T>>;

    fn hash(&self) -> H256;
}

impl EthMessageExt for EthMessage {
    /// Creates a new [`EthMessage`] with the given parameters.
    fn try_new<T: Config>(
        source: ActorId,
        destination: H160,
        payload: Vec<u8>,
    ) -> Result<Self, Error<T>> {
        ensure!(
            payload.len() <= T::MaxPayloadSize::get() as usize,
            Error::<T>::MaxPayloadSizeExceeded
        );

        let nonce = MessageNonce::<T>::mutate(|nonce| {
            let res = *nonce;
            *nonce = nonce.saturating_add(U256::one());
            res
        });

        Ok(unsafe { Self::new_unchecked(nonce, source, destination, payload) })
    }

    /// Returns hash of the message using `Keccak256` hasher.
    fn hash(&self) -> H256 {
        let mut nonce = [0; 32];
        self.nonce().to_little_endian(&mut nonce);

        let bytes = [
            nonce.as_ref(),
            self.source().as_bytes(),
            self.destination().as_bytes(),
            self.payload(),
        ]
        .concat();

        Keccak256::hash(&bytes).cast()
    }
}
