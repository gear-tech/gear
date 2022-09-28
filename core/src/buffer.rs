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

use core::{marker::PhantomData, convert::TryFrom, fmt::Display};

use alloc::vec::Vec;
use alloc::vec;
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
