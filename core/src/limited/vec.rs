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

//! This module provides type for vector with statically limited length.

use core::fmt::{self, Formatter};

use alloc::{vec, vec::Vec};

use derive_more::{AsMut, AsRef, Debug, Deref, DerefMut, Display, Error, Into, IntoIterator};
use gprimitives::utils::ByteSliceFormatter;
use scale_info::{
    TypeInfo,
    scale::{Decode, Encode},
};

/// Vector with limited length.
#[derive(
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Decode,
    Encode,
    Hash,
    TypeInfo,
    AsRef,
    AsMut,
    Deref,
    DerefMut,
    IntoIterator,
    Into,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[as_ref(forward)]
#[as_mut(forward)]
#[deref(forward)]
#[deref_mut(forward)]
#[into_iterator(owned, ref, ref_mut)]
pub struct LimitedVec<T, const N: usize>(Vec<T>);

impl<T: Clone, const N: usize> TryFrom<&[T]> for LimitedVec<T, N> {
    type Error = LimitedVecError;

    fn try_from(slice: &[T]) -> Result<Self, Self::Error> {
        Self::validate_len(slice.len()).map(|_| Self(slice.to_vec()))
    }
}

impl<T, const N: usize> TryFrom<Vec<T>> for LimitedVec<T, N> {
    type Error = LimitedVecError;
    fn try_from(vec: Vec<T>) -> Result<Self, Self::Error> {
        Self::validate_len(vec.len()).map(|_| Self(vec))
    }
}

impl<T, const N: usize> LimitedVec<T, N> {
    /// Maximum length of the vector.
    pub const MAX_LEN: usize = N;

    /// Validates given length.
    ///
    /// Returns `Ok(())` if the vector can store such number
    /// of elements and `Err(LimitedVecError)` otherwise.
    const fn validate_len(len: usize) -> Result<(), LimitedVecError> {
        if len <= N {
            Ok(())
        } else {
            Err(LimitedVecError)
        }
    }

    /// Constructs a new, empty `LimitedVec<T>`.
    pub const fn new() -> Self {
        Self(vec![])
    }

    /// Creates a new limited vector with elements
    /// initialized with [`Default::default`].
    pub fn repeat(value: T) -> Self
    where
        T: Clone,
    {
        Self(vec![value; N])
    }

    /// Creates a new limited vector with given
    /// length by repeatedly cloning a value.
    pub fn try_repeat(value: T, len: usize) -> Result<Self, LimitedVecError>
    where
        T: Clone,
    {
        Self::validate_len(len).map(|_| Self(vec![value; len]))
    }

    /// Extends the vector to its limit by
    /// repeatedly adding a value.
    pub fn extend_with(&mut self, value: T)
    where
        T: Clone,
    {
        self.0.resize(N, value)
    }

    /// Appends a value to the end of the vector.
    pub fn try_push(&mut self, value: T) -> Result<(), LimitedVecError> {
        Self::validate_len(self.len() + 1)?;

        self.0.push(value);
        Ok(())
    }

    /// Appends values from slice to the end of vector.
    pub fn try_extend_from_slice(&mut self, values: &[T]) -> Result<(), LimitedVecError>
    where
        T: Clone,
    {
        let new_len = self
            .len()
            .checked_add(values.len())
            .ok_or(LimitedVecError)?;
        Self::validate_len(new_len)?;

        self.0.extend_from_slice(values);
        Ok(())
    }

    /// Returns a slice reference to the vector contents.
    pub fn as_slice(&self) -> &[T] {
        self
    }

    /// Clones the limited vector into `Vec<T>`.
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.0.clone()
    }

    /// Converts the limited vector into its inner `Vec<T>`.
    pub fn into_vec(self) -> Vec<T> {
        self.0
    }
}

impl<T, const N: usize> fmt::Display for LimitedVec<T, N>
where
    [T]: AsRef<[u8]>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let bytes = ByteSliceFormatter::Dynamic(self.0.as_slice().as_ref());

        let fmt_bytes: fn(&mut Formatter, fmt::Arguments) -> _ = if f.alternate() {
            |f, bytes| write!(f, "LimitedVec({bytes})")
        } else {
            |f, bytes| write!(f, "{bytes}")
        };

        if let Some(precision) = f.precision() {
            fmt_bytes(f, format_args!("{bytes:.precision$}"))
        } else if f.sign_plus() {
            fmt_bytes(f, format_args!("{bytes}"))
        } else {
            fmt_bytes(f, format_args!("{bytes:.8}"))
        }
    }
}

impl<T, const N: usize> fmt::Debug for LimitedVec<T, N>
where
    [T]: AsRef<[u8]>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Error type for limited vector overflowing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Display, Error)]
#[display("{}", Self::MESSAGE)]
pub struct LimitedVecError;

impl LimitedVecError {
    /// Static error message.
    pub const MESSAGE: &str = "vector length limit is exceeded";

    /// Converts the error into a static error message.
    pub const fn as_str(&self) -> &'static str {
        Self::MESSAGE
    }
}

#[cfg(test)]
mod test {
    use super::LimitedVec;
    use alloc::{format, string::String, vec, vec::Vec};
    use core::convert::TryFrom;

    const N: usize = 1000;
    type TestBuffer = LimitedVec<u8, N>;
    const M: usize = 64;
    type SmallTestBuffer = LimitedVec<u8, M>;

    #[test]
    fn test_try_from() {
        let v1 = vec![1; N];
        let v2 = vec![1; N + 1];
        let v3 = vec![1; N - 1];

        let x = TestBuffer::try_from(v1).unwrap();
        let _ = TestBuffer::try_from(v2).expect_err("Must be err because of size overflow");
        let z = TestBuffer::try_from(v3).unwrap();

        assert_eq!(x.len(), N);
        assert_eq!(z.len(), N - 1);
        assert_eq!(x[N / 2], 1);
        assert_eq!(z[N / 2], 1);
    }

    #[test]
    fn test_repeat() {
        let x = LimitedVec::<u32, N>::repeat(0);
        assert_eq!(x.len(), N);

        let y = LimitedVec::<i32, 3>::repeat(-4);
        assert_eq!(y.as_slice(), &[-4, -4, -4]);
    }

    #[test]
    fn test_try_repeat() {
        let x = LimitedVec::<String, N>::try_repeat(String::new(), N).unwrap();
        assert!(
            LimitedVec::<u64, N>::try_repeat(0, N + 1).is_err(),
            "Must be error because of size overflow"
        );
        let y = LimitedVec::<char, 7>::try_repeat('@', 5).unwrap();
        let z = LimitedVec::<Vec<u8>, N>::try_repeat(vec![], 0).unwrap();

        assert_eq!(x.len(), N);
        assert_eq!(z.len(), 0);
        assert_eq!(x[N / 2], "");
        assert_eq!(y.as_slice(), &['@', '@', '@', '@', '@']);
    }

    #[test]
    fn test_full() {
        let mut x = TestBuffer::try_from(vec![1; N]).unwrap();
        let mut y = TestBuffer::try_from(vec![2; N / 2]).unwrap();
        let mut z = TestBuffer::try_from(vec![3; 0]).unwrap();

        x.try_extend_from_slice(&[1, 2, 3]).unwrap_err();
        y.try_extend_from_slice(&[1, 2, 3]).unwrap();
        z.try_extend_from_slice(&[1, 2, 3]).unwrap();

        x.try_push(42).unwrap_err();
        y.try_push(42).unwrap();
        z.try_push(42).unwrap();

        x.try_extend_from_slice(&[1, 2, 3]).unwrap_err();
        y.try_extend_from_slice(&[1, 2, 3]).unwrap();
        z.try_extend_from_slice(&[1, 2, 3]).unwrap();

        z[0] = 0;

        assert_eq!(&z.into_vec(), &[0, 2, 3, 42, 1, 2, 3]);
        assert_eq!(TestBuffer::MAX_LEN, N);
    }

    #[test]
    fn formatting_test() {
        use alloc::format;

        let buffer = SmallTestBuffer::try_from(b"abcdefghijklmnopqrstuvwxyz012345".to_vec())
            .expect("String is 64 bytes");

        // `Debug`/`Display`.
        assert_eq!(
            format!("{buffer:+?}"),
            "0x6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435"
        );
        // `Debug`/`Display` with default precision.
        assert_eq!(
            format!("{buffer:?}"),
            "0x6162636465666768..797a303132333435"
        );
        // `Debug`/`Display` with precision 0.
        assert_eq!(format!("{buffer:.0?}"), "0x..");
        // `Debug`/`Display` with precision 1.
        assert_eq!(format!("{buffer:.1?}"), "0x61..35");
        // `Debug`/`Display` with precision 2.
        assert_eq!(format!("{buffer:.2?}"), "0x6162..3435");
        // `Debug`/`Display` with precision 4.
        assert_eq!(format!("{buffer:.4?}"), "0x61626364..32333435");
        // `Debug`/`Display` with precision 15.
        assert_eq!(
            format!("{buffer:.15?}"),
            "0x6162636465666768696a6b6c6d6e6f..72737475767778797a303132333435"
        );
        // `Debug`/`Display` with precision 30.
        assert_eq!(
            format!("{buffer:.30?}"),
            "0x6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435"
        );
        // Alternate formatter with default precision.
        assert_eq!(
            format!("{buffer:#}"),
            "LimitedVec(0x6162636465666768..797a303132333435)"
        );
        // Alternate formatter with max precision.
        assert_eq!(
            format!("{buffer:+#}"),
            "LimitedVec(0x6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435)"
        );
        // Alternate formatter with precision 2.
        assert_eq!(format!("{buffer:#.2}"), "LimitedVec(0x6162..3435)");
    }
}
