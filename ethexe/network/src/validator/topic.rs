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
    db_sync::PeerId, gossipsub::MessageAcceptance, peer_score,
    validator::list::ValidatorListSnapshot,
};
use ethexe_common::{Address, network::VerifiedValidatorMessage};
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
enum VerificationIgnoreReason {
    #[display("old era: expected {expected_era}, got {received_era}")]
    OldEra {
        expected_era: u64,
        received_era: u64,
    },
}

#[derive(Debug, Eq, PartialEq, derive_more::Display)]
enum VerificationCacheReason {
    #[display("new era: expected {expected_era}, got {received_era}")]
    NewEra {
        expected_era: u64,
        received_era: u64,
    },
}

#[derive(Debug, Eq, PartialEq, derive_more::Display)]
enum VerificationRejectReason {
    #[display("too old era: expected {expected_era}, got {received_era}")]
    TooOldEra {
        expected_era: u64,
        received_era: u64,
    },
    #[display("too new era: expected {expected_era}, got {received_era}")]
    TooNewEra {
        expected_era: u64,
        received_era: u64,
    },
    #[display("address {address} is not validator")]
    AddressIsNotValidator { address: Address },
}

#[derive(Debug, Eq, PartialEq, derive_more::Display, derive_more::From, derive_more::Unwrap)]
enum VerificationError {
    Ignore(VerificationIgnoreReason),
    Cache(VerificationCacheReason),
    Reject(VerificationRejectReason),
}

/// Tracks validator-signed messages and admits each one once the on-chain
/// context confirms it is timely and originates from a legitimate validator.
///
/// Legitimacy is checked via the `era_index` attached to
/// [`ValidatorMessage`](ethexe_common::network::ValidatorMessage) and the
/// validator-signed payload it carries. The hinted era must match the current
/// chain head; eras N-1, N+2, N+3, and so on are dropped when the node is at era N.
/// Messages from era N+1 are rechecked after the next validator set arrives.
pub struct ValidatorTopic {
    cached_messages: CachedMessages,
    verified_messages: VecDeque<VerifiedValidatorMessage>,
    peer_score: peer_score::Handle,
    snapshot: Arc<ValidatorListSnapshot>,
}

impl ValidatorTopic {
    pub fn new(peer_score: peer_score::Handle, snapshot: Arc<ValidatorListSnapshot>) -> Self {
        Self {
            cached_messages: LruCache::new(MAX_CACHED_PEERS),
            verified_messages: VecDeque::new(),
            peer_score,
            snapshot,
        }
    }

    fn inner_verify(&self, message: &VerifiedValidatorMessage) -> Result<(), VerificationError> {
        let chain_head_era = self.snapshot.current_era_index;

        let message_era = message.era_index();
        let address = message.address();

        let res: Result<(), VerificationError> = match message_era.cmp(&chain_head_era) {
            Ordering::Less => {
                if message_era + 1 != chain_head_era {
                    Err(VerificationRejectReason::TooOldEra {
                        expected_era: chain_head_era,
                        received_era: message_era,
                    }
                    .into())
                } else {
                    // node may be not synced yet
                    Err(VerificationIgnoreReason::OldEra {
                        expected_era: chain_head_era,
                        received_era: message_era,
                    }
                    .into())
                }
            }
            Ordering::Equal => {
                // both nodes are in sync

                Ok(())
            }
            Ordering::Greater => {
                if message_era != chain_head_era + 1 {
                    Err(VerificationRejectReason::TooNewEra {
                        expected_era: chain_head_era,
                        received_era: message_era,
                    }
                    .into())
                } else {
                    // node may be synced ahead
                    Err(VerificationCacheReason::NewEra {
                        expected_era: chain_head_era,
                        received_era: message_era,
                    }
                    .into())
                }
            }
        };

        // check if the address is a validator
        match res {
            Ok(()) | Err(VerificationError::Cache(_)) => {
                // if there are no errors, or it is a cache reason, then we definitely need to check
                if !self.snapshot.is_current_or_next(address) {
                    return Err(VerificationRejectReason::AddressIsNotValidator { address }.into());
                }
            }
            Err(VerificationError::Ignore(_)) => {
                // ignore reason would require keeping previous era validators to
                // honestly penalize the peer, so we only ignore the message.
                // `ValidatorList` (and `NetworkService` in general) would have
                // more complex initialization to fetch previous validators
            }
            Err(VerificationError::Reject(_)) => {
                // peer would be penalized anyway, so no need to check
            }
        }

        res
    }

    /// Swap to a fresher validator snapshot and re-run cached messages when the
    /// era advances.
    ///
    /// Cached messages are only revisited if the new snapshot represents a
    /// strictly newer era than the one previously held; this prevents
    /// unnecessary revalidation while height moves inside the same era.
    pub(crate) fn on_new_snapshot(&mut self, snapshot: Arc<ValidatorListSnapshot>) {
        let current_era = self.snapshot.current_era_index;
        let maybe_new_era = snapshot.current_era_index;

        self.snapshot = snapshot;

        // don't reverify messages if era hasn't changed yet
        if current_era >= maybe_new_era {
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
        message: VerifiedValidatorMessage,
    ) -> (MessageAcceptance, Option<VerifiedValidatorMessage>) {
        match self.inner_verify(&message) {
            Ok(()) => (MessageAcceptance::Accept, Some(message)),
            Err(VerificationError::Ignore(reason)) => {
                log::trace!("ignore message from {source} peer: {reason:?}, message: {message:?}");
                (MessageAcceptance::Ignore, None)
            }
            Err(VerificationError::Cache(reason)) => {
                log::trace!(
                    "cache message pending verification from {source} peer: {reason}, message: {message:?}"
                );

                let existed = self
                    .cached_messages
                    .get_or_insert_mut(source, || LruCache::new(MAX_CACHED_MESSAGES_PER_PEER))
                    .put(message, ());
                // gossipsub should ignore a duplicated message
                debug_assert!(existed.is_none());

                (MessageAcceptance::Ignore, None)
            }
            Err(VerificationError::Reject(reason)) => {
                log::trace!(
                    "failed to verify message initially from {source} peer: {reason}, message: {message:?}"
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
        Announce,
        mock::Mock,
        network::{SignedValidatorMessage, ValidatorMessage},
    };
    use ethexe_signer::Signer;
    use nonempty::{NonEmpty, nonempty};
    use std::iter;

    const CHAIN_HEAD_ERA: u64 = 10;

    fn new_snapshot(
        current_era_index: u64,
        current_validators: NonEmpty<Address>,
    ) -> Arc<ValidatorListSnapshot> {
        Arc::new(ValidatorListSnapshot {
            current_era_index,
            current_validators: current_validators.into(),
            next_validators: None,
        })
    }

    fn new_topic(validators: NonEmpty<Address>) -> ValidatorTopic {
        ValidatorTopic::new(
            peer_score::Handle::new_test(),
            new_snapshot(CHAIN_HEAD_ERA, validators),
        )
    }

    fn new_validator_message(era_index: u64) -> VerifiedValidatorMessage {
        let signer = Signer::memory();
        let pub_key = signer.generate_key().unwrap();

        signer
            .signed_data(
                pub_key,
                ValidatorMessage {
                    era_index,
                    payload: Announce::mock(()),
                },
            )
            .map(SignedValidatorMessage::from)
            .unwrap()
            .into_verified()
    }

    #[test]
    fn too_old_era() {
        let bob_message = new_validator_message(CHAIN_HEAD_ERA - 2);
        let mut alice = new_topic(nonempty![bob_message.address()]);

        let err = alice
            .inner_verify(&bob_message)
            .unwrap_err()
            .unwrap_reject();
        assert_eq!(
            err,
            VerificationRejectReason::TooOldEra {
                expected_era: CHAIN_HEAD_ERA,
                received_era: CHAIN_HEAD_ERA - 2
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
        let bob_message = new_validator_message(CHAIN_HEAD_ERA - 1);
        let mut alice = new_topic(nonempty![bob_message.address()]);

        let err = alice
            .inner_verify(&bob_message)
            .unwrap_err()
            .unwrap_ignore();
        assert_eq!(
            err,
            VerificationIgnoreReason::OldEra {
                expected_era: CHAIN_HEAD_ERA,
                received_era: CHAIN_HEAD_ERA - 1
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
        let bob_message = new_validator_message(CHAIN_HEAD_ERA + 2);
        let mut alice = new_topic(nonempty![bob_message.address()]);

        let err = alice
            .inner_verify(&bob_message)
            .unwrap_err()
            .unwrap_reject();
        assert_eq!(
            err,
            VerificationRejectReason::TooNewEra {
                expected_era: CHAIN_HEAD_ERA,
                received_era: CHAIN_HEAD_ERA + 2
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

        let bob_message = new_validator_message(BOB_BLOCK_ERA);
        let mut alice = new_topic(nonempty![bob_message.address()]);

        let err = alice.inner_verify(&bob_message).unwrap_err().unwrap_cache();
        assert_eq!(
            err,
            VerificationCacheReason::NewEra {
                expected_era: CHAIN_HEAD_ERA,
                received_era: CHAIN_HEAD_ERA + 1
            }
        );

        let bob_source = PeerId::random();
        let (acceptance, verified_msg) =
            alice.verify_message_initially(bob_source, bob_message.clone());
        assert_matches!(acceptance, MessageAcceptance::Ignore);
        assert_eq!(verified_msg, None);
        assert_eq!(alice.cached_messages.len(), 1);

        let snapshot = new_snapshot(BOB_BLOCK_ERA, nonempty![bob_message.address()]);
        alice.on_new_snapshot(snapshot);

        assert_eq!(alice.next_message(), Some(bob_message));
    }

    #[test]
    fn address_is_not_validator() {
        let mut alice = new_topic(nonempty![Address::default()]);
        let bob_message = new_validator_message(CHAIN_HEAD_ERA + 1);

        let err = alice
            .inner_verify(&bob_message)
            .unwrap_err()
            .unwrap_reject();
        assert_eq!(
            err,
            VerificationRejectReason::AddressIsNotValidator {
                address: bob_message.address()
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
    fn new_era_address_is_not_validator() {
        let bob_message = new_validator_message(CHAIN_HEAD_ERA);
        let charlie_message = new_validator_message(CHAIN_HEAD_ERA + 1);

        let mut alice = new_topic(nonempty![Default::default()]);

        for message in [bob_message, charlie_message] {
            let err = alice.inner_verify(&message).unwrap_err().unwrap_reject();
            assert_eq!(
                err,
                VerificationRejectReason::AddressIsNotValidator {
                    address: message.address()
                }
            );

            let bob_source = PeerId::random();
            let (acceptance, verified_msg) = alice.verify_message_initially(bob_source, message);
            assert_matches!(acceptance, MessageAcceptance::Reject);
            assert_eq!(verified_msg, None);
            assert_eq!(alice.cached_messages.len(), 0);
            assert_eq!(alice.next_message(), None);
        }
    }

    #[test]
    fn success() {
        let bob_message = new_validator_message(CHAIN_HEAD_ERA);
        let mut alice = new_topic(nonempty![bob_message.address()]);

        alice.inner_verify(&bob_message).unwrap();

        let bob_source = PeerId::random();
        let (acceptance, verified_msg) =
            alice.verify_message_initially(bob_source, bob_message.clone());
        assert_matches!(acceptance, MessageAcceptance::Accept);
        assert_eq!(verified_msg, Some(bob_message));
    }

    #[test]
    fn reverify_cached_messages_with_bad_peer() {
        const NEXT_ERA: u64 = CHAIN_HEAD_ERA + 1;

        // Bob creates a valid message for next era (will be cached)
        let bob_message = new_validator_message(NEXT_ERA);

        // Charlie creates a valid message for next era (will be cached)
        let charlie_message = new_validator_message(NEXT_ERA);

        // Dave creates a message for next era (will be cached, then become invalid when not a validator)
        let dave_message = new_validator_message(NEXT_ERA);

        let mut alice = new_topic(nonempty![
            bob_message.address(),
            charlie_message.address(),
            dave_message.address()
        ]);
        let init_snapshot = alice.snapshot.clone();

        // All three messages are cached (NewEra)
        let bob_source = PeerId::random();
        let charlie_source = PeerId::random();
        let dave_source = PeerId::random();

        let (bob_acceptance, bob_verified_msg) =
            alice.verify_message_initially(bob_source, bob_message.clone());
        assert_matches!(bob_acceptance, MessageAcceptance::Ignore);
        assert!(bob_verified_msg.is_none());

        let (charlie_acceptance, charlie_verified_msg) =
            alice.verify_message_initially(charlie_source, charlie_message.clone());
        assert_matches!(charlie_acceptance, MessageAcceptance::Ignore);
        assert!(charlie_verified_msg.is_none());

        let (dave_acceptance, dave_verified_msg) =
            alice.verify_message_initially(dave_source, dave_message);
        assert_matches!(dave_acceptance, MessageAcceptance::Ignore);
        assert!(dave_verified_msg.is_none());

        assert_eq!(alice.cached_messages.len(), 3);

        // Update chain head to next era, but Dave is no longer a validator
        let snapshot = new_snapshot(
            NEXT_ERA,
            nonempty![bob_message.address(), charlie_message.address()],
        );
        alice.on_new_snapshot(snapshot);

        // Bob and Charlie should be verified, Dave should fail but not block others
        let verified: Vec<_> = iter::from_fn(|| alice.next_message()).collect();

        // Both Bob's and Charlie's messages should be verified despite Dave's failure
        assert_eq!(alice.cached_messages.len(), 0);
        assert_eq!(verified.len(), 2);
        assert!(verified.contains(&bob_message));
        assert!(verified.contains(&charlie_message));

        // reorg case
        alice.on_new_snapshot(init_snapshot.clone());
        assert_eq!(alice.snapshot, init_snapshot);
    }
}
