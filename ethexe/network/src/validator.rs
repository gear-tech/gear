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

use crate::{gossipsub::MessageAcceptance, peer_score};
use anyhow::Context;
use ethexe_common::{
    Address, BlockHeader,
    db::OnChainStorageRead,
    network::{SignedValidatorMessage, VerifiedValidatorMessage},
};
use ethexe_db::Database;
use gprimitives::H256;
use libp2p::PeerId;
use lru::LruCache as LruHashMap;
use nonempty::NonEmpty;
use std::{cmp::Ordering, collections::VecDeque, mem, num::NonZeroUsize};
use uluru::LRUCache as LruVec;

const MAX_CACHED_PEERS: NonZeroUsize = NonZeroUsize::new(50).unwrap();
const MAX_CACHED_MESSAGES_PER_PEER: usize = 20;

// used only in assertion
#[allow(dead_code)]
const TOTAL_CACHED_MESSAGES: usize = 1024;
const _: () =
    assert!(MAX_CACHED_PEERS.get() * MAX_CACHED_MESSAGES_PER_PEER <= TOTAL_CACHED_MESSAGES);

type CachedMessages =
    LruHashMap<PeerId, LruVec<VerifiedValidatorMessage, MAX_CACHED_MESSAGES_PER_PEER>>;

#[auto_impl::auto_impl(&, Box)]
pub trait ValidatorDatabase: Send + OnChainStorageRead {
    fn clone_boxed(&self) -> Box<dyn ValidatorDatabase>;
}

impl ValidatorDatabase for Database {
    fn clone_boxed(&self) -> Box<dyn ValidatorDatabase> {
        Box::new(self.clone())
    }
}

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

struct ChainHead {
    header: BlockHeader,
    current_validators: NonEmpty<Address>,
    next_validators: Option<NonEmpty<Address>>,
}

/// Tracks validator-signed messages and admits each one once the on-chain
/// context confirms it is timely and originates from a legitimate validator.
///
/// Legitimacy is checked via the `block` attached to
/// [`ValidatorMessage`](ethexe_common::network::ValidatorMessage) and the
/// validator-signed payload it carries. The hinted era must match the current
/// chain head; eras N-1, N+2, N+3, and so on are dropped when the node is at era N.
/// Messages from era N+1 are rechecked after the next validator set arrives.
pub(crate) struct Validators {
    genesis_timestamp: u64,
    era_duration: u64,

    cached_messages: CachedMessages,
    verified_messages: VecDeque<VerifiedValidatorMessage>,
    db: Box<dyn ValidatorDatabase>,
    chain_head: ChainHead,
    peer_score: peer_score::Handle,
}

impl Validators {
    pub(crate) fn new(
        genesis_timestamp: u64,
        era_duration: u64,
        genesis_block_hash: H256,
        db: Box<dyn ValidatorDatabase>,
        peer_score: peer_score::Handle,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            genesis_timestamp,
            era_duration,
            cached_messages: LruHashMap::new(MAX_CACHED_PEERS),
            verified_messages: VecDeque::new(),
            chain_head: Self::get_chain_head(&db, genesis_block_hash)?,
            db,
            peer_score,
        })
    }

    fn get_chain_head(db: &impl ValidatorDatabase, chain_head: H256) -> anyhow::Result<ChainHead> {
        let chain_head_header = db
            .block_header(chain_head)
            .context("chain head header not found")?;
        let validators = db
            .block_validators(chain_head)
            .context("validators not found")?;
        Ok(ChainHead {
            header: chain_head_header,
            current_validators: validators,
            next_validators: None,
        })
    }

    /// Refresh the current chain head and validator set snapshot.
    ///
    /// Previously cached messages are rechecked once the new context is available.
    pub(crate) fn set_chain_head(&mut self, chain_head: H256) -> anyhow::Result<()> {
        self.chain_head = Self::get_chain_head(&self.db, chain_head)?;

        self.verify_on_new_chain_head();

        Ok(())
    }

    // TODO: make actual implementation when `NextEraValidatorsCommitted` event is emitted before era transition
    #[allow(dead_code)]
    pub(crate) fn set_next_era_validators(&mut self) {}

    /// Retrieve the next verified message that is ready for further processing.
    pub(crate) fn next_message(&mut self) -> Option<VerifiedValidatorMessage> {
        self.verified_messages.pop_front()
    }

    fn block_era_index(&self, block_ts: u64) -> u64 {
        (block_ts - self.genesis_timestamp) / self.era_duration
    }

    fn inner_verify(&self, message: &VerifiedValidatorMessage) -> Result<(), VerificationError> {
        let ChainHead {
            header: chain_head,
            current_validators,
            next_validators,
        } = &self.chain_head;
        let chain_head_era = self.block_era_index(chain_head.timestamp);

        let block = message.block();
        let address = message.address();

        let is_current_validator = current_validators.contains(&address);
        let is_next_validator = next_validators
            .as_ref()
            .map(|v| v.contains(&address))
            .unwrap_or(false);
        if !is_current_validator && !is_next_validator {
            return Err(VerificationError::AddressIsNotValidator { address });
        }

        let Some(block_header) = self.db.block_header(block) else {
            return Err(VerificationError::UnknownBlock { block });
        };
        let block_era = self.block_era_index(block_header.timestamp);

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

    fn verify_on_new_chain_head(&mut self) {
        let cached_messages =
            mem::replace(&mut self.cached_messages, LruHashMap::new(MAX_CACHED_PEERS));
        'cached: for (source, messages) in cached_messages {
            for message in messages.iter().cloned() {
                match self.inner_verify(&message) {
                    Ok(()) => {
                        self.verified_messages.push_back(message);
                    }
                    Err(err) => {
                        log::trace!(
                            "failed to verify message again from {source} peer: {err}, message: {message:?}"
                        );
                        self.peer_score.invalid_data(source);
                        break 'cached;
                    }
                }
            }
        }
    }

    /// Perform signature validation, chain context checks, and peer scoring.
    ///
    /// Returns the appropriate gossipsub acceptance outcome while optionally
    /// caching messages that will become valid after the node catches up.
    pub(crate) fn verify_message_initially(
        &mut self,
        source: PeerId,
        message: SignedValidatorMessage,
    ) -> MessageAcceptance {
        let message = message.into_verified();

        match self.inner_verify(&message) {
            Ok(()) => {
                self.verified_messages.push_back(message);
                MessageAcceptance::Accept
            }
            Err(VerificationError::OldEra { .. }) => MessageAcceptance::Ignore,
            Err(VerificationError::UnknownBlock { .. }) | Err(VerificationError::NewEra { .. }) => {
                self.cached_messages
                    .get_or_insert_mut(source, LruVec::new)
                    .insert(message);
                MessageAcceptance::Ignore
            }
            Err(err @ VerificationError::TooOldEra { .. })
            | Err(err @ VerificationError::TooNewEra { .. })
            | Err(err @ VerificationError::AddressIsNotValidator { .. }) => {
                log::trace!(
                    "failed to verify message initially from {source} peer: {err}, message: {message:?}"
                );
                self.peer_score.invalid_data(source);
                MessageAcceptance::Reject
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use ethexe_common::{Announce, db::OnChainStorageWrite, mock::Mock, network::ValidatorMessage};
    use ethexe_signer::Signer;
    use nonempty::nonempty;

    const GENESIS_TIMESTAMP: u64 = 1_000_000;
    const ERA_DURATION: u64 = 1_000;
    const GENESIS_CHAIN_HEAD: H256 = H256::zero();
    const CHAIN_HEAD_TIMESTAMP: u64 = GENESIS_TIMESTAMP + (ERA_DURATION * 10);

    fn new_validators() -> (Validators, Database) {
        let db = Database::memory();
        db.set_block_header(
            GENESIS_CHAIN_HEAD,
            BlockHeader {
                height: 0,
                timestamp: CHAIN_HEAD_TIMESTAMP,
                parent_hash: H256::random(),
            },
        );
        db.set_block_validators(GENESIS_CHAIN_HEAD, nonempty![Address::default()]);

        let validators = Validators::new(
            GENESIS_TIMESTAMP,
            ERA_DURATION,
            GENESIS_CHAIN_HEAD,
            ValidatorDatabase::clone_boxed(&db),
            peer_score::Handle::new_test(),
        )
        .unwrap();

        (validators, db)
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
        const BOB_BLOCK_TIMESTAMP: u64 = CHAIN_HEAD_TIMESTAMP + (ERA_DURATION * 100);

        let (mut alice, alice_db) = new_validators();
        let (bob_address, bob_message, bob_block) = new_validator_message();
        let bob_verified = bob_message.clone().into_verified();

        alice_db.set_block_validators(GENESIS_CHAIN_HEAD, nonempty![bob_address]);
        alice.set_chain_head(GENESIS_CHAIN_HEAD).unwrap();

        let err = alice.inner_verify(&bob_verified).unwrap_err();
        assert_eq!(err, VerificationError::UnknownBlock { block: bob_block });

        let bob_source = PeerId::random();
        let acceptance = alice.verify_message_initially(bob_source, bob_message);
        assert_matches!(acceptance, MessageAcceptance::Ignore);
        assert_eq!(alice.cached_messages.len(), 1);

        alice_db.set_block_header(
            bob_block,
            BlockHeader {
                height: 0,
                timestamp: BOB_BLOCK_TIMESTAMP,
                parent_hash: Default::default(),
            },
        );
        let new_chain_head = H256::random();
        alice_db.set_block_header(
            new_chain_head,
            BlockHeader {
                height: 0,
                timestamp: BOB_BLOCK_TIMESTAMP,
                parent_hash: Default::default(),
            },
        );
        alice_db.set_block_validators(new_chain_head, nonempty![bob_address]);
        alice.set_chain_head(new_chain_head).unwrap();

        assert_eq!(alice.next_message(), Some(bob_verified));
    }

    #[test]
    fn too_old_era() {
        let (mut alice, alice_db) = new_validators();
        let (bob_address, bob_message, bob_block) = new_validator_message();
        let bob_message = bob_message.into_verified();

        alice_db.set_block_header(
            bob_block,
            BlockHeader {
                height: 1,
                timestamp: CHAIN_HEAD_TIMESTAMP - (ERA_DURATION * 2),
                parent_hash: Default::default(),
            },
        );
        alice_db.set_block_validators(GENESIS_CHAIN_HEAD, nonempty![bob_address]);
        alice.set_chain_head(GENESIS_CHAIN_HEAD).unwrap();

        let chain_head_era = alice.block_era_index(CHAIN_HEAD_TIMESTAMP);

        let err = alice.inner_verify(&bob_message).unwrap_err();
        assert_eq!(
            err,
            VerificationError::TooOldEra {
                expected_era: chain_head_era,
                received_era: chain_head_era - 2
            }
        );
    }

    #[test]
    fn old_era() {
        let (mut alice, alice_db) = new_validators();
        let (bob_address, bob_message, bob_block) = new_validator_message();
        let bob_message = bob_message.into_verified();

        alice_db.set_block_header(
            bob_block,
            BlockHeader {
                height: 1,
                timestamp: CHAIN_HEAD_TIMESTAMP - ERA_DURATION,
                parent_hash: Default::default(),
            },
        );
        alice_db.set_block_validators(GENESIS_CHAIN_HEAD, nonempty![bob_address]);
        alice.set_chain_head(GENESIS_CHAIN_HEAD).unwrap();

        let chain_head_era = alice.block_era_index(CHAIN_HEAD_TIMESTAMP);

        let err = alice.inner_verify(&bob_message).unwrap_err();
        assert_eq!(
            err,
            VerificationError::OldEra {
                expected_era: chain_head_era,
                received_era: chain_head_era - 1
            }
        );
    }

    #[test]
    fn too_new_era() {
        const BOB_BLOCK_TIMESTAMP: u64 = CHAIN_HEAD_TIMESTAMP + (ERA_DURATION * 2);

        let (mut alice, alice_db) = new_validators();
        let (bob_address, bob_message, bob_block) = new_validator_message();
        let bob_verified = bob_message.clone().into_verified();

        alice_db.set_block_header(
            bob_block,
            BlockHeader {
                height: 1,
                timestamp: BOB_BLOCK_TIMESTAMP,
                parent_hash: Default::default(),
            },
        );
        alice_db.set_block_validators(GENESIS_CHAIN_HEAD, nonempty![bob_address]);
        alice.set_chain_head(GENESIS_CHAIN_HEAD).unwrap();

        let chain_head_era = alice.block_era_index(CHAIN_HEAD_TIMESTAMP);

        let err = alice.inner_verify(&bob_verified).unwrap_err();
        assert_eq!(
            err,
            VerificationError::TooNewEra {
                expected_era: chain_head_era,
                received_era: chain_head_era + 2
            }
        );

        let bob_source = PeerId::random();
        let acceptance = alice.verify_message_initially(bob_source, bob_message);
        assert_matches!(acceptance, MessageAcceptance::Reject);
        assert_eq!(alice.cached_messages.len(), 0);
    }

    #[test]
    fn new_era() {
        const BOB_BLOCK_TIMESTAMP: u64 = CHAIN_HEAD_TIMESTAMP + ERA_DURATION;

        let (mut alice, alice_db) = new_validators();
        let (bob_address, bob_message, bob_block) = new_validator_message();
        let bob_verified = bob_message.clone().into_verified();

        alice_db.set_block_header(
            bob_block,
            BlockHeader {
                height: 1,
                timestamp: BOB_BLOCK_TIMESTAMP,
                parent_hash: Default::default(),
            },
        );
        alice_db.set_block_validators(GENESIS_CHAIN_HEAD, nonempty![bob_address]);
        alice.set_chain_head(GENESIS_CHAIN_HEAD).unwrap();

        let chain_head_era = alice.block_era_index(CHAIN_HEAD_TIMESTAMP);

        let err = alice.inner_verify(&bob_verified).unwrap_err();
        assert_eq!(
            err,
            VerificationError::NewEra {
                expected_era: chain_head_era,
                received_era: chain_head_era + 1
            }
        );

        let bob_source = PeerId::random();
        let acceptance = alice.verify_message_initially(bob_source, bob_message);
        assert_matches!(acceptance, MessageAcceptance::Ignore);
        assert_eq!(alice.cached_messages.len(), 1);

        let new_chain_head = H256::random();
        alice_db.set_block_header(
            new_chain_head,
            BlockHeader {
                height: 0,
                timestamp: BOB_BLOCK_TIMESTAMP,
                parent_hash: Default::default(),
            },
        );
        alice_db.set_block_validators(new_chain_head, nonempty![bob_address]);
        alice.set_chain_head(new_chain_head).unwrap();

        assert_eq!(alice.next_message(), Some(bob_verified));
    }

    #[test]
    fn address_is_not_validator() {
        let (alice, _alice_db) = new_validators();
        let (bob_address, bob_message, _bob_block) = new_validator_message();
        let bob_message = bob_message.into_verified();

        let err = alice.inner_verify(&bob_message).unwrap_err();
        assert_eq!(
            err,
            VerificationError::AddressIsNotValidator {
                address: bob_address
            }
        );
    }

    #[test]
    fn success() {
        let (mut alice, alice_db) = new_validators();
        let (bob_address, bob_message, bob_block) = new_validator_message();
        let bob_verified = bob_message.clone().into_verified();

        alice_db.set_block_validators(GENESIS_CHAIN_HEAD, nonempty![bob_address]);
        alice.set_chain_head(GENESIS_CHAIN_HEAD).unwrap();

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
        let acceptance = alice.verify_message_initially(bob_source, bob_message);
        assert_matches!(acceptance, MessageAcceptance::Accept);

        assert_eq!(alice.next_message(), Some(bob_verified));
    }
}
