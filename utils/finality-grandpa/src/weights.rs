// Copyright 2019 Parity Technologies (UK) Ltd
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

//! This module lays out the rules for the arithmetic of vote(r) weights.

use crate::std::{
    cmp::Ordering,
    fmt,
    num::NonZeroU64,
    ops::{Add, Sub},
};

/// The accumulated weight of any number of voters (possibly none).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct VoteWeight(pub u64);

impl fmt::Display for VoteWeight {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Add for VoteWeight {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        VoteWeight(self.0.saturating_add(rhs.0))
    }
}

impl Add<VoterWeight> for VoteWeight {
    type Output = Self;

    fn add(self, rhs: VoterWeight) -> Self {
        VoteWeight(self.0.saturating_add(rhs.0.get()))
    }
}

impl Sub for VoteWeight {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        VoteWeight(self.0.saturating_sub(rhs.0))
    }
}

impl Sub<VoterWeight> for VoteWeight {
    type Output = Self;

    fn sub(self, rhs: VoterWeight) -> Self {
        self - VoteWeight(rhs.get())
    }
}

impl PartialEq<VoterWeight> for VoteWeight {
    fn eq(&self, other: &VoterWeight) -> bool {
        self.0 == other.get()
    }
}

impl PartialOrd<VoterWeight> for VoteWeight {
    fn partial_cmp(&self, other: &VoterWeight) -> Option<Ordering> {
        Some(self.0.cmp(&other.0.get()))
    }
}

impl From<u64> for VoteWeight {
    fn from(weight: u64) -> Self {
        VoteWeight(weight)
    }
}

/// The (non-zero) weight of one or more voters.
///
/// Having a non-zero weight is part of the definition of being a voter.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub struct VoterWeight(pub NonZeroU64);

impl fmt::Display for VoterWeight {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl VoterWeight {
    pub fn new(weight: u64) -> Option<Self> {
        NonZeroU64::new(weight).map(Self)
    }

    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl Sub<VoteWeight> for VoterWeight {
    type Output = VoteWeight;

    fn sub(self, rhs: VoteWeight) -> VoteWeight {
        VoteWeight(self.0.get()) - rhs
    }
}

impl Sub<VoterWeight> for VoterWeight {
    type Output = VoteWeight;

    fn sub(self, rhs: VoterWeight) -> VoteWeight {
        VoteWeight(self.0.get()) - VoteWeight(rhs.get())
    }
}

#[cfg(feature = "std")]
impl std::convert::TryFrom<u64> for VoterWeight {
    type Error = &'static str;

    fn try_from(weight: u64) -> Result<Self, Self::Error> {
        VoterWeight::new(weight).ok_or("VoterWeight only takes non-zero values.")
    }
}
