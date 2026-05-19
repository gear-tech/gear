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

//! Logic for a single round of GRANDPA.

mod context;

use context::{Context, Vote, VoteNode};

#[cfg(feature = "derive-codec")]
use parity_scale_codec::{Decode, Encode};

use crate::{
    std::{
        self,
        collections::btree_map::{BTreeMap, Entry},
        fmt,
        vec::Vec,
    },
    vote_graph::VoteGraph,
    voter_set::{VoterInfo, VoterSet},
    weights::{VoteWeight, VoterWeight},
};

use super::{
    BlockNumberOps, Chain, Equivocation, HistoricalVotes, Message, Precommit, Prevote,
    SignedMessage,
};

/// The (voting) phases of a round, each corresponding to the type of
/// votes cast in that phase.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum Phase {
    /// The prevote phase in which [`Prevote`]s are cast.
    Prevote,
    /// The precommit phase in which [`Precommit`]s are cast.
    Precommit,
}

/// The observed vote from a single voter.
enum VoteMultiplicity<Vote, Signature> {
    /// A single vote has been observed from the voter.
    Single(Vote, Signature),
    /// At least two votes have been observed from the voter,
    /// i.e. an equivocation.
    Equivocated((Vote, Signature), (Vote, Signature)),
}

impl<Vote: Eq, Signature: Eq> VoteMultiplicity<Vote, Signature> {
    fn contains(&self, vote: &Vote, signature: &Signature) -> bool {
        match self {
            VoteMultiplicity::Single(v, s) => v == vote && s == signature,
            VoteMultiplicity::Equivocated((v1, s1), (v2, s2)) => {
                v1 == vote && s1 == signature || v2 == vote && s2 == signature
            }
        }
    }
}

struct VoteTracker<Id: Ord + Eq, Vote, Signature> {
    votes: BTreeMap<Id, VoteMultiplicity<Vote, Signature>>,
    current_weight: VoteWeight,
}

/// Result of adding a vote.
pub(crate) struct AddVoteResult<'a, Vote, Signature> {
    multiplicity: Option<&'a VoteMultiplicity<Vote, Signature>>,
    duplicated: bool,
}

impl<Id: Ord + Eq + Clone, Vote: Clone + Eq, Signature: Clone + Eq>
    VoteTracker<Id, Vote, Signature>
{
    fn new() -> Self {
        VoteTracker {
            votes: BTreeMap::new(),
            current_weight: VoteWeight(0),
        }
    }

    // track a vote, returning a value containing the multiplicity of all votes from this ID
    // and a bool indicating if the vote is duplicated.
    // if the vote is the first equivocation, returns a value indicating
    // it as such (the new vote is always the last in the multiplicity).
    //
    // if the vote is a further equivocation, it is ignored and there is nothing
    // to do.
    //
    // since this struct doesn't track the round-number of votes, that must be set
    // by the caller.
    fn add_vote(
        &mut self,
        id: Id,
        vote: Vote,
        signature: Signature,
        weight: VoterWeight,
    ) -> AddVoteResult<Vote, Signature> {
        match self.votes.entry(id) {
            Entry::Vacant(vacant) => {
                self.current_weight = self.current_weight + weight;
                let multiplicity = vacant.insert(VoteMultiplicity::Single(vote, signature));

                AddVoteResult {
                    multiplicity: Some(multiplicity),
                    duplicated: false,
                }
            }
            Entry::Occupied(mut occupied) => {
                if occupied.get().contains(&vote, &signature) {
                    return AddVoteResult {
                        multiplicity: None,
                        duplicated: true,
                    };
                }

                // import, but ignore further equivocations.
                let new_val = match *occupied.get_mut() {
                    VoteMultiplicity::Single(ref v, ref s) => {
                        VoteMultiplicity::Equivocated((v.clone(), s.clone()), (vote, signature))
                    }
                    VoteMultiplicity::Equivocated(_, _) => {
                        return AddVoteResult {
                            multiplicity: None,
                            duplicated: false,
                        }
                    }
                };

                *occupied.get_mut() = new_val;

                AddVoteResult {
                    multiplicity: Some(&*occupied.into_mut()),
                    duplicated: false,
                }
            }
        }
    }

    // Returns all imported votes.
    fn votes(&self) -> Vec<(Id, Vote, Signature)> {
        let mut votes = Vec::new();

        for (id, vote) in &self.votes {
            match vote {
                VoteMultiplicity::Single(v, s) => votes.push((id.clone(), v.clone(), s.clone())),
                VoteMultiplicity::Equivocated((v1, s1), (v2, s2)) => {
                    votes.push((id.clone(), v1.clone(), s1.clone()));
                    votes.push((id.clone(), v2.clone(), s2.clone()));
                }
            }
        }

        votes
    }

    // Current vote weight and number of participants.
    fn participation(&self) -> (VoteWeight, usize) {
        (self.current_weight, self.votes.len())
    }
}

/// State of the round.
#[derive(PartialEq, Clone, Debug)]
#[cfg_attr(feature = "derive-codec", derive(Encode, Decode, scale_info::TypeInfo))]
pub struct State<H, N> {
    /// The prevote-GHOST block.
    pub prevote_ghost: Option<(H, N)>,
    /// The finalized block.
    pub finalized: Option<(H, N)>,
    /// The new round-estimate.
    pub estimate: Option<(H, N)>,
    /// Whether the round is completable.
    pub completable: bool,
}

impl<H: Clone, N: Clone> State<H, N> {
    /// Genesis state.
    pub fn genesis(genesis: (H, N)) -> Self {
        State {
            prevote_ghost: Some(genesis.clone()),
            finalized: Some(genesis.clone()),
            estimate: Some(genesis),
            completable: true,
        }
    }
}

/// Parameters for starting a round.
pub struct RoundParams<Id: Ord + Eq, H, N> {
    /// The round number for votes.
    pub round_number: u64,
    /// Actors and weights in the round.
    pub voters: VoterSet<Id>,
    /// The base block to build on.
    pub base: (H, N),
}

/// Stores data for a round.
pub struct Round<Id: Ord + Eq, H: Ord + Eq, N, Signature> {
    round_number: u64,
    context: Context<Id>,
    graph: VoteGraph<H, N, VoteNode>, // DAG of blocks which have been voted on.
    prevote: VoteTracker<Id, Prevote<H, N>, Signature>, // tracks prevotes that have been counted
    precommit: VoteTracker<Id, Precommit<H, N>, Signature>, // tracks precommits
    historical_votes: HistoricalVotes<H, N, Signature, Id>,
    prevote_ghost: Option<(H, N)>, // current memoized prevote-GHOST block
    precommit_ghost: Option<(H, N)>, // current memoized precommit-GHOST block
    finalized: Option<(H, N)>,     // best finalized block in this round.
    estimate: Option<(H, N)>,      // current memoized round-estimate
    completable: bool,             // whether the round is completable
}

/// Result of importing a Prevote or Precommit.
pub(crate) struct ImportResult<Id, P, Signature> {
    /// Indicates if the voter is part of the voter set.
    pub(crate) valid_voter: bool,
    /// Indicates if the vote is duplicated.
    pub(crate) duplicated: bool,
    /// An equivocation proof, if the vote is an equivocation.
    pub(crate) equivocation: Option<Equivocation<Id, P, Signature>>,
}

impl<Id, P, Signature> Default for ImportResult<Id, P, Signature> {
    fn default() -> Self {
        ImportResult {
            valid_voter: false,
            duplicated: false,
            equivocation: None,
        }
    }
}

impl<Id, H, N, Signature> Round<Id, H, N, Signature>
where
    Id: Ord + Clone + Eq + fmt::Debug,
    H: Ord + Clone + Eq + Ord + fmt::Debug,
    N: Copy + fmt::Debug + BlockNumberOps,
    Signature: Eq + Clone,
{
    /// Create a new round accumulator for given round number and with given weight.
    pub fn new(round_params: RoundParams<Id, H, N>) -> Self {
        let (base_hash, base_number) = round_params.base;

        Round {
            round_number: round_params.round_number,
            context: Context::new(round_params.voters),
            graph: VoteGraph::new(base_hash, base_number, VoteNode::default()),
            prevote: VoteTracker::new(),
            precommit: VoteTracker::new(),
            historical_votes: HistoricalVotes::new(),
            prevote_ghost: None,
            precommit_ghost: None,
            finalized: None,
            estimate: None,
            completable: false,
        }
    }

    /// Return the round number.
    pub fn number(&self) -> u64 {
        self.round_number
    }

    /// Import a prevote. Returns an equivocation proof, if the vote is an equivocation,
    /// and a bool indicating if the vote is duplicated (see `ImportResult`).
    ///
    /// Ignores duplicate prevotes (not equivocations).
    #[cfg_attr(not(feature = "std"), allow(unused))]
    pub(crate) fn import_prevote<C: Chain<H, N>>(
        &mut self,
        chain: &C,
        prevote: Prevote<H, N>,
        signer: Id,
        signature: Signature,
    ) -> Result<ImportResult<Id, Prevote<H, N>, Signature>, crate::Error> {
        let mut import_result = ImportResult::default();

        let info = match self.context.voters().get(&signer) {
            Some(info) => info.clone(),
            None => return Ok(import_result),
        };

        import_result.valid_voter = true;
        let weight = info.weight();

        let equivocation = {
            let multiplicity = match self.prevote.add_vote(
                signer.clone(),
                prevote.clone(),
                signature.clone(),
                weight,
            ) {
                AddVoteResult {
                    multiplicity: Some(m),
                    ..
                } => m,
                AddVoteResult { duplicated, .. } => {
                    import_result.duplicated = duplicated;
                    return Ok(import_result);
                }
            };
            let round_number = self.round_number;

            match multiplicity {
                VoteMultiplicity::Single(single_vote, _) => {
                    let vote = Vote::new(&info, Phase::Prevote);

                    self.graph.insert(
                        single_vote.target_hash.clone(),
                        single_vote.target_number,
                        vote,
                        chain,
                    )?;

                    // Push the vote into HistoricalVotes.
                    let message = Message::Prevote(prevote);
                    let signed_message = SignedMessage {
                        id: signer,
                        signature,
                        message,
                    };
                    self.historical_votes.push_vote(signed_message);

                    None
                }
                VoteMultiplicity::Equivocated(ref first, ref second) => {
                    // mark the equivocator as such. no need to "undo" the first vote.
                    self.context.equivocated(&info, Phase::Prevote);

                    // Push the vote into HistoricalVotes.
                    let message = Message::Prevote(prevote);
                    let signed_message = SignedMessage {
                        id: signer.clone(),
                        signature,
                        message,
                    };
                    self.historical_votes.push_vote(signed_message);

                    Some(Equivocation {
                        round_number,
                        identity: signer,
                        first: first.clone(),
                        second: second.clone(),
                    })
                }
            }
        };

        // update prevote-GHOST
        let threshold = self.threshold();
        if self.prevote.current_weight >= threshold {
            self.prevote_ghost = self.graph.find_ghost(self.prevote_ghost.take(), |v| {
                self.context.weight(v, Phase::Prevote) >= threshold
            });
        }

        self.update();
        import_result.equivocation = equivocation;
        Ok(import_result)
    }

    /// Import a precommit. Returns an equivocation proof, if the vote is an
    /// equivocation, and a bool indicating if the vote is duplicated (see `ImportResult`).
    ///
    /// Ignores duplicate precommits (not equivocations).
    pub(crate) fn import_precommit<C: Chain<H, N>>(
        &mut self,
        chain: &C,
        precommit: Precommit<H, N>,
        signer: Id,
        signature: Signature,
    ) -> Result<ImportResult<Id, Precommit<H, N>, Signature>, crate::Error> {
        let mut import_result = ImportResult::default();

        let info = match self.context.voters().get(&signer) {
            Some(info) => info.clone(),
            None => return Ok(import_result),
        };
        import_result.valid_voter = true;
        let weight = info.weight();

        let equivocation = {
            let multiplicity = match self.precommit.add_vote(
                signer.clone(),
                precommit.clone(),
                signature.clone(),
                weight,
            ) {
                AddVoteResult {
                    multiplicity: Some(m),
                    ..
                } => m,
                AddVoteResult { duplicated, .. } => {
                    import_result.duplicated = duplicated;
                    return Ok(import_result);
                }
            };

            let round_number = self.round_number;

            match multiplicity {
                VoteMultiplicity::Single(single_vote, _) => {
                    let vote = Vote::new(&info, Phase::Precommit);

                    self.graph.insert(
                        single_vote.target_hash.clone(),
                        single_vote.target_number,
                        vote,
                        chain,
                    )?;

                    let message = Message::Precommit(precommit);
                    let signed_message = SignedMessage {
                        id: signer,
                        signature,
                        message,
                    };
                    self.historical_votes.push_vote(signed_message);

                    None
                }
                VoteMultiplicity::Equivocated(ref first, ref second) => {
                    // mark the equivocator as such. no need to "undo" the first vote.
                    self.context.equivocated(&info, Phase::Precommit);

                    // Push the vote into HistoricalVotes.
                    let message = Message::Precommit(precommit);
                    let signed_message = SignedMessage {
                        id: signer.clone(),
                        signature,
                        message,
                    };
                    self.historical_votes.push_vote(signed_message);

                    Some(Equivocation {
                        round_number,
                        identity: signer,
                        first: first.clone(),
                        second: second.clone(),
                    })
                }
            }
        };

        self.update();
        import_result.equivocation = equivocation;
        Ok(import_result)
    }

    /// Return the current state.
    pub fn state(&self) -> State<H, N> {
        State {
            prevote_ghost: self.prevote_ghost.clone(),
            finalized: self.finalized.clone(),
            estimate: self.estimate.clone(),
            completable: self.completable,
        }
    }

    /// Compute and cache the precommit-GHOST.
    pub fn precommit_ghost(&mut self) -> Option<(H, N)> {
        // update precommit-GHOST
        let threshold = self.threshold();
        if self.precommit.current_weight >= threshold {
            self.precommit_ghost = self.graph.find_ghost(self.precommit_ghost.take(), |v| {
                self.context.weight(v, Phase::Precommit) >= threshold
            });
        }

        self.precommit_ghost.clone()
    }

    /// Returns an iterator of all precommits targeting the finalized hash.
    ///
    /// Only returns `None` if no block has been finalized in this round.
    pub fn finalizing_precommits<'a, C: 'a + Chain<H, N>>(
        &'a mut self,
        chain: &'a C,
    ) -> Option<impl Iterator<Item = crate::SignedPrecommit<H, N, Signature, Id>> + 'a> {
        struct YieldVotes<'b, V: 'b, S: 'b> {
            yielded: usize,
            multiplicity: &'b VoteMultiplicity<V, S>,
        }

        impl<'b, V: 'b + Clone, S: 'b + Clone> Iterator for YieldVotes<'b, V, S> {
            type Item = (V, S);

            fn next(&mut self) -> Option<(V, S)> {
                match self.multiplicity {
                    VoteMultiplicity::Single(ref v, ref s) => {
                        if self.yielded == 0 {
                            self.yielded += 1;
                            Some((v.clone(), s.clone()))
                        } else {
                            None
                        }
                    }
                    VoteMultiplicity::Equivocated(ref a, ref b) => {
                        let res = match self.yielded {
                            0 => Some(a.clone()),
                            1 => Some(b.clone()),
                            _ => None,
                        };

                        self.yielded += 1;
                        res
                    }
                }
            }
        }

        let (f_hash, _f_num) = self.finalized.clone()?;
        let find_valid_precommits = self
            .precommit
            .votes
            .iter()
            .filter(move |&(_id, multiplicity)| {
                if let VoteMultiplicity::Single(ref v, _) = *multiplicity {
                    // if there is a single vote from this voter, we only include it
                    // if it branches off of the target.
                    chain.is_equal_or_descendent_of(f_hash.clone(), v.target_hash.clone())
                } else {
                    // equivocations count for everything, so we always include them.
                    true
                }
            })
            .flat_map(|(id, multiplicity)| {
                let yield_votes = YieldVotes {
                    yielded: 0,
                    multiplicity,
                };

                yield_votes.map(move |(v, s)| crate::SignedPrecommit {
                    precommit: v,
                    signature: s,
                    id: id.clone(),
                })
            });

        Some(find_valid_precommits)
    }

    // update the round-estimate and whether the round is completable.
    fn update(&mut self) {
        let threshold = self.threshold();

        if self.prevote.current_weight < threshold {
            return;
        }

        let (g_hash, g_num) = match self.prevote_ghost.clone() {
            None => return,
            Some(x) => x,
        };

        let ctx = &self.context;

        // anything new finalized? finalized blocks are those which have both
        // 2/3+ prevote and precommit weight.
        let current_precommits = self.precommit.current_weight;
        if current_precommits >= self.threshold() {
            self.finalized = self.graph.find_ancestor(g_hash.clone(), g_num, |v| {
                ctx.weight(v, Phase::Precommit) >= threshold
            });
        };

        // figuring out whether a block can still be committed for is
        // not straightforward because we have to account for all possible future
        // equivocations and thus cannot discount weight from validators who
        // have already voted.
        let possible_to_precommit = {
            // find how many more equivocations we could still get.
            //
            // it is only important to consider the voters whose votes
            // we have already seen, because we are assuming any votes we
            // haven't seen will target this block.
            let tolerated_equivocations = ctx.voters().total_weight() - threshold;
            let current_equivocations = ctx.equivocation_weight(Phase::Precommit);
            let additional_equiv = tolerated_equivocations - current_equivocations;
            let remaining_commit_votes =
                ctx.voters().total_weight() - self.precommit.current_weight;

            move |node: &VoteNode| {
                // total precommits for this block, including equivocations.
                let precommitted_for = ctx.weight(node, Phase::Precommit);

                // equivocations we could still get are out of those who
                // have already voted, but not on this block.
                let possible_equivocations =
                    std::cmp::min(current_precommits - precommitted_for, additional_equiv);

                // all the votes already applied on this block,
                // assuming all remaining actors commit to this block,
                // and that we get further equivocations
                let full_possible_weight =
                    precommitted_for + remaining_commit_votes + possible_equivocations;

                full_possible_weight >= threshold
            }
        };

        // until we have threshold precommits, any new block could get supermajority
        // precommits because there are at least f + 1 precommits remaining and then
        // f equivocations.
        //
        // once it's at least that level, we only need to consider blocks
        // already referenced in the graph, because no new leaf nodes
        // could ever have enough precommits.
        //
        // the round-estimate is the highest block in the chain with head
        // `prevote_ghost` that could have supermajority-commits.
        if self.precommit.current_weight >= threshold {
            self.estimate = self
                .graph
                .find_ancestor(g_hash.clone(), g_num, possible_to_precommit);
        } else {
            self.estimate = Some((g_hash, g_num));
            return;
        }

        self.completable = self.estimate.clone().is_some_and(|(b_hash, b_num)| {
            b_hash != g_hash || {
                // round-estimate is the same as the prevote-ghost.
                // this round is still completable if no further blocks
                // could have commit-supermajority.
                self.graph
                    .find_ghost(Some((b_hash, b_num)), possible_to_precommit)
                    .map_or(true, |x| x == (g_hash, g_num))
            }
        })
    }

    /// Fetch the "round-estimate": the best block which might have been finalized
    /// in this round.
    ///
    /// Returns `None` when new new blocks could have been finalized in this round,
    /// according to our estimate.
    pub fn estimate(&self) -> Option<&(H, N)> {
        self.estimate.as_ref()
    }

    /// Fetch the most recently finalized block.
    pub fn finalized(&self) -> Option<&(H, N)> {
        self.finalized.as_ref()
    }

    /// Returns `true` when the round is completable.
    ///
    /// This is the case when the round-estimate is an ancestor of the prevote-ghost head,
    /// or when they are the same block _and_ none of its children could possibly have
    /// enough precommits.
    pub fn completable(&self) -> bool {
        self.completable
    }

    /// Threshold weight for supermajority.
    pub fn threshold(&self) -> VoterWeight {
        self.context.voters().threshold()
    }

    /// Return the round base.
    pub fn base(&self) -> (H, N) {
        self.graph.base()
    }

    /// Return the round voters and weights.
    pub fn voters(&self) -> &VoterSet<Id> {
        self.context.voters()
    }

    /// Return the primary voter of the round.
    pub fn primary_voter(&self) -> (&Id, &VoterInfo) {
        self.context.voters().nth_mod(self.round_number as usize)
    }

    /// Get the current weight and number of voters who have participated in prevoting.
    pub fn prevote_participation(&self) -> (VoteWeight, usize) {
        self.prevote.participation()
    }

    /// Get the current weight and number of voters who have participated in precommitting.
    pub fn precommit_participation(&self) -> (VoteWeight, usize) {
        self.precommit.participation()
    }

    /// Return all imported prevotes.
    pub fn prevotes(&self) -> Vec<(Id, Prevote<H, N>, Signature)> {
        self.prevote.votes()
    }

    /// Return all imported precommits.
    pub fn precommits(&self) -> Vec<(Id, Precommit<H, N>, Signature)> {
        self.precommit.votes()
    }

    /// Return all votes for the round (prevotes and precommits), sorted by
    /// imported order and indicating the indices where we voted. At most two
    /// prevotes and two precommits per voter are present, further equivocations
    /// are not stored (as they are redundant).
    pub fn historical_votes(&self) -> &HistoricalVotes<H, N, Signature, Id> {
        &self.historical_votes
    }

    /// Set the number of prevotes and precommits received at the moment of prevoting.
    /// It should be called immediately after prevoting.
    pub fn set_prevoted_index(&mut self) {
        self.historical_votes.set_prevoted_idx()
    }

    /// Set the number of prevotes and precommits received at the moment of precommiting.
    /// It should be called immediately after precommiting.
    pub fn set_precommitted_index(&mut self) {
        self.historical_votes.set_precommitted_idx()
    }

    /// Get the number of prevotes and precommits received at the moment of prevoting.
    /// Returns None if the prevote wasn't realized.
    pub fn prevoted_index(&self) -> Option<u64> {
        self.historical_votes.prevote_idx
    }

    /// Get the number of prevotes and precommits received at the moment of precommiting.
    /// Returns None if the precommit wasn't realized.
    pub fn precommitted_index(&self) -> Option<u64> {
        self.historical_votes.precommit_idx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::chain::{DummyChain, GENESIS_HASH};

    fn voters() -> VoterSet<&'static str> {
        VoterSet::new([("Alice", 4), ("Bob", 7), ("Eve", 3)].iter().cloned()).expect("nonempty")
    }

    #[derive(PartialEq, Eq, Clone, Debug)]
    struct Signature(&'static str);

    #[test]
    fn estimate_is_valid() {
        let mut chain = DummyChain::new();
        chain.push_blocks(GENESIS_HASH, &["A", "B", "C", "D", "E", "F"]);
        chain.push_blocks("E", &["EA", "EB", "EC", "ED"]);
        chain.push_blocks("F", &["FA", "FB", "FC"]);

        let mut round = Round::new(RoundParams {
            round_number: 1,
            voters: voters(),
            base: ("C", 4),
        });

        round
            .import_prevote(&chain, Prevote::new("FC", 10), "Alice", Signature("Alice"))
            .unwrap();

        round
            .import_prevote(&chain, Prevote::new("ED", 10), "Bob", Signature("Bob"))
            .unwrap();

        assert_eq!(round.prevote_ghost, Some(("E", 6)));
        assert_eq!(round.estimate(), Some(&("E", 6)));
        assert!(!round.completable());

        round
            .import_prevote(&chain, Prevote::new("F", 7), "Eve", Signature("Eve"))
            .unwrap();

        assert_eq!(round.prevote_ghost, Some(("E", 6)));
        assert_eq!(round.estimate(), Some(&("E", 6)));
    }

    #[test]
    fn finalization() {
        let mut chain = DummyChain::new();
        chain.push_blocks(GENESIS_HASH, &["A", "B", "C", "D", "E", "F"]);
        chain.push_blocks("E", &["EA", "EB", "EC", "ED"]);
        chain.push_blocks("F", &["FA", "FB", "FC"]);

        let mut round = Round::new(RoundParams {
            round_number: 1,
            voters: voters(),
            base: ("C", 4),
        });

        round
            .import_precommit(
                &chain,
                Precommit::new("FC", 10),
                "Alice",
                Signature("Alice"),
            )
            .unwrap();

        round
            .import_precommit(&chain, Precommit::new("ED", 10), "Bob", Signature("Bob"))
            .unwrap();

        assert_eq!(round.finalized, None);

        // import some prevotes.
        {
            round
                .import_prevote(&chain, Prevote::new("FC", 10), "Alice", Signature("Alice"))
                .unwrap();

            round
                .import_prevote(&chain, Prevote::new("ED", 10), "Bob", Signature("Bob"))
                .unwrap();

            round
                .import_prevote(&chain, Prevote::new("EA", 7), "Eve", Signature("Eve"))
                .unwrap();

            assert_eq!(round.finalized, Some(("E", 6)));
        }

        round
            .import_precommit(&chain, Precommit::new("EA", 7), "Eve", Signature("Eve"))
            .unwrap();

        assert_eq!(round.finalized, Some(("EA", 7)));
    }

    #[test]
    fn equivocate_does_not_double_count() {
        let mut chain = DummyChain::new();
        chain.push_blocks(GENESIS_HASH, &["A", "B", "C", "D", "E", "F"]);
        chain.push_blocks("E", &["EA", "EB", "EC", "ED"]);
        chain.push_blocks("F", &["FA", "FB", "FC"]);

        let mut round = Round::new(RoundParams {
            round_number: 1,
            voters: voters(),
            base: ("C", 4),
        });

        // first prevote by eve
        assert!(round
            .import_prevote(
                &chain,
                Prevote::new("FC", 10),
                "Eve", // 3 on F, E
                Signature("Eve-1"),
            )
            .unwrap()
            .equivocation
            .is_none());

        assert!(round.prevote_ghost.is_none());

        // second prevote by eve: comes with equivocation proof
        assert!(round
            .import_prevote(
                &chain,
                Prevote::new("ED", 10),
                "Eve", // still 3 on E
                Signature("Eve-2"),
            )
            .unwrap()
            .equivocation
            .is_some());

        // third prevote: returns nothing.
        assert!(round
            .import_prevote(
                &chain,
                Prevote::new("F", 7),
                "Eve", // still 3 on F and E
                Signature("Eve-2"),
            )
            .unwrap()
            .equivocation
            .is_none());

        // three eves together would be enough.

        assert!(round.prevote_ghost.is_none());

        assert!(round
            .import_prevote(
                &chain,
                Prevote::new("FA", 8),
                "Bob", // add 7 to FA and you get FA.
                Signature("Bob-1"),
            )
            .unwrap()
            .equivocation
            .is_none());

        assert_eq!(round.prevote_ghost, Some(("FA", 8)));
    }

    #[test]
    fn historical_votes_works() {
        let mut chain = DummyChain::new();
        chain.push_blocks(GENESIS_HASH, &["A", "B", "C", "D", "E", "F"]);
        chain.push_blocks("E", &["EA", "EB", "EC", "ED"]);
        chain.push_blocks("F", &["FA", "FB", "FC"]);

        let mut round = Round::new(RoundParams {
            round_number: 1,
            voters: voters(),
            base: ("C", 4),
        });

        round
            .import_prevote(&chain, Prevote::new("FC", 10), "Alice", Signature("Alice"))
            .unwrap();

        round.set_prevoted_index();

        round
            .import_prevote(&chain, Prevote::new("EA", 7), "Eve", Signature("Eve"))
            .unwrap();

        round
            .import_precommit(&chain, Precommit::new("EA", 7), "Eve", Signature("Eve"))
            .unwrap();

        round
            .import_prevote(&chain, Prevote::new("EC", 10), "Alice", Signature("Alice"))
            .unwrap();

        round.set_precommitted_index();

        assert_eq!(
            round.historical_votes(),
            &HistoricalVotes::new_with(
                vec![
                    SignedMessage {
                        message: Message::Prevote(Prevote {
                            target_hash: "FC",
                            target_number: 10
                        }),
                        signature: Signature("Alice"),
                        id: "Alice"
                    },
                    SignedMessage {
                        message: Message::Prevote(Prevote {
                            target_hash: "EA",
                            target_number: 7
                        }),
                        signature: Signature("Eve"),
                        id: "Eve"
                    },
                    SignedMessage {
                        message: Message::Precommit(Precommit {
                            target_hash: "EA",
                            target_number: 7
                        }),
                        signature: Signature("Eve"),
                        id: "Eve"
                    },
                    SignedMessage {
                        message: Message::Prevote(Prevote {
                            target_hash: "EC",
                            target_number: 10
                        }),
                        signature: Signature("Alice"),
                        id: "Alice"
                    },
                ],
                Some(1),
                Some(4),
            )
        );
    }
}
