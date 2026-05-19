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

//! Dynamically sized, write-once, lazily allocating bitfields,
//! e.g. for compact accumulation of votes cast on a block while
//! retaining information on the type of vote and identity of the
//! voter within a voter set.

use crate::std::{cmp::Ordering, iter, ops::BitOr, vec::Vec};
use either::Either;

/// A dynamically sized, write-once (per bit), lazily allocating bitfield.
#[derive(Eq, PartialEq, Clone, Debug, Default)]
pub struct Bitfield {
    bits: Vec<u64>,
}

impl From<Vec<u64>> for Bitfield {
    fn from(bits: Vec<u64>) -> Bitfield {
        Bitfield { bits }
    }
}

impl Bitfield {
    /// Create a new empty bitfield.
    ///
    /// Does not allocate.
    pub fn new() -> Bitfield {
        Bitfield { bits: Vec::new() }
    }

    /// Whether the bitfield is blank / empty.
    pub fn is_blank(&self) -> bool {
        self.bits.is_empty()
    }

    /// Merge another bitfield into this bitfield.
    ///
    /// As a result, this bitfield has all bits set that are set in either bitfield.
    ///
    /// This function only allocates if this bitfield is shorter than the other
    /// bitfield, in which case it is resized accordingly to accommodate for all
    /// bits of the other bitfield.
    pub fn merge(&mut self, other: &Self) -> &mut Self {
        if self.bits.len() < other.bits.len() {
            let new_len = other.bits.len();
            self.bits.resize(new_len, 0);
        }

        for (i, word) in other.bits.iter().enumerate() {
            self.bits[i] |= word;
        }

        self
    }

    /// Set a bit in the bitfield at the specified position.
    ///
    /// If the bitfield is not large enough to accommodate for a bit set
    /// at the specified position, it is resized accordingly.
    pub fn set_bit(&mut self, position: usize) -> &mut Self {
        let word_off = position / 64;
        let bit_off = position % 64;

        if word_off >= self.bits.len() {
            let new_len = word_off + 1;
            self.bits.resize(new_len, 0);
        }

        self.bits[word_off] |= 1 << (63 - bit_off);
        self
    }

    /// Test if the bit at the specified position is set.
    #[cfg(test)]
    pub fn test_bit(&self, position: usize) -> bool {
        let word_off = position / 64;

        if word_off >= self.bits.len() {
            return false;
        }

        test_bit(self.bits[word_off], position % 64)
    }

    /// Get an iterator over all bits that are set (i.e. 1) at even bit positions.
    pub fn iter1s_even(&self) -> impl Iterator<Item = Bit1> + '_ {
        self.iter1s(0, 1)
    }

    /// Get an iterator over all bits that are set (i.e. 1) at odd bit positions.
    pub fn iter1s_odd(&self) -> impl Iterator<Item = Bit1> + '_ {
        self.iter1s(1, 1)
    }

    /// Get an iterator over all bits that are set (i.e. 1) at even bit positions
    /// when merging this bitfield with another bitfield, without modifying
    /// either bitfield.
    pub fn iter1s_merged_even<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = Bit1> + 'a {
        self.iter1s_merged(other, 0, 1)
    }

    /// Get an iterator over all bits that are set (i.e. 1) at odd bit positions
    /// when merging this bitfield with another bitfield, without modifying
    /// either bitfield.
    pub fn iter1s_merged_odd<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = Bit1> + 'a {
        self.iter1s_merged(other, 1, 1)
    }

    /// Get an iterator over all bits that are set (i.e. 1) in the bitfield,
    /// starting at bit position `start` and moving in steps of size `2^step`
    /// per word.
    fn iter1s(&self, start: usize, step: usize) -> impl Iterator<Item = Bit1> + '_ {
        iter1s(self.bits.iter().cloned(), start, step)
    }

    /// Get an iterator over all bits that are set (i.e. 1) when merging
    /// this bitfield with another bitfield, without modifying either
    /// bitfield, starting at bit position `start` and moving in steps
    /// of size `2^step` per word.
    fn iter1s_merged<'a>(
        &'a self,
        other: &'a Self,
        start: usize,
        step: usize,
    ) -> impl Iterator<Item = Bit1> + 'a {
        match self.bits.len().cmp(&other.bits.len()) {
            Ordering::Equal => Either::Left(iter1s(
                self.bits.iter().zip(&other.bits).map(|(a, b)| a | b),
                start,
                step,
            )),
            Ordering::Less => Either::Right(Either::Left(iter1s(
                self.bits
                    .iter()
                    .chain(iter::repeat(&0))
                    .zip(&other.bits)
                    .map(|(a, b)| a | b),
                start,
                step,
            ))),
            Ordering::Greater => Either::Right(Either::Right(iter1s(
                self.bits
                    .iter()
                    .zip(other.bits.iter().chain(iter::repeat(&0)))
                    .map(|(a, b)| a | b),
                start,
                step,
            ))),
        }
    }
}

/// Turn an iterator over u64 words into an iterator over bits that
/// are set (i.e. `1`) in these words, starting at bit position `start`
/// and moving in steps of size `2^step` per word.
fn iter1s<'a, I>(iter: I, start: usize, step: usize) -> impl Iterator<Item = Bit1> + 'a
where
    I: Iterator<Item = u64> + 'a,
{
    debug_assert!(start < 64 && step < 7);
    let steps = (64 >> step) - (start >> step);
    iter.enumerate().flat_map(move |(i, word)| {
        let n = if word == 0 { 0 } else { steps };
        (0..n).filter_map(move |j| {
            let bit_pos = start + (j << step);
            if test_bit(word, bit_pos) {
                Some(Bit1 {
                    position: i * 64 + bit_pos,
                })
            } else {
                None
            }
        })
    })
}

fn test_bit(word: u64, position: usize) -> bool {
    let mask = 1 << (63 - position);
    word & mask == mask
}

impl BitOr<&Bitfield> for Bitfield {
    type Output = Bitfield;

    fn bitor(mut self, rhs: &Bitfield) -> Self::Output {
        self.merge(rhs);
        self
    }
}

/// A bit that is set (i.e. 1) in a `Bitfield`.
#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub struct Bit1 {
    /// The position of the bit in the bitfield.
    pub position: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::std::iter;
    use quickcheck::*;

    impl Arbitrary for Bitfield {
        fn arbitrary(g: &mut Gen) -> Bitfield {
            let n = usize::arbitrary(g) % g.size();
            let mut b = iter::from_fn(|| Some(u64::arbitrary(g)))
                .take(n)
                .collect::<Vec<_>>();

            // we need to make sure we don't add empty words at the end of the
            // bitfield otherwise it would break equality on some of the tests
            // below.
            while let Some(0) = b.last() {
                b.pop();
            }

            Bitfield::from(b)
        }
    }

    #[test]
    fn set_bit() {
        fn prop(mut a: Bitfield, idx: usize) -> bool {
            // let's bound the max bitfield index at 2^24. this is needed because when calling
            // `set_bit` we will extend the backing vec to accommodate the given bitfield size, this
            // way we restrict the maximum allocation size to 16MB.
            let idx = idx.min(1 << 24);

            a.set_bit(idx).test_bit(idx)
        }

        quickcheck(prop as fn(_, _) -> _)
    }

    #[test]
    fn bitor() {
        fn prop(a: Bitfield, b: Bitfield) -> bool {
            let c = a.clone() | &b;
            let mut c_bits = c.iter1s(0, 0);
            c_bits.all(|bit| a.test_bit(bit.position) || b.test_bit(bit.position))
        }

        quickcheck(prop as fn(_, _) -> _)
    }

    #[test]
    fn bitor_commutative() {
        fn prop(a: Bitfield, b: Bitfield) -> bool {
            a.clone() | &b == b | &a
        }

        quickcheck(prop as fn(_, _) -> _)
    }

    #[test]
    fn bitor_associative() {
        fn prop(a: Bitfield, b: Bitfield, c: Bitfield) -> bool {
            (a.clone() | &b) | &c == a | &(b | &c)
        }

        quickcheck(prop as fn(_, _, _) -> _)
    }

    #[test]
    fn iter1s() {
        fn all(a: Bitfield) {
            let mut b = Bitfield::new();
            for Bit1 { position } in a.iter1s(0, 0) {
                b.set_bit(position);
            }
            assert_eq!(a, b);
        }

        fn even_odd(a: Bitfield) {
            let mut b = Bitfield::new();
            for Bit1 { position } in a.iter1s_even() {
                assert!(!b.test_bit(position));
                assert!(position % 2 == 0);
                b.set_bit(position);
            }
            for Bit1 { position } in a.iter1s_odd() {
                assert!(!b.test_bit(position));
                assert!(position % 2 == 1);
                b.set_bit(position);
            }
            assert_eq!(a, b);
        }

        quickcheck(all as fn(_));
        quickcheck(even_odd as fn(_));
    }

    #[test]
    fn iter1s_merged() {
        fn all(mut a: Bitfield, b: Bitfield) {
            let mut c = Bitfield::new();
            for bit1 in a.iter1s_merged(&b, 0, 0) {
                c.set_bit(bit1.position);
            }
            assert_eq!(&c, a.merge(&b))
        }

        fn even_odd(mut a: Bitfield, b: Bitfield) {
            let mut c = Bitfield::new();
            for Bit1 { position } in a.iter1s_merged_even(&b) {
                assert!(!c.test_bit(position));
                assert!(position % 2 == 0);
                c.set_bit(position);
            }
            for Bit1 { position } in a.iter1s_merged_odd(&b) {
                assert!(!c.test_bit(position));
                assert!(position % 2 == 1);
                c.set_bit(position);
            }
            assert_eq!(&c, a.merge(&b));
        }

        quickcheck(all as fn(_, _));
        quickcheck(even_odd as fn(_, _));
    }
}
