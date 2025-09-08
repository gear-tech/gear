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

//! Vector with limited len realization.

use core::{
    convert::TryFrom,
    fmt::{self, Debug, Display, Formatter},
    ops::{Deref, DerefMut},
};

use alloc::{sync::Arc, vec, vec::Vec};
use core::hash::{Hash, Hasher};
use parity_scale_codec::{Compact, MaxEncodedLen};
use scale_info::{
    TypeInfo,
    scale::{Decode, Encode},
};

use crate::str::LimitedStr;

/// Limited len vector.
/// `T` is data type.
/// `E` is overflow error type.
/// `N` is max len which a vector can have.
#[derive(Clone, Default, Eq, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct LimitedVec<T, const N: usize> {
    inner: Vec<T>,
}

/// Formatter for [`LimitedVec`] will print to precision of 8 by default, to print the whole data, use `{:+}`.
impl<T: Clone + Default, const N: usize> Display for LimitedVec<T, N>
where
    [T]: AsRef<[u8]>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let len = self.inner.len();
        let median = len.div_ceil(2);

        let mut e1 = median;
        let mut s2 = median;

        if let Some(precision) = f.precision() {
            if precision < median {
                e1 = precision;
                s2 = len - precision;
            }
        } else if !f.sign_plus() && median > 8 {
            e1 = 8;
            s2 = len - 8;
        }

        let p1 = hex::encode(&self.inner[..e1]);
        let p2 = hex::encode(&self.inner[s2..]);
        let sep = if e1.ne(&s2) { ".." } else { Default::default() };

        if f.alternate() {
            write!(f, "LimitedVec(0x{p1}{sep}{p2})")
        } else {
            write!(f, "0x{p1}{sep}{p2}")
        }
    }
}

/// Error type for limited vector overflowing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LimitedVecError;

impl LimitedVecError {
    /// Returns a static error message.
    pub const fn message(&self) -> &'static str {
        "vector length limit is exceeded"
    }
}

impl Display for LimitedVecError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl core::error::Error for LimitedVecError {}

impl<T: Clone + Default, const N: usize> Debug for LimitedVec<T, N>
where
    [T]: AsRef<[u8]>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

impl<T: Clone, const N: usize> TryFrom<&[T]> for LimitedVec<T, N> {
    type Error = LimitedVecError;

    fn try_from(slice: &[T]) -> Result<Self, Self::Error> {
        Self::validate_len(slice.len()).map(|_| Self {
            inner: slice.to_vec(),
        })
    }
}

impl<T, const N: usize> TryFrom<Vec<T>> for LimitedVec<T, N> {
    type Error = LimitedVecError;
    fn try_from(vec: Vec<T>) -> Result<Self, Self::Error> {
        Self::validate_len(vec.len()).map(|_| Self { inner: vec })
    }
}

impl<T, const N: usize> AsRef<[T]> for LimitedVec<T, N> {
    fn as_ref(&self) -> &[T] {
        &self.inner
    }
}

impl<T, const N: usize> AsMut<[T]> for LimitedVec<T, N> {
    fn as_mut(&mut self) -> &mut [T] {
        &mut self.inner
    }
}

impl<T, const N: usize> Deref for LimitedVec<T, N> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T, const N: usize> DerefMut for LimitedVec<T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

impl<T, const N: usize> IntoIterator for LimitedVec<T, N> {
    type Item = T;
    type IntoIter = alloc::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a LimitedVec<T, N> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a mut LimitedVec<T, N> {
    type Item = &'a mut T;
    type IntoIter = core::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T: Hash, const N: usize> Hash for LimitedVec<T, N> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
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
        Self { inner: vec![] }
    }

    /// Tries to create new limited vector of length `len`
    /// with default initialized elements.
    pub fn try_new_default(len: usize) -> Result<Self, LimitedVecError>
    where
        T: Default + Clone,
    {
        Self::validate_len(len).map(|_| Self {
            inner: vec![T::default(); len],
        })
    }

    /// Creates new limited vector with default initialized elements.
    pub fn new_default() -> Self
    where
        T: Default + Clone,
    {
        Self::filled_with(T::default())
    }

    /// Creates limited vector filled with the specified `value`.
    pub fn filled_with(value: T) -> Self
    where
        T: Clone,
    {
        Self {
            inner: vec![value; N],
        }
    }

    /// Extends the array to its limit and fills with the specified `value`.
    pub fn extend_with(&mut self, value: T)
    where
        T: Clone,
    {
        self.inner.resize(N, value);
    }

    /// Append `value` to the end of vector.
    pub fn try_push(&mut self, value: T) -> Result<(), LimitedVecError> {
        Self::validate_len(self.inner.len() + 1)?;

        self.inner.push(value);
        Ok(())
    }

    /// Append `values` to the end of vector.
    pub fn try_extend_from_slice(&mut self, values: &[T]) -> Result<(), LimitedVecError>
    where
        T: Clone,
    {
        let new_len = self
            .inner
            .len()
            .checked_add(values.len())
            .ok_or(LimitedVecError)?;
        Self::validate_len(new_len)?;

        self.inner.extend_from_slice(values);
        Ok(())
    }

    /// Append `values` to the begin of vector.
    pub fn try_prepend(&mut self, values: Self) -> Result<(), LimitedVecError> {
        let new_len = self
            .inner
            .len()
            .checked_add(values.inner.len())
            .ok_or(LimitedVecError)?;
        Self::validate_len(new_len)?;

        self.inner.splice(0..0, values.inner);
        Ok(())
    }

    /// Clones self into vector.
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.inner.clone()
    }

    /// Destruct limited vector and returns inner vector.
    pub fn into_vec(self) -> Vec<T> {
        self.inner
    }

    /// Returns max len which this type of limited vector can have.
    pub const fn max_len() -> usize {
        N
    }
}

/// Max memory size, which runtime can allocate at once.
/// Substrate allocator restrict allocations bigger then 512 wasm pages at once.
/// See more information about:
/// https://github.com/paritytech/substrate/blob/cc4d5cc8654d280f03a13421669ba03632e14aa7/client/allocator/src/freeing_bump.rs#L136-L149
/// https://github.com/paritytech/substrate/blob/cc4d5cc8654d280f03a13421669ba03632e14aa7/primitives/core/src/lib.rs#L385-L388
const RUNTIME_MAX_ALLOC_SIZE: usize = 512 * 0x10000;

/// Take half from [RUNTIME_MAX_ALLOC_SIZE] in order to avoid problems with capacity overflow.
const RUNTIME_MAX_BUFF_SIZE: usize = RUNTIME_MAX_ALLOC_SIZE / 2;

/// Wrapper for payload slice.
pub struct PayloadSlice {
    /// Start of the slice.
    start: usize,
    /// End of the slice.
    end: usize,
    /// Payload
    payload: Arc<Payload>,
}

impl PayloadSlice {
    /// Try to create a new PayloadSlice.
    pub fn try_new(start: u32, end: u32, payload: Arc<Payload>) -> Option<Self> {
        // Check if start and end are within the bounds of the payload
        if start > end || end > payload.len_u32() {
            return None;
        }

        Some(Self {
            start: start as usize,
            end: end as usize,
            payload,
        })
    }

    /// Get slice of the payload.
    pub fn slice(&self) -> &[u8] {
        &self.payload[self.start..self.end]
    }
}

/// Buffer which size cannot be bigger then max allowed allocation size in runtime.
pub type RuntimeBuffer = LimitedVec<u8, RUNTIME_MAX_BUFF_SIZE>;

/// Max payload size which one message can have (8 MiB).
pub const MAX_PAYLOAD_SIZE: usize = 8 * 1024 * 1024;

// **WARNING**: do not remove this check
const _: () = assert!(MAX_PAYLOAD_SIZE <= u32::MAX as usize);

/// Payload type for message.
pub type Payload = LimitedVec<u8, MAX_PAYLOAD_SIZE>;

impl Payload {
    /// Get payload length as u32.
    pub fn len_u32(&self) -> u32 {
        // Safe, cause it's guarantied: `MAX_PAYLOAD_SIZE` <= u32::MAX
        self.len() as u32
    }
}

impl MaxEncodedLen for Payload {
    fn max_encoded_len() -> usize {
        Compact::<u32>::max_encoded_len() + MAX_PAYLOAD_SIZE
    }
}

/// Panic buffer which size cannot be bigger then max allowed payload size.
#[derive(
    Clone,
    Default,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Decode,
    Encode,
    TypeInfo,
    derive_more::From,
    derive_more::Into,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct PanicBuffer(Payload);

impl PanicBuffer {
    /// Returns ref to the internal data.
    pub fn inner(&self) -> &Payload {
        &self.0
    }

    fn to_limited_str(&self) -> Option<LimitedStr<'_>> {
        let s = core::str::from_utf8(&self.0).ok()?;
        LimitedStr::try_from(s).ok()
    }
}

impl From<LimitedStr<'_>> for PanicBuffer {
    fn from(value: LimitedStr) -> Self {
        const _: () = assert!(crate::str::TRIMMED_MAX_LEN <= MAX_PAYLOAD_SIZE);
        Payload::try_from(value.into_inner().into_owned().into_bytes())
            .map(Self)
            .unwrap_or_else(|LimitedVecError| {
                unreachable!("`LimitedStr` is always smaller than maximum payload size",)
            })
    }
}

impl Display for PanicBuffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(s) = self.to_limited_str() {
            Display::fmt(&s, f)
        } else {
            Display::fmt(&self.0, f)
        }
    }
}

impl Debug for PanicBuffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(s) = self.to_limited_str() {
            Debug::fmt(s.as_str(), f)
        } else {
            Debug::fmt(&self.0, f)
        }
    }
}

#[cfg(test)]
mod test {
    use super::{LimitedVec, PanicBuffer, Payload};
    use alloc::{format, string::String, vec, vec::Vec};
    use core::convert::{TryFrom, TryInto};

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
    fn test_new_default() {
        let x = LimitedVec::<String, N>::try_new_default(N).unwrap();
        assert!(
            LimitedVec::<u64, N>::try_new_default(N + 1).is_err(),
            "Must be error because of size overflow"
        );
        let z = LimitedVec::<Vec<u8>, N>::try_new_default(0).unwrap();

        assert_eq!(x.len(), N);
        assert_eq!(z.len(), 0);
        assert_eq!(x[N / 2], "");
    }

    #[test]
    fn test_prepend_works() {
        let mut buf = TestBuffer::try_from(vec![1, 2, 3, 4, 5]).unwrap();
        let prepend_buf = TestBuffer::try_from(vec![6, 7, 8]).unwrap();
        buf.try_prepend(prepend_buf).unwrap();

        assert_eq!(buf.as_ref(), &[6, 7, 8, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_full() {
        let mut x = TestBuffer::try_from(vec![1; N]).unwrap();
        let mut y = TestBuffer::try_from(vec![2; N / 2]).unwrap();
        let mut z = TestBuffer::try_from(vec![3; 0]).unwrap();

        x.try_push(42).unwrap_err();
        y.try_push(42).unwrap();
        z.try_push(42).unwrap();

        x.try_extend_from_slice(&[1, 2, 3]).unwrap_err();
        y.try_extend_from_slice(&[1, 2, 3]).unwrap();
        z.try_extend_from_slice(&[1, 2, 3]).unwrap();

        x.try_prepend(vec![1, 2, 3].try_into().unwrap())
            .unwrap_err();
        y.try_prepend(vec![1, 2, 3].try_into().unwrap()).unwrap();
        z.try_prepend(vec![1, 2, 3].try_into().unwrap()).unwrap();

        z[0] = 0;

        assert_eq!(&z.into_vec(), &[0, 2, 3, 42, 1, 2, 3]);
        assert_eq!(TestBuffer::max_len(), N);
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

    fn panic_buf(bytes: &[u8]) -> PanicBuffer {
        Payload::try_from(bytes).map(PanicBuffer).unwrap()
    }

    #[test]
    fn panic_buffer_debug() {
        let buf = panic_buf(b"Hello, world!");
        assert_eq!(format!("{buf:?}"), r#""Hello, world!""#);

        let buf = panic_buf(b"\xE0\x80\x80");
        assert_eq!(format!("{buf:?}"), "0xe08080");
    }

    #[test]
    fn panic_buffer_display() {
        let buf = panic_buf(b"Hello, world!");
        assert_eq!(format!("{buf}"), "Hello, world!");

        let buf = panic_buf(b"\xE0\x80\x80");
        assert_eq!(format!("{buf}"), "0xe08080");
    }
}
