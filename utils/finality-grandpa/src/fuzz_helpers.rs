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

//! Fuzzing utilities for the vote graph.

use crate::{
    round::{Round, RoundParams},
    vote_graph::VoteGraph,
    voter_set::VoterSet,
    Chain, Error, Precommit, Prevote,
};

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

type Voter = u8;
type Hash = u8;
type BlockNumber = u8;
type Signature = u8;
type Block = (Hash, BlockNumber);

/// The fuzzing chain is made of 16 blocks, including the genesis.
/// The genesis is 0. each block can be distinguished by a 4-bit number.
///
/// Parent -> Children
/// 0      -> [1, 2, 3]
/// 1      -> [4, 5, 6]
/// 2      -> [7, 8, 9]
/// 4      -> [10, 11, 12]
/// 7      -> [13, 14, 15]

#[derive(Default, Clone, Copy)]
pub struct FuzzChain;

impl FuzzChain {
    fn number(hash: Hash) -> BlockNumber {
        match hash {
            0 => 0,

            1..=3 => 1,

            4..=6 => 2,
            7..=9 => 2,

            10..=12 => 3,
            13..=15 => 3,

            _ => panic!("invalid block hash"),
        }
    }

    fn children(hash: Hash) -> &'static [Hash] {
        match hash {
            0 => &[1, 2, 3],
            1 => &[4, 5, 6],
            2 => &[7, 8, 9],
            4 => &[10, 11, 12],
            7 => &[13, 14, 15],
            _ => &[],
        }
    }

    fn all_descendents(hash: Hash) -> impl Iterator<Item = Hash> {
        let children = Self::children(hash);

        struct Descendents(Vec<Hash>);
        impl Iterator for Descendents {
            type Item = Hash;

            fn next(&mut self) -> Option<Hash> {
                let next = self.0.pop()?;
                self.0.extend(FuzzChain::children(next).iter().cloned());
                Some(next)
            }
        }

        Descendents(children.to_vec())
    }
}

impl Chain<Hash, BlockNumber> for FuzzChain {
    fn ancestry(&self, base: Hash, block: Hash) -> Result<Vec<Hash>, Error> {
        // filter out bad descendents.
        match (base, block) {
            (0, x) if x <= 15 => {}

            (1, 4) => {}
            (1, 5) => {}
            (1, 6) => {}
            (1, 10) => {}
            (1, 11) => {}
            (1, 12) => {}

            (2, 7) => {}
            (2, 8) => {}
            (2, 9) => {}
            (2, 13) => {}
            (2, 14) => {}
            (2, 15) => {}

            (4, 10) => {}
            (4, 11) => {}
            (4, 12) => {}

            (7, 13) => {}
            (7, 14) => {}
            (7, 15) => {}

            _ => return Err(Error::NotDescendent),
        }

        let full_ancestry: &[Hash] = match block {
            0 => &[],
            1..=3 => &[0],
            4..=6 => &[0, 1],
            7..=9 => &[0, 2],
            10..=12 => &[0, 1, 4],
            13..=15 => &[0, 2, 7],
            _ => panic!("invalid block hash"),
        };

        Ok(full_ancestry
            .iter()
            .rev()
            .take_while(|x| **x != base)
            .cloned()
            .collect::<Vec<_>>())
    }
}

struct RandomnessStream<'a> {
    inner: &'a [u8],
    pos: usize,
    half_nibble: bool,
}

impl<'a> RandomnessStream<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            inner: data,
            pos: 0,
            half_nibble: false,
        }
    }

    fn read_nibble(&mut self) -> Option<u8> {
        let active = *self.inner.get(self.pos)?;
        if self.half_nibble {
            self.half_nibble = false;
            self.pos += 1;

            Some(active & 0x0F)
        } else {
            self.half_nibble = true;

            Some((active >> 4) & 0x0F)
        }
    }

    fn read_byte(&mut self) -> Option<u8> {
        if self.half_nibble {
            // just skip 4 bytes.
            self.half_nibble = false;
        }
        self.pos += 1;
        self.inner.get(self.pos).copied()
    }
}

fn voters() -> [Voter; 10] {
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
}

const FACTORIAL: [u32; 11] = [
    1,       // 0
    1,       // 1
    2,       // 2
    6,       // 3
    24,      // 4
    120,     // 5
    720,     // 6
    5040,    // 7
    40320,   // 8
    362880,  // 9
    3628800, // 10
];

// The number of r-combinations of n elements.
fn n_choose_r(n: u8, r: u8) -> u8 {
    assert!(r <= 7);
    assert!(n <= 10);
    assert!(n >= r);

    (FACTORIAL[n as usize] / (FACTORIAL[r as usize] * FACTORIAL[(n - r) as usize])) as u8
}

// returns the kth combination of r numbers from the first n.
//
// only works for values of n and r up to 10.
// this is used to select 6 other voters from the 10 (bumping indices after our own)
// to import prevotes from.
fn kth_combination(k: u8, n: u8, r: u8) -> Vec<u8> {
    fn r_helper(k: u8, n: u8, r: u8, off: u8, v: &mut Vec<u8>) {
        if r == 0 {
            return;
        }

        // the "tail" of the list we have here is all the elements from the offset
        // to the total number of elements.
        if n == 0 {
            v.extend((0..r).map(|x| x + off));
            return;
        }

        // how many choices there are of the remaining.
        let i = n_choose_r(n - 1, r - 1);
        if k < i {
            // first item of the list and then the k'th choice of the remainder
            v.push(off);
            r_helper(k, n - 1, r - 1, off + 1, v);
        } else {
            // choose k - i of items not including the first.
            r_helper(k - i, n - 1, r, off + 1, v);
        }
    }

    let mut v = Vec::with_capacity(r as usize);
    r_helper(k, n, r, 0, &mut v);

    v
}

/// The k-th voter combination out of all n-choose-r combinations,
/// assuming `v` to be excluded.
fn voter_combination(v: Voter, k: u8, n: u8, r: u8) -> Vec<Voter> {
    let mut others = kth_combination(k, n, r);
    for other in &mut others {
        // Assume that v was omitted.
        if *other >= v {
            *other += 1
        }
    }
    others
}

/// Check that an estimate block is monotonically decreasing.
fn check_estimate((old_hash, old_nr): Block, (new_hash, new_nr): Block) {
    assert!(
        old_hash == new_hash || (new_nr < old_nr && FuzzChain.ancestry(new_hash, old_hash).is_ok())
    );
}

/// Check that a prevote ghost block is monotonically increasing.
fn check_prevote_ghost((old_hash, old_nr): Block, (new_hash, new_nr): Block) {
    assert!(
        old_hash == new_hash || (new_nr > old_nr && FuzzChain.ancestry(old_hash, new_hash).is_ok())
    );
}

/// Choose a random block from the static block tree.
fn rand_block(s: &mut RandomnessStream) -> Option<Block> {
    s.read_nibble().map(|hash| (hash, FuzzChain::number(hash)))
}

/// Execute a fuzzed voting process on a `Round`.
pub fn execute_fuzzed_vote(data: &[u8]) {
    assert!(voters().len() <= core::u8::MAX as usize);

    let n = voters().len() as u8;
    let f = (n - 1) / 3;
    let t = n - f;

    let mut stream = RandomnessStream::new(data);

    // Initialise a round for each voter.
    let mut rounds: Vec<Round<Voter, Hash, BlockNumber, Signature>> = voters()
        .iter()
        .map(|_| {
            Round::new(RoundParams {
                round_number: 0,
                voters: VoterSet::new(voters().iter().cloned().map(|v| (v, 1))).expect("nonempty"),
                base: (0, 0),
            })
        })
        .collect();

    // Create a random list of prevotes.
    let prevotes = voters()
        .iter()
        .filter_map(|_| {
            rand_block(&mut stream).map(|(target_hash, target_number)| Prevote {
                target_hash,
                target_number,
            })
        })
        .collect::<Vec<_>>();

    if prevotes.len() != n as usize {
        // fuzzer needs to get us more data.
        return;
    }

    // Import prevotes and determine the precommit target (i.e. the prevote
    // ghost) of each voter.
    let mut precommits = Vec::with_capacity(n as usize);
    let n_combinations = n_choose_r(n - 1, t - 1);
    for (i, &voter) in voters().iter().enumerate() {
        let round = &mut rounds[i];

        // Import enough prevotes (including `voter`s) to reach supermajority.
        let k = match stream.read_byte() {
            Some(b) => b % n_combinations,
            None => return, // not enough randomness
        };
        let omit = voter_combination(voter, k, n - 1, f);
        for &j in voters().iter().filter(|j| !omit.contains(j)) {
            let vote = prevotes[j as usize].clone();
            round.import_prevote(&FuzzChain, vote, j, j).unwrap();
        }

        // Determine precommit target for `voter` (i.e. the prevote ghost).
        let (target_hash, target_number) = round
            .state()
            .prevote_ghost
            .expect("after importing threshold votes, ghost exists");
        let precommit = Precommit {
            target_hash,
            target_number,
        };
        precommits.push(precommit.clone());
    }

    // Import precommits.
    for (i, &voter) in voters().iter().enumerate() {
        let round = &mut rounds[i];

        // Import enough precommits (including our own) to reach supermajority.
        let k = match stream.read_byte() {
            Some(b) => b % n_combinations,
            None => return,
        };
        let omit = voter_combination(voter, k, n - 1, f);
        for &j in voters().iter().filter(|j| !omit.contains(j)) {
            let vote = precommits[j as usize].clone();
            round.import_precommit(&FuzzChain, vote, j, j).unwrap();
        }

        // Start tracking completability and estimate.
        let mut completable = round.state().completable;
        let mut last_estimate = round.state().estimate;

        // Import the remaining precommits.
        for j in omit {
            let vote = precommits[j as usize].clone();
            round.import_precommit(&FuzzChain, vote, j, j).unwrap();

            let new_state = round.state();

            if completable {
                // Check monotonicity of completability.
                assert!(new_state.completable);

                // Check (backwards) monotonicity of the estimate.
                let new_estimate = new_state.estimate.expect("is completable");
                let old_estimate = last_estimate.expect("is completable");
                check_estimate(old_estimate, new_estimate);
                last_estimate = Some(new_estimate);
            } else {
                completable = new_state.completable;
                last_estimate = new_state.estimate;
            }
        }

        // An estimate must always exist after importing a supermajority of precommits.
        assert!(round.state().estimate.is_some());

        // Note: Since every voter only imported a supermajority of prevotes so far,
        // but not all, the round may at this point still not be completable even
        // though all precommits have been observed, because the prevote-ghost may
        // be too far up the chain and still needs to move down by observing
        // the remaining prevotes.

        // Now (re-)import _all_ prevotes, checking the prevote-ghost along the way.
        for &v in voters().iter() {
            let old_ghost = round.state().prevote_ghost.expect("supermajority seen");

            let vote = prevotes[v as usize].clone();
            let result = round.import_prevote(&FuzzChain, vote, v, v).unwrap();

            // The prevote may be a duplicate, but never an equivocation.
            assert!(result.equivocation.is_none());

            // Check monotonicity of the prevote ghost.
            let new_ghost = round.state().prevote_ghost.expect("supermajority seen");
            check_prevote_ghost(old_ghost, new_ghost);
        }

        // After observing all prevotes and precommits, the round _MUST_
        // now be completable.
        assert!(round.state().completable);
    }
}

/// Execute a fuzzed voting process directly on a `VoteGraph`,
/// explicitly tracking the expected ghost and estimate.
pub fn execute_fuzzed_graph(data: &[u8]) {
    // 100 voters, all voting on random blocks.
    const N: u8 = 100;
    const F: u8 = (N - 1) / 3;
    const T: u8 = N - F;

    /// A vote-node on the graph.
    #[derive(Default, Clone, Debug)]
    struct Vote {
        prevote: u8,
        precommit: u8,
    }
    fn new_prevote() -> Vote {
        Vote {
            prevote: 1,
            precommit: 0,
        }
    }
    fn new_precommit() -> Vote {
        Vote {
            prevote: 0,
            precommit: 1,
        }
    }
    impl core::ops::AddAssign<&Vote> for Vote {
        fn add_assign(&mut self, other: &Vote) {
            self.prevote += other.prevote;
            self.precommit += other.precommit;
        }
    }

    let mut stream = RandomnessStream::new(data);
    let mut graph = VoteGraph::new(0, 0, Vote::default());

    // Record all prevote weights, checking the prevote-ghost.
    let mut prevote_ghost = None;
    for _ in 0..N {
        let (target_hash, target_number) = match rand_block(&mut stream) {
            None => return,
            Some(b) => b,
        };

        graph
            .insert(target_hash, target_number, new_prevote(), &FuzzChain)
            .unwrap();

        let new_prevote_ghost = graph.find_ghost(prevote_ghost, |v| v.prevote >= T);
        if let Some(old_ghost) = prevote_ghost {
            let new_ghost = new_prevote_ghost.expect("ghost does not disappear with more votes.");
            check_prevote_ghost(old_ghost, new_ghost);
        }

        if let Some((hash, _nr)) = new_prevote_ghost {
            // Check maximality of the prevote-ghost's block number w.r.t threshold weight.
            for descendent in FuzzChain::all_descendents(hash) {
                let desc_nr = FuzzChain::number(descendent);
                assert!(graph.cumulative_vote(descendent, desc_nr).prevote < T);
            }
        }

        prevote_ghost = new_prevote_ghost;
    }

    let prevote_ghost = prevote_ghost.expect("prevote ghost always some by this point.");

    // Record precommit weights, monitoring the precommit-ghost and estimate.
    let mut estimate: Option<(Hash, BlockNumber)> = None;
    let mut completable = false;
    for i in 0..N {
        // Pick a random precommit target on the chain with head `prevote_ghost`,
        // i.e. all (honest) precommits must be on the same chain as they are all
        // cast on a prevote-ghost block.
        let (target_hash, target_number) = match rand_block(&mut stream) {
            None => return, // Not enough randomness
            Some((hash, nr)) => match FuzzChain.ancestry(hash, prevote_ghost.0) {
                Ok(_) => (hash, nr),
                Err(_) => prevote_ghost,
            },
        };

        // Add the precommit vote to the graph.
        graph
            .insert(target_hash, target_number, new_precommit(), &FuzzChain)
            .unwrap();

        // The already calculated prevote ghost should not change as a result of
        // adding precommit weights.
        let new_prevote_ghost = graph
            .find_ghost(Some(prevote_ghost), |v| v.prevote >= T)
            .unwrap();
        assert_eq!(new_prevote_ghost, prevote_ghost, "prevote ghost changed");

        // The number of voters who did not yet cast a vote.
        let remaining = N - i - 1;

        // Note: This overestimates the possible weight of a vote-node by up to F
        // as a result of overestimating possible equivocations (always F).
        // However, any situation where the possible equivocations should be less
        // than F for some block b, is also a situation in which there is less than F
        // vote weight on blocks not >= b and it is thus possible for b to have supermajority.
        let possible_to_precommit = |v: &Vote| v.precommit + remaining + F >= T;

        let new_estimate =
            graph.find_ancestor(prevote_ghost.0, prevote_ghost.1, possible_to_precommit);

        let newly_completable = new_estimate.map_or(false, |(hash, nr)| {
            // Every estimate must be on the chain with head prevote ghost.
            if hash != prevote_ghost.0 {
                assert!(FuzzChain.ancestry(hash, prevote_ghost.0).is_ok());
                true
            } else {
                // Determine completability.
                graph
                    .find_ghost(Some((hash, nr)), possible_to_precommit)
                    .expect("by definition of estimate")
                    == prevote_ghost
            }
        });

        if completable {
            // Check monotonicity of completability.
            assert!(newly_completable);

            // Check (backwards) monotonicity of the estimate.
            let old_estimate = estimate.expect("was completable before");
            let new_estimate = new_estimate.expect("was completable before; estimate exists");
            check_estimate(old_estimate, new_estimate);
        }

        estimate = new_estimate;
        completable = newly_completable;
    }

    // After all votes have been recorded, we must have an estimate and
    // determined completability of the (implicit) round.
    assert!(completable);
    assert!(estimate.is_some());
}

#[cfg(test)]
mod tests {
    #[test]
    fn be9e58ec5a0d4dce97bd1f07a3d1ffddd7d4b48b() {
        let data = include_bytes!("../fuzz_corpus/be9e58ec5a0d4dce97bd1f07a3d1ffddd7d4b48b");
        super::execute_fuzzed_vote(&data[..]);
    }

    #[test]
    fn a8898e66e34fee70c41c7aac26369c02e249dfe9() {
        let data = include_bytes!("../fuzz_corpus/a8898e66e34fee70c41c7aac26369c02e249dfe9");
        super::execute_fuzzed_vote(&data[..]);
    }

    #[test]
    fn crash_499bf756959c90958d05c669b77b4a6f85e4fbf5() {
        let data = include_bytes!("../fuzz_corpus/crash-499bf756959c90958d05c669b77b4a6f85e4fbf5");
        super::execute_fuzzed_graph(&data[..]);
    }
}
