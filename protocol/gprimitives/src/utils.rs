// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Primitives' utils

use core::{
    fmt,
    ops::{Index, RangeFrom, RangeTo},
    str,
};
#[cfg(feature = "serde")]
use {
    core::{marker::PhantomData, str::FromStr},
    serde::de,
};

/// `[u8]` formatter.
///
/// So it looks like `0x12ae..ff80`.
pub enum ByteSliceFormatter<'a> {
    /// Fixed-size array so it can be formatted on stack.
    Array(&'a [u8; 32]),
    /// Slice of any size.
    ///
    /// If the size is less or equal to 32, it is formatted on stack,
    /// on heap otherwise.
    Dynamic(&'a [u8]),
}

impl ByteSliceFormatter<'_> {
    fn len(&self) -> usize {
        match self {
            ByteSliceFormatter::Array(arr) => arr.len(),
            ByteSliceFormatter::Dynamic(slice) => slice.len(),
        }
    }
}

impl Index<RangeTo<usize>> for ByteSliceFormatter<'_> {
    type Output = <[u8] as Index<RangeTo<usize>>>::Output;

    fn index(&self, index: RangeTo<usize>) -> &Self::Output {
        match self {
            ByteSliceFormatter::Array(arr) => &arr[index],
            ByteSliceFormatter::Dynamic(slice) => &slice[index],
        }
    }
}

impl Index<RangeFrom<usize>> for ByteSliceFormatter<'_> {
    type Output = <[u8] as Index<RangeFrom<usize>>>::Output;

    fn index(&self, index: RangeFrom<usize>) -> &Self::Output {
        match self {
            ByteSliceFormatter::Array(arr) => &arr[index],
            ByteSliceFormatter::Dynamic(slice) => &slice[index],
        }
    }
}

impl fmt::Display for ByteSliceFormatter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const STACK_LEN: usize = 32;

        let len = self.len();
        let median = len.div_ceil(2);

        let mut e1 = median;
        let mut s2 = median;

        if let Some(precision) = f.precision()
            && precision < median
        {
            e1 = precision;
            s2 = len - precision;
        }

        let out1_len = e1 * 2;
        let out2_len = (len - s2) * 2;

        let mut out1_vec;
        let mut out2_vec;

        let (out1, out2) = match self {
            ByteSliceFormatter::Array(_arr) => (
                &mut [0u8; STACK_LEN] as &mut [u8],
                &mut [0u8; STACK_LEN] as &mut [u8],
            ),
            ByteSliceFormatter::Dynamic(slice) if slice.len() <= STACK_LEN => (
                &mut [0u8; STACK_LEN] as &mut [u8],
                &mut [0u8; STACK_LEN] as &mut [u8],
            ),
            ByteSliceFormatter::Dynamic(_slice) => {
                out1_vec = alloc::vec![0u8; out1_len];
                out2_vec = alloc::vec![0u8; out2_len];
                (&mut out1_vec[..], &mut out2_vec[..])
            }
        };

        let _ = hex::encode_to_slice(&self[..e1], &mut out1[..out1_len]);
        let _ = hex::encode_to_slice(&self[s2..], &mut out2[..out2_len]);

        let p1 = unsafe { str::from_utf8_unchecked(&out1[..out1_len]) };
        let p2 = unsafe { str::from_utf8_unchecked(&out2[..out2_len]) };
        let sep = if e1.ne(&s2) { ".." } else { Default::default() };

        write!(f, "0x{p1}{sep}{p2}")
    }
}

#[cfg(feature = "serde")]
pub(crate) struct HexStrVisitor<T: FromStr>(PhantomData<T>);

#[cfg(feature = "serde")]
impl<T: FromStr> HexStrVisitor<T> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

#[cfg(feature = "serde")]
impl<T: FromStr> de::Visitor<'_> for HexStrVisitor<T> {
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string in hex format starting with 0x")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        value
            .parse()
            .map_err(|_| de::Error::invalid_value(de::Unexpected::Str(value), &self))
    }
}
