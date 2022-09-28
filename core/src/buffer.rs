// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use core::{convert::TryFrom, fmt::Display, marker::PhantomData};

use alloc::{vec, vec::Vec};
use codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Limited len vector.
/// `T` is data type.
/// `E` is overflow error type.
/// `N` is max len which a vector can have.
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct LimitVec<T, E, const N: usize>(Vec<T>, PhantomData<E>);

impl<T, E: Default, const N: usize> TryFrom<Vec<T>> for LimitVec<T, E, N> {
    type Error = E;
    fn try_from(x: Vec<T>) -> Result<Self, Self::Error> {
        if x.len() > N {
            Err(E::default())
        } else {
            Ok(Self(x, Default::default()))
        }
    }
}

impl<T: Clone + Default, E: Default, const N: usize> LimitVec<T, E, N> {
    /// Returns new limited vector with default initialized elements.
    pub fn new_empty(len: usize) -> Result<Self, E> {
        if len > N {
            Err(E::default())
        } else {
            Ok(Self(vec![T::default(); len], Default::default()))
        }
    }

    /// Append `value` to the end of vector.
    pub fn push(&mut self, value: T) -> Result<(), E> {
        if self.0.len() == N {
            Err(E::default())
        } else {
            self.0.push(value);
            Ok(())
        }
    }

    /// Append `values` to the end of vector.
    pub fn extend_from_slice(&mut self, values: &[T]) -> Result<(), E> {
        if self
            .0
            .len()
            .checked_add(values.len())
            .ok_or_else(E::default)?
            > N
        {
            Err(E::default())
        } else {
            self.0.extend_from_slice(values);
            Ok(())
        }
    }

    /// Append `values` to the begin of vector.
    pub fn prepend(&mut self, values: Self) -> Result<(), E> {
        if self
            .0
            .len()
            .checked_add(values.0.len())
            .ok_or_else(E::default)?
            > N
        {
            Err(E::default())
        } else {
            self.0.splice(0..0, values.0);
            Ok(())
        }
    }

    /// Returns ref to the internal data.
    pub fn get(&self) -> &[T] {
        &self.0
    }

    /// Returns mut ref to the internal data slice.
    pub fn get_mut(&mut self) -> &mut [T] {
        &mut self.0
    }

    /// Destruct limited vector and returns inner vector.
    pub fn into_vec(self) -> Vec<T> {
        self.0
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

/// Payload size exceed error
#[derive(
    Clone, Copy, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo,
)]
pub struct RuntimeBufferSizeError;

impl From<RuntimeBufferSizeError> for &str {
    fn from(_: RuntimeBufferSizeError) -> Self {
        "Runtime buffer size exceed"
    }
}

impl Display for RuntimeBufferSizeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str((*self).into())
    }
}

/// Buffer which size cannot be bigger then max allowed allocation size in runtime.
pub type RuntimeBuffer = LimitVec<u8, RuntimeBufferSizeError, RUNTIME_MAX_BUFF_SIZE>;

#[cfg(test)]
mod test {
    use super::{LimitVec, RuntimeBufferSizeError};
    use alloc::{string::String, vec, vec::Vec};
    use core::convert::{TryFrom, TryInto};

    const N: usize = 1000;
    type TestBuffer = LimitVec<u8, RuntimeBufferSizeError, N>;

    #[test]
    fn test_try_from() {
        let v1 = vec![1; N];
        let v2 = vec![1; N + 1];
        let v3 = vec![1; N - 1];

        let x = TestBuffer::try_from(v1).unwrap();
        let _ = TestBuffer::try_from(v2).expect_err("Must be err because of size overflow");
        let z = TestBuffer::try_from(v3).unwrap();

        assert_eq!(x.get().len(), N);
        assert_eq!(z.get().len(), N - 1);
        assert_eq!(x.get()[N / 2], 1);
        assert_eq!(z.get()[N / 2], 1);
    }

    #[test]
    fn test_new_empty() {
        let x = LimitVec::<String, RuntimeBufferSizeError, N>::new_empty(N).unwrap();
        let _ = LimitVec::<u64, RuntimeBufferSizeError, N>::new_empty(N + 1)
            .expect_err("Must be error because of size overflow");
        let z = LimitVec::<Vec<u8>, RuntimeBufferSizeError, N>::new_empty(0).unwrap();

        assert_eq!(x.get().len(), N);
        assert_eq!(z.get().len(), 0);
        assert_eq!(x.get()[N / 2], "");
    }

    #[test]
    fn test_full() {
        let mut x = TestBuffer::try_from(vec![1; N]).unwrap();
        let mut y = TestBuffer::try_from(vec![2; N / 2]).unwrap();
        let mut z = TestBuffer::try_from(vec![3; 0]).unwrap();

        x.push(42).unwrap_err();
        y.push(42).unwrap();
        z.push(42).unwrap();

        x.extend_from_slice(&[1, 2, 3]).unwrap_err();
        y.extend_from_slice(&[1, 2, 3]).unwrap();
        z.extend_from_slice(&[1, 2, 3]).unwrap();

        x.prepend(vec![1, 2, 3].try_into().unwrap()).unwrap_err();
        y.prepend(vec![1, 2, 3].try_into().unwrap()).unwrap();
        z.prepend(vec![1, 2, 3].try_into().unwrap()).unwrap();

        z.get_mut()[0] = 0;

        assert_eq!(&z.into_vec(), &[0, 2, 3, 42, 1, 2, 3]);
        assert_eq!(TestBuffer::max_len(), N);
    }
}
