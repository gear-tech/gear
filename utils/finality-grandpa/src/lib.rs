// Copyright 2018-2019 Parity Technologies (UK) Ltd
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Finality gadget for blockchains.
//!
//! <https://github.com/w3f/consensus/blob/master/pdf/grandpa.pdf>
//!
//! Consensus proceeds in rounds. Each round, voters will cast a prevote
//! and precommit message.
//!
//! Votes on blocks are then applied to the blockchain, being recursively applied to
//! blocks before them. A DAG is superimposed over the blockchain with the `vote_graph` logic.
//!
//! Equivocation detection and vote-set management is done in the `round` module.
//! The work for actually casting votes is done in the `voter` module.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
#[macro_use]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod round;
pub mod vote_graph;
#[cfg(feature = "std")]
pub mod voter;
pub mod voter_set;

mod bitfield;
#[cfg(feature = "std")]
mod bridge_state;
#[cfg(any(test, feature = "fuzz-helpers"))]
pub mod fuzz_helpers;
#[cfg(test)]
mod testing;
mod weights;
#[cfg(not(feature = "std"))]
mod std {
    pub use core::{cmp, hash, iter, mem, num, ops};

    pub mod vec {
        pub use alloc::vec::Vec;
    }

    pub mod collections {
        pub use alloc::collections::{
            btree_map::{self, BTreeMap},
            btree_set::{self, BTreeSet},
        };
    }

    pub mod fmt {
        pub use core::fmt::{Display, Formatter, Result};

        pub trait Debug {}
        impl<T> Debug for T {}
    }
}

use crate::{std::vec::Vec, voter_set::VoterSet};
#[cfg(feature = "derive-codec")]
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode};
use round::ImportResult;
#[cfg(feature = "derive-codec")]
use scale_info::TypeInfo;

// Overarching log target
const LOG_TARGET: &str = "grandpa";

/// A prevote for a block and its ancestors.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "derive-codec",
    derive(Encode, Decode, DecodeWithMemTracking, TypeInfo)
)]
pub struct Prevote<H, N> {
    /// The target block's hash.
    pub target_hash: H,
    /// The target block's number.
    pub target_number: N,
}

impl<H, N> Prevote<H, N> {
    /// Create a new prevote for the given block (hash and number).
    pub fn new(target_hash: H, target_number: N) -> Self {
        Prevote {
            target_hash,
            target_number,
        }
    }
}

/// A precommit for a block and its ancestors.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "derive-codec",
    derive(Encode, Decode, DecodeWithMemTracking, TypeInfo)
)]
pub struct Precommit<H, N> {
    /// The target block's hash.
    pub target_hash: H,
    /// The target block's number
    pub target_number: N,
}

impl<H, N> Precommit<H, N> {
    /// Create a new precommit for the given block (hash and number).
    pub fn new(target_hash: H, target_number: N) -> Self {
        Precommit {
            target_hash,
            target_number,
        }
    }
}

/// A primary proposed block, this is a broadcast of the last round's estimate.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "derive-codec", derive(Encode, Decode, TypeInfo))]
pub struct PrimaryPropose<H, N> {
    /// The target block's hash.
    pub target_hash: H,
    /// The target block's number
    pub target_number: N,
}

impl<H, N> PrimaryPropose<H, N> {
    /// Create a new primary proposal for the given block (hash and number).
    pub fn new(target_hash: H, target_number: N) -> Self {
        PrimaryPropose {
            target_hash,
            target_number,
        }
    }
}

/// Top-level error type used by this crate.
#[derive(Clone, PartialEq, Debug)]
pub enum Error {
    /// The block is not a descendent of the given base block.
    NotDescendent,
}

#[cfg(feature = "std")]
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Error::NotDescendent => write!(f, "Block not descendent of base"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::NotDescendent => "Block not descendent of base",
        }
    }
}

/// Arithmetic necessary for a block number.
pub trait BlockNumberOps:
    std::fmt::Debug
    + std::cmp::Ord
    + std::ops::Add<Output = Self>
    + std::ops::Sub<Output = Self>
    + num::One
    + num::Zero
    + num::AsPrimitive<usize>
{
}

impl<T> BlockNumberOps for T
where
    T: std::fmt::Debug,
    T: std::cmp::Ord,
    T: std::ops::Add<Output = Self>,
    T: std::ops::Sub<Output = Self>,
    T: num::One,
    T: num::Zero,
    T: num::AsPrimitive<usize>,
{
}

/// Chain context necessary for implementation of the finality gadget.
pub trait Chain<H: Eq, N: Copy + BlockNumberOps> {
    /// Get the ancestry of a block up to but not including the base hash.
    /// Should be in reverse order from `block`'s parent.
    ///
    /// If the block is not a descendent of `base`, returns an error.
    fn ancestry(&self, base: H, block: H) -> Result<Vec<H>, Error>;

    /// Returns true if `block` is a descendent of or equal to the given `base`.
    fn is_equal_or_descendent_of(&self, base: H, block: H) -> bool {
        if base == block {
            return true;
        }

        // TODO: currently this function always succeeds since the only error
        // variant is `Error::NotDescendent`, this may change in the future as
        // other errors (e.g. IO) are not being exposed.
        match self.ancestry(base, block) {
            Ok(_) => true,
            Err(Error::NotDescendent) => false,
        }
    }
}

/// An equivocation (double-vote) in a given round.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "derive-codec",
    derive(Encode, Decode, DecodeWithMemTracking, TypeInfo)
)]
pub struct Equivocation<Id, V, S> {
    /// The round number equivocated in.
    pub round_number: u64,
    /// The identity of the equivocator.
    pub identity: Id,
    /// The first vote in the equivocation.
    pub first: (V, S),
    /// The second vote in the equivocation.
    pub second: (V, S),
}

/// A protocol message or vote.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "derive-codec", derive(Encode, Decode, TypeInfo))]
pub enum Message<H, N> {
    /// A prevote message.
    #[cfg_attr(feature = "derive-codec", codec(index = 0))]
    Prevote(Prevote<H, N>),
    /// A precommit message.
    #[cfg_attr(feature = "derive-codec", codec(index = 1))]
    Precommit(Precommit<H, N>),
    /// A primary proposal message.
    #[cfg_attr(feature = "derive-codec", codec(index = 2))]
    PrimaryPropose(PrimaryPropose<H, N>),
}

impl<H, N: Copy> Message<H, N> {
    /// Get the target block of the vote.
    pub fn target(&self) -> (&H, N) {
        match *self {
            Message::Prevote(ref v) => (&v.target_hash, v.target_number),
            Message::Precommit(ref v) => (&v.target_hash, v.target_number),
            Message::PrimaryPropose(ref v) => (&v.target_hash, v.target_number),
        }
    }
}

/// A signed message.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "derive-codec", derive(Encode, Decode, TypeInfo))]
pub struct SignedMessage<H, N, S, Id> {
    /// The internal message which has been signed.
    pub message: Message<H, N>,
    /// The signature on the message.
    pub signature: S,
    /// The Id of the signer
    pub id: Id,
}

impl<H, N, S, Id> Unpin for SignedMessage<H, N, S, Id> {}

impl<H, N: Copy, S, Id> SignedMessage<H, N, S, Id> {
    /// Get the target block of the vote.
    pub fn target(&self) -> (&H, N) {
        self.message.target()
    }
}

/// A commit message which is an aggregate of precommits.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(
    feature = "derive-codec",
    derive(Encode, Decode, DecodeWithMemTracking, TypeInfo)
)]
pub struct Commit<H, N, S, Id> {
    /// The target block's hash.
    pub target_hash: H,
    /// The target block's number.
    pub target_number: N,
    /// Precommits for target block or any block after it that justify this commit.
    pub precommits: Vec<SignedPrecommit<H, N, S, Id>>,
}

/// A signed prevote message.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "derive-codec", derive(Encode, Decode, TypeInfo))]
pub struct SignedPrevote<H, N, S, Id> {
    /// The prevote message which has been signed.
    pub prevote: Prevote<H, N>,
    /// The signature on the message.
    pub signature: S,
    /// The Id of the signer.
    pub id: Id,
}

/// A signed precommit message.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(
    feature = "derive-codec",
    derive(Encode, Decode, DecodeWithMemTracking, TypeInfo)
)]
pub struct SignedPrecommit<H, N, S, Id> {
    /// The precommit message which has been signed.
    pub precommit: Precommit<H, N>,
    /// The signature on the message.
    pub signature: S,
    /// The Id of the signer.
    pub id: Id,
}

/// A commit message with compact representation of authentication data.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "derive-codec", derive(Encode, Decode, TypeInfo))]
pub struct CompactCommit<H, N, S, Id> {
    /// The target block's hash.
    pub target_hash: H,
    /// The target block's number.
    pub target_number: N,
    /// Precommits for target block or any block after it that justify this commit.
    pub precommits: Vec<Precommit<H, N>>,
    /// Authentication data for the commit.
    pub auth_data: MultiAuthData<S, Id>,
}

/// A catch-up message, which is an aggregate of prevotes and precommits necessary
/// to complete a round.
///
/// This message contains a "base", which is a block all of the vote-targets are
/// a descendent of.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "derive-codec", derive(Encode, Decode, TypeInfo))]
pub struct CatchUp<H, N, S, Id> {
    /// Round number.
    pub round_number: u64,
    /// Prevotes for target block or any block after it that justify this catch-up.
    pub prevotes: Vec<SignedPrevote<H, N, S, Id>>,
    /// Precommits for target block or any block after it that justify this catch-up.
    pub precommits: Vec<SignedPrecommit<H, N, S, Id>>,
    /// The base hash. See struct docs.
    pub base_hash: H,
    /// The base number. See struct docs.
    pub base_number: N,
}

/// Authentication data for a set of many messages, currently a set of precommit signatures but
/// in the future could be optimized with BLS signature aggregation.
pub type MultiAuthData<S, Id> = Vec<(S, Id)>;

impl<H, N, S, Id> From<CompactCommit<H, N, S, Id>> for Commit<H, N, S, Id> {
    fn from(commit: CompactCommit<H, N, S, Id>) -> Commit<H, N, S, Id> {
        Commit {
            target_hash: commit.target_hash,
            target_number: commit.target_number,
            precommits: commit
                .precommits
                .into_iter()
                .zip(commit.auth_data)
                .map(|(precommit, (signature, id))| SignedPrecommit {
                    precommit,
                    signature,
                    id,
                })
                .collect(),
        }
    }
}

impl<H: Clone, N: Clone, S, Id> From<Commit<H, N, S, Id>> for CompactCommit<H, N, S, Id> {
    fn from(commit: Commit<H, N, S, Id>) -> CompactCommit<H, N, S, Id> {
        CompactCommit {
            target_hash: commit.target_hash,
            target_number: commit.target_number,
            precommits: commit
                .precommits
                .iter()
                .map(|signed| signed.precommit.clone())
                .collect(),
            auth_data: commit
                .precommits
                .into_iter()
                .map(|signed| (signed.signature, signed.id))
                .collect(),
        }
    }
}

/// Struct returned from `validate_commit` function with information
/// about the validation result.
#[derive(Debug, Default)]
pub struct CommitValidationResult {
    valid: bool,
    num_precommits: usize,
    num_duplicated_precommits: usize,
    num_equivocations: usize,
    num_invalid_voters: usize,
}

impl CommitValidationResult {
    /// Returns `true` if the commit is valid, which implies that the target
    /// block in the commit is finalized.
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// Returns the number of precommits in the commit.
    pub fn num_precommits(&self) -> usize {
        self.num_precommits
    }

    /// Returns the number of duplicate precommits in the commit.
    pub fn num_duplicated_precommits(&self) -> usize {
        self.num_duplicated_precommits
    }

    /// Returns the number of equivocated precommits in the commit.
    pub fn num_equivocations(&self) -> usize {
        self.num_equivocations
    }

    /// Returns the number of invalid voters in the commit, i.e. votes from
    /// identities that are not part of the voter set.
    pub fn num_invalid_voters(&self) -> usize {
        self.num_invalid_voters
    }
}

/// Validates a GRANDPA commit message.
///
/// For a commit to be valid the round ghost is calculated using the precommits
/// in the commit message, making sure that it exists and that it is the same
/// as the commit target. The precommit with the lowest block number is used as
/// the round base.
///
/// Signatures on precommits are assumed to have been checked.
///
/// Duplicate votes or votes from voters not in the voter-set will be ignored,
/// but it is recommended for the caller of this function to remove those at
/// signature-verification time.
pub fn validate_commit<H, N, S, I, C: Chain<H, N>>(
    commit: &Commit<H, N, S, I>,
    voters: &VoterSet<I>,
    chain: &C,
) -> Result<CommitValidationResult, crate::Error>
where
    H: Clone + Eq + Ord + std::fmt::Debug,
    N: Copy + BlockNumberOps + std::fmt::Debug,
    I: Clone + Ord + Eq + std::fmt::Debug,
    S: Clone + Eq,
{
    let mut validation_result = CommitValidationResult {
        num_precommits: commit.precommits.len(),
        ..Default::default()
    };

    // filter any precommits by voters that are not part of the set
    let valid_precommits = commit
        .precommits
        .iter()
        .filter(|signed| {
            if !voters.contains(&signed.id) {
                validation_result.num_invalid_voters += 1;
                return false;
            }

            true
        })
        .collect::<Vec<_>>();

    // the base of the round should be the lowest block for which we can find a
    // precommit (any vote would only have been accepted if it was targeting a
    // block higher or equal to the round base)
    let base = match valid_precommits
        .iter()
        .map(|signed| &signed.precommit)
        .min_by_key(|precommit| precommit.target_number)
        .map(|precommit| (precommit.target_hash.clone(), precommit.target_number))
    {
        None => return Ok(validation_result),
        Some(base) => base,
    };

    // check that all precommits are for blocks that are equal to or descendants
    // of the round base
    let all_precommits_higher_than_base = valid_precommits.iter().all(|signed| {
        chain.is_equal_or_descendent_of(base.0.clone(), signed.precommit.target_hash.clone())
    });

    if !all_precommits_higher_than_base {
        return Ok(validation_result);
    }

    let mut equivocated = std::collections::BTreeSet::new();

    // add all precommits to the round with correct counting logic
    let mut round = round::Round::new(round::RoundParams {
        round_number: 0, // doesn't matter here.
        voters: voters.clone(),
        base,
    });

    for SignedPrecommit {
        precommit,
        id,
        signature,
    } in &valid_precommits
    {
        match round.import_precommit(chain, precommit.clone(), id.clone(), signature.clone())? {
            ImportResult {
                equivocation: Some(_),
                ..
            } => {
                validation_result.num_equivocations += 1;
                // allow only one equivocation per voter, as extras are redundant.
                if !equivocated.insert(id) {
                    return Ok(validation_result);
                }
            }
            ImportResult { duplicated, .. } => {
                if duplicated {
                    validation_result.num_duplicated_precommits += 1;
                }
            }
        }
    }

    // for the commit to be valid, then a precommit ghost must be found for the
    // round and it must be equal to the commit target
    match round.precommit_ghost() {
        Some((precommit_ghost_hash, precommit_ghost_number))
            if precommit_ghost_hash == commit.target_hash
                && precommit_ghost_number == commit.target_number =>
        {
            validation_result.valid = true;
        }
        _ => {}
    }

    Ok(validation_result)
}

/// Runs the callback with the appropriate `CommitProcessingOutcome` based on
/// the given `CommitValidationResult`. Outcome is bad if ghost is undefined,
/// good otherwise.
#[cfg(feature = "std")]
pub fn process_commit_validation_result(
    validation_result: CommitValidationResult,
    mut callback: voter::Callback<voter::CommitProcessingOutcome>,
) {
    if validation_result.is_valid() {
        callback.run(voter::CommitProcessingOutcome::Good(
            voter::GoodCommit::new(),
        ))
    } else {
        callback.run(voter::CommitProcessingOutcome::Bad(voter::BadCommit::from(
            validation_result,
        )))
    }
}

/// Historical votes seen in a round.
#[derive(Default, Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "derive-codec", derive(Encode, Decode, TypeInfo))]
pub struct HistoricalVotes<H, N, S, Id> {
    seen: Vec<SignedMessage<H, N, S, Id>>,
    prevote_idx: Option<u64>,
    precommit_idx: Option<u64>,
}

impl<H, N, S, Id> HistoricalVotes<H, N, S, Id> {
    /// Create a new HistoricalVotes.
    pub fn new() -> Self {
        HistoricalVotes {
            seen: Vec::new(),
            prevote_idx: None,
            precommit_idx: None,
        }
    }

    /// Create a new HistoricalVotes initialized from the parameters.
    pub fn new_with(
        seen: Vec<SignedMessage<H, N, S, Id>>,
        prevote_idx: Option<u64>,
        precommit_idx: Option<u64>,
    ) -> Self {
        HistoricalVotes {
            seen,
            prevote_idx,
            precommit_idx,
        }
    }

    /// Push a vote into the list. The value of `self` before this call
    /// is considered to be a prefix of the value post-call.
    pub fn push_vote(&mut self, msg: SignedMessage<H, N, S, Id>) {
        self.seen.push(msg)
    }

    /// Return the messages seen so far.
    pub fn seen(&self) -> &[SignedMessage<H, N, S, Id>] {
        &self.seen
    }

    /// Return the number of messages seen before prevoting.
    /// None in case we didn't prevote yet.
    pub fn prevote_idx(&self) -> Option<u64> {
        self.prevote_idx
    }

    /// Return the number of messages seen before precommiting.
    /// None in case we didn't precommit yet.
    pub fn precommit_idx(&self) -> Option<u64> {
        self.precommit_idx
    }

    /// Set the number of messages seen before prevoting.
    pub fn set_prevoted_idx(&mut self) {
        self.prevote_idx = Some(self.seen.len() as u64)
    }

    /// Set the number of messages seen before precommiting.
    pub fn set_precommitted_idx(&mut self) {
        self.precommit_idx = Some(self.seen.len() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::chain::{DummyChain, GENESIS_HASH};

    #[cfg(feature = "derive-codec")]
    #[test]
    fn codec_was_derived() {
        use parity_scale_codec::{Decode, Encode};

        let signed = crate::SignedMessage {
            message: crate::Message::Prevote(crate::Prevote {
                target_hash: b"Hello".to_vec(),
                target_number: 5,
            }),
            signature: b"Signature".to_vec(),
            id: 5000,
        };

        let encoded = signed.encode();
        let signed2 = crate::SignedMessage::decode(&mut &encoded[..]).unwrap();
        assert_eq!(signed, signed2);
    }

    #[test]
    fn commit_validation() {
        let mut chain = DummyChain::new();
        chain.push_blocks(GENESIS_HASH, &["A"]);

        let voters = VoterSet::new((1..=100).map(|id| (id, 1))).unwrap();

        let make_precommit = |target_hash, target_number, id| SignedPrecommit {
            precommit: Precommit {
                target_hash,
                target_number,
            },
            id,
            signature: (),
        };

        let mut precommits = Vec::new();
        for id in 1..67 {
            let precommit = make_precommit("C", 3, id);
            precommits.push(precommit);
        }

        // we have still not reached threshold with 66/100 votes, so the commit
        // is not valid.
        let result = validate_commit(
            &Commit {
                target_hash: "C",
                target_number: 3,
                precommits: precommits.clone(),
            },
            &voters,
            &chain,
        )
        .unwrap();

        assert!(!result.is_valid());

        // after adding one more commit targeting the same block we are over
        // the finalization threshold and the commit should be valid
        precommits.push(make_precommit("C", 3, 67));

        let result = validate_commit(
            &Commit {
                target_hash: "C",
                target_number: 3,
                precommits: precommits.clone(),
            },
            &voters,
            &chain,
        )
        .unwrap();

        assert!(result.is_valid());

        // the commit target must be the exact same as the round precommit ghost
        // that is calculated with the given precommits for the commit to be valid
        let result = validate_commit(
            &Commit {
                target_hash: "B",
                target_number: 2,
                precommits: precommits.clone(),
            },
            &voters,
            &chain,
        )
        .unwrap();

        assert!(!result.is_valid());
    }

    #[test]
    fn commit_validation_with_equivocation() {
        let mut chain = DummyChain::new();
        chain.push_blocks(GENESIS_HASH, &["A", "B", "C"]);

        let voters = VoterSet::new((1..=100).map(|id| (id, 1))).unwrap();

        let make_precommit = |target_hash, target_number, id| SignedPrecommit {
            precommit: Precommit {
                target_hash,
                target_number,
            },
            id,
            signature: (),
        };

        // we add 66/100 precommits targeting block C
        let mut precommits = Vec::new();
        for id in 1..67 {
            let precommit = make_precommit("C", 3, id);
            precommits.push(precommit);
        }

        // we then add two equivocated votes targeting A and B
        // from the 67th validator
        precommits.push(make_precommit("A", 1, 67));
        precommits.push(make_precommit("B", 2, 67));

        // this equivocation is treated as "voting for all blocks", which means
        // that block C will now have 67/100 votes and therefore it can be
        // finalized.
        let result = validate_commit(
            &Commit {
                target_hash: "C",
                target_number: 3,
                precommits: precommits.clone(),
            },
            &voters,
            &chain,
        )
        .unwrap();

        assert!(result.is_valid());
        assert_eq!(result.num_equivocations(), 1);
    }

    #[test]
    fn commit_validation_precommit_from_unknown_voter_is_ignored() {
        let mut chain = DummyChain::new();
        chain.push_blocks(GENESIS_HASH, &["A", "B", "C"]);

        let voters = VoterSet::new((1..=100).map(|id| (id, 1))).unwrap();

        let make_precommit = |target_hash, target_number, id| SignedPrecommit {
            precommit: Precommit {
                target_hash,
                target_number,
            },
            id,
            signature: (),
        };

        let mut precommits = Vec::new();

        // invalid vote from unknown voter should not influence the base
        precommits.push(make_precommit("Z", 1, 1000));

        for id in 1..=67 {
            let precommit = make_precommit("C", 3, id);
            precommits.push(precommit);
        }

        let result = validate_commit(
            &Commit {
                target_hash: "C",
                target_number: 3,
                precommits: precommits.clone(),
            },
            &voters,
            &chain,
        )
        .unwrap();

        // we have threshold votes for block "C" so it should be valid
        assert!(result.is_valid());

        // there is one invalid voter in the commit
        assert_eq!(result.num_invalid_voters(), 1);
    }
}
