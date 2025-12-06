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

//! Validator-specific networking logic that verifies signed messages
//! against on-chain state.

use crate::{
    db_sync::PeerId,
    gossipsub::MessageAcceptance,
    peer_score,
    validator::{ValidatorDatabase, list::ValidatorListSnapshot},
};
use ethexe_common::{
    Address,
    network::{SignedValidatorMessage, VerifiedValidatorMessage},
};
use gprimitives::H256;
use lru::LruCache;
use std::{cmp::Ordering, collections::VecDeque, mem, num::NonZeroUsize, sync::Arc};

const MAX_CACHED_PEERS: NonZeroUsize = NonZeroUsize::new(50).unwrap();
const MAX_CACHED_MESSAGES_PER_PEER: NonZeroUsize = NonZeroUsize::new(20).unwrap();

// used only in assertion
#[allow(dead_code)]
const TOTAL_CACHED_MESSAGES: usize = 1024;
const _: () =
    assert!(MAX_CACHED_PEERS.get() * MAX_CACHED_MESSAGES_PER_PEER.get() <= TOTAL_CACHED_MESSAGES);

type CachedMessages = LruCache<PeerId, LruCache<VerifiedValidatorMessage, ()>>;

#[derive(Debug, Eq, PartialEq, derive_more::Display)]
enum VerificationError {
    #[display("unknown block {block}")]
    UnknownBlock { block: H256 },
    #[display("too old era: expected {expected_era}, got {received_era}")]
    TooOldEra {
        expected_era: u64,
        received_era: u64,
    },
    #[display("old era: expected {expected_era}, got {received_era}")]
    OldEra {
        expected_era: u64,
        received_era: u64,
    },
    #[display("too new era: expected {expected_era}, got {received_era}")]
    TooNewEra {
        expected_era: u64,
        received_era: u64,
    },
    #[display("new era: expected {expected_era}, got {received_era}")]
    NewEra {
        expected_era: u64,
        received_era: u64,
    },
    #[display("address {address} is not validator")]
    AddressIsNotValidator { address: Address },
}

/// Tracks validator-signed messages and admits each one once the on-chain
/// context confirms it is timely and originates from a legitimate validator.
///
/// Legitimacy is checked via the `block` attached to
/// [`ValidatorMessage`](ethexe_common::network::ValidatorMessage) and the
/// validator-signed payload it carries. The hinted era must match the current
/// chain head; eras N-1, N+2, N+3, and so on are dropped when the node is at era N.
/// Messages from era N+1 are rechecked after the next validator set arrives.
pub struct ValidatorTopic {
    cached_messages: CachedMessages,
    verified_messages: VecDeque<VerifiedValidatorMessage>,
    db: Box<dyn ValidatorDatabase>,
    peer_score: peer_score::Handle,
    snapshot: Arc<ValidatorListSnapshot>,
}

impl ValidatorTopic {
    pub fn new(
        db: Box<dyn ValidatorDatabase>,
        peer_score: peer_score::Handle,
        snapshot: Arc<ValidatorListSnapshot>,
    ) -> Self {
        Self {
            cached_messages: LruCache::new(MAX_CACHED_PEERS),
            verified_messages: VecDeque::new(),
            db,
            peer_score,
            snapshot,
        }
    }

    fn inner_verify(&self, message: &VerifiedValidatorMessage) -> Result<(), VerificationError> {
        let chain_head_era = self.snapshot.current_era_index();

        let block = message.block();
        let address = message.address();

        if !self.snapshot.contains_any_validator(address) {
            return Err(VerificationError::AddressIsNotValidator { address });
        }

        let Some(block_header) = self.db.block_header(block) else {
            return Err(VerificationError::UnknownBlock { block });
        };
        let block_era = self.snapshot.block_era_index(block_header.timestamp);

        match block_era.cmp(&chain_head_era) {
            Ordering::Less => {
                return if block_era + 1 != chain_head_era {
                    Err(VerificationError::TooOldEra {
                        expected_era: chain_head_era,
                        received_era: block_era,
                    })
                } else {
                    // node may be not synced yet
                    Err(VerificationError::OldEra {
                        expected_era: chain_head_era,
                        received_era: block_era,
                    })
                };
            }
            Ordering::Equal => {
                // both nodes are in sync
            }
            Ordering::Greater => {
                return if block_era != chain_head_era + 1 {
                    Err(VerificationError::TooNewEra {
                        expected_era: chain_head_era,
                        received_era: block_era,
                    })
                } else {
                    // node may be synced ahead
                    Err(VerificationError::NewEra {
                        expected_era: chain_head_era,
                        received_era: block_era,
                    })
                };
            }
        }

        Ok(())
    }

    /// Swap to a fresher validator snapshot and re-run cached messages when the
    /// era advances.
    ///
    /// Cached messages are only revisited if the new snapshot represents a
    /// strictly newer era than the one previously held; this prevents
    /// unnecessary revalidation while height moves inside the same era.
    pub(crate) fn on_new_snapshot(&mut self, snapshot: Arc<ValidatorListSnapshot>) {
        let is_older_era = self.snapshot.is_older_era(&snapshot);

        self.snapshot = snapshot;

        // don't reverify messages if era hasn't changed yet
        if !is_older_era {
            return;
        }

        let cached_messages =
            mem::replace(&mut self.cached_messages, LruCache::new(MAX_CACHED_PEERS));
        'cached: for (source, messages) in cached_messages {
            for (message, ()) in messages {
                match self.inner_verify(&message) {
                    Ok(()) => {
                        self.verified_messages.push_back(message);
                    }
                    Err(err) => {
                        log::trace!(
                            "failed to verify message again from {source} peer: {err}, message: {message:?}"
                        );
                        self.peer_score.invalid_data(source);
                        continue 'cached;
                    }
                }
            }
        }
    }

    /// Perform signature validation, chain context checks, and peer scoring.
    ///
    /// Returns the appropriate gossipsub acceptance outcome while optionally
    /// caching messages that can become valid once either the block header
    /// arrives (`UnknownBlock`) or the node enters the hinted next era
    /// (`NewEra`). All other mismatches are penalized via peer scoring and
    /// rejected immediately.
    pub(crate) fn verify_message_initially(
        &mut self,
        source: PeerId,
        message: SignedValidatorMessage,
    ) -> (MessageAcceptance, Option<VerifiedValidatorMessage>) {
        let message = message.into_verified();

        match self.inner_verify(&message) {
            Ok(()) => (MessageAcceptance::Accept, Some(message)),
            Err(VerificationError::OldEra { .. }) => (MessageAcceptance::Ignore, None),
            Err(err @ VerificationError::UnknownBlock { .. })
            | Err(err @ VerificationError::NewEra { .. }) => {
                log::trace!(
                    "cache message pending verification from {source} peer: {err}, message: {message:?}"
                );

                let existed = self
                    .cached_messages
                    .get_or_insert_mut(source, || LruCache::new(MAX_CACHED_MESSAGES_PER_PEER))
                    .put(message, ());
                // gossipsub should ignore a duplicated message
                debug_assert!(existed.is_none());

                (MessageAcceptance::Ignore, None)
            }
            Err(err @ VerificationError::TooOldEra { .. })
            | Err(err @ VerificationError::TooNewEra { .. })
            | Err(err @ VerificationError::AddressIsNotValidator { .. }) => {
                log::trace!(
                    "failed to verify message initially from {source} peer: {err}, message: {message:?}"
                );
                self.peer_score.invalid_data(source);
                (MessageAcceptance::Reject, None)
            }
        }
    }

    /// Retrieve the next verified message that is ready for further processing.
    pub(crate) fn next_message(&mut self) -> Option<VerifiedValidatorMessage> {
        self.verified_messages.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use ethexe_common::{
        Announce, BlockHeader, ProtocolTimelines, db::OnChainStorageRW, mock::Mock,
        network::ValidatorMessage,
    };
    use ethexe_db::Database;
    use ethexe_signer::Signer;
    use nonempty::{NonEmpty, nonempty};
    use std::iter;

    const GENESIS_TIMESTAMP: u64 = 1_000_000;
    const ERA_DURATION: u64 = 1_000;
    const GENESIS_CHAIN_HEAD: H256 = H256::zero();
    const CHAIN_HEAD_ERA: u64 = 10;
    const CHAIN_HEAD_TIMESTAMP: u64 = GENESIS_TIMESTAMP + (ERA_DURATION * CHAIN_HEAD_ERA);
    const PROTOCOL_TIMELINES: ProtocolTimelines = ProtocolTimelines {
        genesis_ts: GENESIS_TIMESTAMP,
        era: ERA_DURATION,
        election: ERA_DURATION / 2,
    };

    fn new_snapshot(
        current_era_index: u64,
        current_validators: NonEmpty<Address>,
    ) -> Arc<ValidatorListSnapshot> {
        Arc::new(ValidatorListSnapshot {
            current_era_index,
            timelines: PROTOCOL_TIMELINES,
            current_validators: current_validators.into(),
            next_validators: None,
        })
    }

    fn new_validators_with(validators: NonEmpty<Address>) -> (ValidatorTopic, Database) {
        let db = Database::memory();
        db.set_block_header(
            GENESIS_CHAIN_HEAD,
            BlockHeader {
                height: 0,
                timestamp: CHAIN_HEAD_TIMESTAMP,
                parent_hash: H256::random(),
            },
        );

        let snapshot = Arc::new(ValidatorListSnapshot {
            current_era_index: CHAIN_HEAD_ERA,
            timelines: PROTOCOL_TIMELINES,
            current_validators: validators.into(),
            next_validators: None,
        });

        let topic = ValidatorTopic::new(
            ValidatorDatabase::clone_boxed(&db),
            peer_score::Handle::new_test(),
            snapshot,
        );

        (topic, db)
    }

    fn new_validators() -> (ValidatorTopic, Database) {
        new_validators_with(nonempty![Address::default()])
    }

    fn new_validator_message() -> (Address, SignedValidatorMessage, H256) {
        let signer = Signer::memory();
        let pub_key = signer.generate_key().unwrap();

        let block = H256::random();

        let message = signer
            .signed_data(
                pub_key,
                ValidatorMessage {
                    block,
                    payload: Announce::mock(()),
                },
            )
            .map(SignedValidatorMessage::from)
            .unwrap();

        (pub_key.to_address(), message, block)
    }

    #[test]
    fn unknown_block() {
        const BOB_BLOCK_ERA: u64 = CHAIN_HEAD_ERA + 1;
        const BOB_BLOCK_TIMESTAMP: u64 = CHAIN_HEAD_TIMESTAMP + (ERA_DURATION * 1);

        let (bob_address, bob_message, bob_block) = new_validator_message();
        let bob_verified = bob_message.clone().into_verified();

        let (mut alice, alice_db) = new_validators_with(nonempty![bob_address]);

        let err = alice.inner_verify(&bob_verified).unwrap_err();
        assert_eq!(err, VerificationError::UnknownBlock { block: bob_block });

        let bob_source = PeerId::random();
        let (acceptance, verified_msg) = alice.verify_message_initially(bob_source, bob_message);
        assert_matches!(acceptance, MessageAcceptance::Ignore);
        assert_eq!(verified_msg, None);
        assert_eq!(alice.cached_messages.len(), 1);

        alice_db.set_block_header(
            bob_block,
            BlockHeader {
                height: 0,
                timestamp: BOB_BLOCK_TIMESTAMP,
                parent_hash: Default::default(),
            },
        );
        let snapshot = new_snapshot(BOB_BLOCK_ERA, nonempty![bob_address]);
        alice.on_new_snapshot(snapshot);

        assert_eq!(alice.next_message(), Some(bob_verified));
    }

    #[test]
    fn too_old_era() {
        let (bob_address, bob_message, bob_block) = new_validator_message();
        let bob_verified = bob_message.clone().into_verified();

        let (mut alice, alice_db) = new_validators_with(nonempty![bob_address]);

        alice_db.set_block_header(
            bob_block,
            BlockHeader {
                height: 1,
                timestamp: CHAIN_HEAD_TIMESTAMP - (ERA_DURATION * 2),
                parent_hash: Default::default(),
            },
        );

        let chain_head_era = alice.snapshot.block_era_index(CHAIN_HEAD_TIMESTAMP);

        let err = alice.inner_verify(&bob_verified).unwrap_err();
        assert_eq!(
            err,
            VerificationError::TooOldEra {
                expected_era: chain_head_era,
                received_era: chain_head_era - 2
            }
        );

        let bob_source = PeerId::random();
        let (acceptance, verified_msg) = alice.verify_message_initially(bob_source, bob_message);
        assert_matches!(acceptance, MessageAcceptance::Reject);
        assert_eq!(verified_msg, None);
        assert_eq!(alice.cached_messages.len(), 0);
        assert_eq!(alice.next_message(), None);
    }

    #[test]
    fn old_era() {
        let (bob_address, bob_message, bob_block) = new_validator_message();
        let bob_verified = bob_message.clone().into_verified();

        let (mut alice, alice_db) = new_validators_with(nonempty![bob_address]);

        alice_db.set_block_header(
            bob_block,
            BlockHeader {
                height: 1,
                timestamp: CHAIN_HEAD_TIMESTAMP - ERA_DURATION,
                parent_hash: Default::default(),
            },
        );

        let chain_head_era = alice.snapshot.block_era_index(CHAIN_HEAD_TIMESTAMP);

        let err = alice.inner_verify(&bob_verified).unwrap_err();
        assert_eq!(
            err,
            VerificationError::OldEra {
                expected_era: chain_head_era,
                received_era: chain_head_era - 1
            }
        );

        let bob_source = PeerId::random();
        let (acceptance, verified_msg) = alice.verify_message_initially(bob_source, bob_message);
        assert_matches!(acceptance, MessageAcceptance::Ignore);
        assert_eq!(verified_msg, None);
        assert_eq!(alice.cached_messages.len(), 0);
        assert_eq!(alice.next_message(), None);
    }

    #[test]
    fn too_new_era() {
        const BOB_BLOCK_TIMESTAMP: u64 = CHAIN_HEAD_TIMESTAMP + (ERA_DURATION * 2);

        let (bob_address, bob_message, bob_block) = new_validator_message();
        let bob_verified = bob_message.clone().into_verified();

        let (mut alice, alice_db) = new_validators_with(nonempty![bob_address]);

        alice_db.set_block_header(
            bob_block,
            BlockHeader {
                height: 1,
                timestamp: BOB_BLOCK_TIMESTAMP,
                parent_hash: Default::default(),
            },
        );

        let chain_head_era = alice.snapshot.block_era_index(CHAIN_HEAD_TIMESTAMP);

        let err = alice.inner_verify(&bob_verified).unwrap_err();
        assert_eq!(
            err,
            VerificationError::TooNewEra {
                expected_era: chain_head_era,
                received_era: chain_head_era + 2
            }
        );

        let bob_source = PeerId::random();
        let (acceptance, verified_msg) = alice.verify_message_initially(bob_source, bob_message);
        assert_matches!(acceptance, MessageAcceptance::Reject);
        assert_eq!(verified_msg, None);
        assert_eq!(alice.cached_messages.len(), 0);
    }

    #[test]
    fn new_era() {
        const BOB_BLOCK_ERA: u64 = CHAIN_HEAD_ERA + 1;
        const BOB_BLOCK_TIMESTAMP: u64 = CHAIN_HEAD_TIMESTAMP + ERA_DURATION;

        let (bob_address, bob_message, bob_block) = new_validator_message();
        let bob_verified = bob_message.clone().into_verified();

        let (mut alice, alice_db) = new_validators_with(nonempty![bob_address]);

        alice_db.set_block_header(
            bob_block,
            BlockHeader {
                height: 1,
                timestamp: BOB_BLOCK_TIMESTAMP,
                parent_hash: Default::default(),
            },
        );

        let chain_head_era = alice.snapshot.block_era_index(CHAIN_HEAD_TIMESTAMP);

        let err = alice.inner_verify(&bob_verified).unwrap_err();
        assert_eq!(
            err,
            VerificationError::NewEra {
                expected_era: chain_head_era,
                received_era: chain_head_era + 1
            }
        );

        let bob_source = PeerId::random();
        let (acceptance, verified_msg) = alice.verify_message_initially(bob_source, bob_message);
        assert_matches!(acceptance, MessageAcceptance::Ignore);
        assert_eq!(verified_msg, None);
        assert_eq!(alice.cached_messages.len(), 1);

        let snapshot = new_snapshot(BOB_BLOCK_ERA, nonempty![bob_address]);
        alice.on_new_snapshot(snapshot);

        assert_eq!(alice.next_message(), Some(bob_verified));
    }

    #[test]
    fn address_is_not_validator() {
        let (mut alice, _alice_db) = new_validators();
        let (bob_address, bob_message, _bob_block) = new_validator_message();
        let bob_verified = bob_message.clone().into_verified();

        let err = alice.inner_verify(&bob_verified).unwrap_err();
        assert_eq!(
            err,
            VerificationError::AddressIsNotValidator {
                address: bob_address
            }
        );

        let bob_source = PeerId::random();
        let (acceptance, verified_msg) = alice.verify_message_initially(bob_source, bob_message);
        assert_matches!(acceptance, MessageAcceptance::Reject);
        assert_eq!(verified_msg, None);
        assert_eq!(alice.cached_messages.len(), 0);
        assert_eq!(alice.next_message(), None);
    }

    #[test]
    fn success() {
        let (bob_address, bob_message, bob_block) = new_validator_message();
        let bob_verified = bob_message.clone().into_verified();

        let (mut alice, alice_db) = new_validators_with(nonempty![bob_address]);

        alice_db.set_block_header(
            bob_block,
            BlockHeader {
                height: 1,
                timestamp: CHAIN_HEAD_TIMESTAMP,
                parent_hash: Default::default(),
            },
        );

        alice.inner_verify(&bob_verified).unwrap();

        let bob_source = PeerId::random();
        let (acceptance, verified_msg) = alice.verify_message_initially(bob_source, bob_message);
        assert_matches!(acceptance, MessageAcceptance::Accept);
        assert_eq!(verified_msg, Some(bob_verified));
    }

    #[test]
    fn reverify_cached_messages_with_bad_peer() {
        const NEXT_ERA: u64 = CHAIN_HEAD_ERA + 1;
        const NEXT_ERA_TIMESTAMP: u64 = CHAIN_HEAD_TIMESTAMP + ERA_DURATION;

        // Bob creates a valid message for next era (will be cached)
        let (bob_address, bob_message, bob_block) = new_validator_message();
        let bob_verified = bob_message.clone().into_verified();

        // Charlie creates a valid message for next era (will be cached)
        let (charlie_address, charlie_message, charlie_block) = new_validator_message();
        let charlie_verified = charlie_message.clone().into_verified();

        // Dave creates a message for next era (will be cached, then become invalid when not a validator)
        let (dave_address, dave_message, dave_block) = new_validator_message();

        let (mut alice, alice_db) =
            new_validators_with(nonempty![bob_address, charlie_address, dave_address]);

        // Setup all blocks for next era
        alice_db.set_block_header(
            bob_block,
            BlockHeader {
                height: 1,
                timestamp: NEXT_ERA_TIMESTAMP,
                parent_hash: Default::default(),
            },
        );

        alice_db.set_block_header(
            charlie_block,
            BlockHeader {
                height: 2,
                timestamp: NEXT_ERA_TIMESTAMP,
                parent_hash: Default::default(),
            },
        );

        alice_db.set_block_header(
            dave_block,
            BlockHeader {
                height: 3,
                timestamp: NEXT_ERA_TIMESTAMP,
                parent_hash: Default::default(),
            },
        );

        // All three messages are cached (NewEra)
        let bob_source = PeerId::random();
        let charlie_source = PeerId::random();
        let dave_source = PeerId::random();

        let (bob_acceptance, bob_verified_msg) =
            alice.verify_message_initially(bob_source, bob_message);
        assert_matches!(bob_acceptance, MessageAcceptance::Ignore);
        assert!(bob_verified_msg.is_none());

        let (charlie_acceptance, charlie_verified_msg) =
            alice.verify_message_initially(charlie_source, charlie_message);
        assert_matches!(charlie_acceptance, MessageAcceptance::Ignore);
        assert!(charlie_verified_msg.is_none());

        let (dave_acceptance, dave_verified_msg) =
            alice.verify_message_initially(dave_source, dave_message);
        assert_matches!(dave_acceptance, MessageAcceptance::Ignore);
        assert!(dave_verified_msg.is_none());

        assert_eq!(alice.cached_messages.len(), 3);

        // Update chain head to next era, but Dave is no longer a validator
        let snapshot = new_snapshot(NEXT_ERA, nonempty![bob_address, charlie_address]);
        alice.on_new_snapshot(snapshot);

        // Bob and Charlie should be verified, Dave should fail but not block others
        let verified: Vec<_> = iter::from_fn(|| alice.next_message()).collect();

        // Both Bob's and Charlie's messages should be verified despite Dave's failure
        assert_eq!(verified.len(), 2);
        assert!(verified.contains(&bob_verified));
        assert!(verified.contains(&charlie_verified));
    }
}
