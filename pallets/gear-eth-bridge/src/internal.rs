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

use crate::{
    AuthoritySetHash, ClearTimer, Config, Error, Event, Initialized, MessageNonce, Pallet, Paused,
    Queue, QueueCapacityOf, QueueChanged, QueueMerkleRoot,
};
use frame_support::{Blake2_256, StorageHasher, ensure, traits::Get, weights::Weight};
use gprimitives::{ActorId, H160, H256, U256};
use pallet_gear_eth_bridge_primitives::EthMessage;
use sp_runtime::{
    RuntimeAppPublic as _,
    traits::{Hash, Keccak256},
};
use sp_std::vec::Vec;

impl<T: Config> Pallet<T> {
    /// Updates the authority set hash in storage and emits an event.
    pub(super) fn update_authority_set_hash<'a, I>(validators: I)
    where
        I: Iterator<Item = (&'a T::AccountId, sp_consensus_grandpa::AuthorityId)>,
    {
        log::debug!("Updating the authority set hash");

        // Collecting all keys into `Vec<u8>`.
        let keys_bytes = validators
            .flat_map(|(_, key)| key.to_raw_vec())
            .collect::<Vec<_>>();

        // Hashing keys bytes with `Blake2`.
        let grandpa_set_hash = Blake2_256::hash(&keys_bytes).into();

        // Setting new grandpa set hash into storage.
        AuthoritySetHash::<T>::put(grandpa_set_hash);

        // Depositing event about update in the set.
        Self::deposit_event(Event::<T>::AuthoritySetHashChanged(grandpa_set_hash));
    }

    /// Updates the queue merkle root in storage and emits an event if it has changed.
    pub(super) fn update_queue_merkle_root() {
        log::debug!("Updating the queue merkle root");

        // Querying actual queue.
        let queue = Queue::<T>::get();

        let merkle_root = if queue.is_empty() {
            // Empty queue should always result in zero hash.
            H256::zero()
        } else {
            // Actual hash recalculation.
            binary_merkle_tree::merkle_root::<Keccak256, _>(queue)
        };

        // Checking if we should update the merkle root.
        let changed = QueueMerkleRoot::<T>::get() != Some(merkle_root);

        if changed {
            // Updating queue merkle root in storage.
            QueueMerkleRoot::<T>::put(merkle_root);

            // Depositing event about queue root being updated.
            Self::deposit_event(Event::<T>::QueueMerkleRootChanged(merkle_root));
        }
    }

    /// Clears the bridge state: removes timer, authority set hash, message queue and emits event.
    pub(super) fn clear_bridge() -> Weight {
        log::debug!("Clearing the bridge state");

        let mut weight = Weight::zero();
        let db_weight = T::DbWeight::get();

        // Removing timer for the next session hook.
        ClearTimer::<T>::kill();
        weight = weight.saturating_add(db_weight.writes(1));

        // Removing grandpa set hash from storage.
        AuthoritySetHash::<T>::kill();
        Self::deposit_event(Event::<T>::AuthoritySetHashReset);
        weight = weight.saturating_add(db_weight.writes(2));

        // Resetting queue.
        let reset_weight = Self::reset_queue();
        weight = weight.saturating_add(reset_weight);

        weight
    }

    /// Resets the message queue and its merkle root: makes 2 storage writes.
    pub(super) fn reset_queue() -> Weight {
        log::debug!("Resetting the message queue");

        // Removing queued messages from storage.
        Queue::<T>::kill();

        // Setting zero queue root, keeping invariant of this key existence.
        QueueMerkleRoot::<T>::put(H256::zero());

        // Depositing event about queue root being reset.
        Self::deposit_event(Event::<T>::QueueMerkleRootReset);

        T::DbWeight::get().writes(3)
    }

    pub(crate) fn queue_message(
        source: ActorId,
        destination: H160,
        payload: Vec<u8>,
        is_governance_origin: bool,
    ) -> Result<(U256, H256), Error<T>> {
        // Ensuring that pallet is initialized.
        ensure!(
            Initialized::<T>::get(),
            Error::<T>::BridgeIsNotYetInitialized
        );

        // Ensuring that pallet isn't paused.
        ensure!(!Paused::<T>::get(), Error::<T>::BridgeIsPaused);

        // Creating new message from given data.
        //
        // Inside goes query and bump of nonce,
        // as well as checking payload size.
        let message = EthMessage::try_new(source, destination, payload)?;

        // Appending hash of the message into the queue,
        // checks whether the queue capacity is exceeded,
        // or skips the check when the origin is governance.
        let hash = Queue::<T>::mutate(|v| {
            (is_governance_origin || v.len() < QueueCapacityOf::<T>::get() as usize)
                .then(|| {
                    let hash = message.hash();
                    v.push(hash);
                    hash
                })
                .ok_or(Error::<T>::QueueCapacityExceeded)
        })
        .inspect_err(|_| {
            // In case of error, reverting increase of `MessageNonce` performed
            // in message creation to keep builtin interactions transactional.
            MessageNonce::<T>::mutate_exists(|nonce| {
                *nonce = nonce
                    .and_then(|inner| inner.checked_sub(U256::one()).filter(|new| !new.is_zero()));
            });
        })?;

        // Marking queue as changed, so root will be updated later.
        QueueChanged::<T>::put(true);

        // Extracting nonce to return.
        let nonce = message.nonce();

        // Depositing event about message being queued for bridging.
        Self::deposit_event(Event::<T>::MessageQueued { message, hash });

        Ok((nonce, hash))
    }
}

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

        Keccak256::hash(&bytes)
    }
}
