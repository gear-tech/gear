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

//! Implementation of a `VoterSet`, representing the complete set
//! of voters and their weights in the context of a round of the
//! protocol.

use crate::{
    std::{
        collections::{btree_map::Entry, BTreeMap},
        num::{NonZeroU64, NonZeroUsize},
        vec::Vec,
    },
    weights::VoterWeight,
};

/// A (non-empty) set of voters and associated weights.
///
/// A `VoterSet` identifies all voters that are permitted to vote in a round
/// of the protocol and their associated weights. A `VoterSet` is furthermore
/// equipped with a total order, given by the ordering of the voter's IDs.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VoterSet<Id: Eq + Ord> {
    /// The voters in the voter set, this vec is always sorted by the voter ID.
    voters: Vec<(Id, VoterInfo)>,
    /// The required weight threshold for supermajority w.r.t. this set.
    threshold: VoterWeight,
    /// The total weight of all voters.
    total_weight: VoterWeight,
}

impl<Id: Eq + Ord> VoterSet<Id> {
    /// Create a voter set from a weight distribution produced by the given iterator.
    ///
    /// If the distribution contains multiple weights for the same voter ID, they are
    /// understood to be partial weights and are accumulated. As a result, the
    /// order in which the iterator produces the weights is irrelevant.
    ///
    /// Returns `None` if the iterator does not yield a valid voter set, which is
    /// the case if it either produced no non-zero weights or, i.e. the voter set
    /// would be empty, or if the total voter weight exceeds `u64::MAX`.
    pub fn new<I>(weights: I) -> Option<Self>
    where
        Id: Ord + Clone,
        I: IntoIterator<Item = (Id, u64)>,
    {
        // Populate the voter set, thereby calculating the total weight.
        let mut voters = BTreeMap::new();
        let mut total_weight = 0u64;
        for (id, weight) in weights {
            if let Some(w) = NonZeroU64::new(weight) {
                // Prevent construction of inconsistent voter sets by checking
                // for weight overflow (not just in debug mode). The protocol
                // should never run with such voter sets.
                total_weight = total_weight.checked_add(weight)?;
                match voters.entry(id) {
                    Entry::Vacant(e) => {
                        e.insert(VoterInfo {
                            position: 0, // The total order is determined afterwards.
                            weight: VoterWeight(w),
                        });
                    }
                    Entry::Occupied(mut e) => {
                        let v = e.get_mut();
                        let n = v.weight.get() + weight;
                        let w = NonZeroU64::new(n).expect("nonzero + nonzero is nonzero");
                        v.weight = VoterWeight(w);
                    }
                }
            }
        }

        if voters.is_empty() {
            // No non-zero weights; the set would be empty.
            return None;
        }

        let voters = voters
            .into_iter()
            .enumerate()
            .map(|(position, (id, mut info))| {
                info.position = position;
                (id, info)
            })
            .collect();

        let total_weight = VoterWeight::new(total_weight).expect("voters nonempty; qed");

        Some(VoterSet {
            voters,
            total_weight,
            threshold: threshold(total_weight),
        })
    }

    /// Get the voter info for the voter with the given ID, if any.
    pub fn get(&self, id: &Id) -> Option<&VoterInfo> {
        self.voters
            .binary_search_by_key(&id, |(id, _)| id)
            .ok()
            .map(|idx| &self.voters[idx].1)
    }

    /// Get the size of the set.
    pub fn len(&self) -> NonZeroUsize {
        unsafe {
            // SAFETY: By VoterSet::new()
            NonZeroUsize::new_unchecked(self.voters.len())
        }
    }

    /// Whether the set contains a voter with the given ID.
    pub fn contains(&self, id: &Id) -> bool {
        self.voters.binary_search_by_key(&id, |(id, _)| id).is_ok()
    }

    /// Get the nth voter in the set, modulo the size of the set,
    /// as per the associated total order.
    pub fn nth_mod(&self, n: usize) -> (&Id, &VoterInfo) {
        self.nth(n % self.voters.len())
            .expect("set is nonempty and n % len < len; qed")
    }

    /// Get the nth voter in the set, if any.
    ///
    /// Returns `None` if `n >= len`.
    pub fn nth(&self, n: usize) -> Option<(&Id, &VoterInfo)> {
        self.voters.get(n).map(|(id, info)| (id, info))
    }

    /// Get the threshold vote weight required for supermajority
    /// w.r.t. this set of voters.
    pub fn threshold(&self) -> VoterWeight {
        self.threshold
    }

    /// Get the total weight of all voters.
    pub fn total_weight(&self) -> VoterWeight {
        self.total_weight
    }

    /// Get an iterator over the voters in the set, as given by
    /// the associated total order.
    pub fn iter(&self) -> impl Iterator<Item = (&Id, &VoterInfo)> {
        self.voters.iter().map(|(id, info)| (id, info))
    }
}

/// Information about a voter in a `VoterSet`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VoterInfo {
    position: usize,
    weight: VoterWeight,
}

impl VoterInfo {
    /// Get the position of the voter in the total order associated
    /// with the `VoterSet` from which the `VoterInfo` was obtained.
    pub fn position(&self) -> usize {
        self.position
    }

    /// Get the weight of the voter.
    pub fn weight(&self) -> VoterWeight {
        self.weight
    }
}

/// Compute the threshold weight given the total voting weight.
fn threshold(total_weight: VoterWeight) -> VoterWeight {
    let faulty = total_weight.get().saturating_sub(1) / 3;
    VoterWeight::new(total_weight.get() - faulty).expect("subtrahend > minuend; qed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::std::iter;
    use quickcheck::*;
    use rand::{seq::SliceRandom, thread_rng};

    impl<Id: Arbitrary + Eq + Ord> Arbitrary for VoterSet<Id> {
        fn arbitrary(g: &mut Gen) -> VoterSet<Id> {
            loop {
                let mut ids = Vec::<Id>::arbitrary(g);
                if ids.is_empty() {
                    ids.push(Id::arbitrary(g))
                }

                let weights = iter::from_fn(|| Some(u32::arbitrary(g) as u64));

                // we might generate an invalid voter set above if:
                // - all validators have 0 weight
                // - the total weight is higher than `u64::max_value()`
                //
                // the easiest thing to do is to just retry generating another instance.
                if let Some(set) = VoterSet::new(ids.into_iter().zip(weights)) {
                    break set;
                }
            }
        }
    }

    #[test]
    fn equality() {
        fn prop(mut v: Vec<(usize, u64)>) {
            if let Some(v1) = VoterSet::new(v.clone()) {
                v.shuffle(&mut thread_rng());
                let v2 = VoterSet::new(v).expect("nonempty");
                assert_eq!(v1, v2)
            } else {
                assert!(
                    // either no authority has a valid weight
                    v.iter().all(|(_, w)| w == &0) ||
					// or the total weight overflows a u64
					v.iter().map(|(_, w)| *w as u128).sum::<u128>() > u64::max_value() as u128
                );
            }
        }

        quickcheck(prop as fn(_))
    }

    #[test]
    fn total_weight() {
        fn prop(v: Vec<(usize, u64)>) {
            let total_weight = v.iter().map(|(_, weight)| *weight as u128).sum::<u128>();

            // this validator set is invalid
            if total_weight > u64::max_value() as u128 {
                return;
            }

            let expected = VoterWeight::new(total_weight as u64);

            if let Some(v1) = VoterSet::new(v) {
                assert_eq!(Some(v1.total_weight()), expected)
            } else {
                assert_eq!(expected, None)
            }
        }

        quickcheck(prop as fn(_))
    }

    #[test]
    fn min_threshold() {
        fn prop(v: VoterSet<usize>) -> bool {
            let t = v.threshold.get();
            let w = v.total_weight.get();
            t >= 2 * (w / 3) + (w % 3)
        }

        quickcheck(prop as fn(_) -> _);
    }
}
