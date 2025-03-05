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

use core::{fmt, str};
#[cfg(feature = "serde")]
use {
    core::{marker::PhantomData, str::FromStr},
    serde::de,
};

const LEN: usize = 32;
const MEDIAN: usize = LEN.div_ceil(2);

pub(crate) struct ByteArray<'a>(pub &'a [u8; LEN]);

impl fmt::Display for ByteArray<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut e1 = MEDIAN;
        let mut s2 = MEDIAN;

        if let Some(precision) = f.precision() {
            if precision < MEDIAN {
                e1 = precision;
                s2 = LEN - precision;
            }
        }

        let mut out1 = [0; MEDIAN * 2];
        let mut out2 = [0; MEDIAN * 2];

        let out1_len = e1 * 2;
        let out2_len = (LEN - s2) * 2;

        let _ = hex::encode_to_slice(&self.0[..e1], &mut out1[..out1_len]);
        let _ = hex::encode_to_slice(&self.0[s2..], &mut out2[..out2_len]);

        let p1 = unsafe { str::from_utf8_unchecked(&out1[..out1_len]) };
        let p2 = unsafe { str::from_utf8_unchecked(&out2[..out2_len]) };
        let sep = e1.ne(&s2).then_some("..").unwrap_or_default();

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
