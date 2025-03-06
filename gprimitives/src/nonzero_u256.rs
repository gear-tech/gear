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

//! Definitions of integer that is known not to equal zero.

use crate::U256;
use core::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    mem::transmute,
    num::NonZero,
};
#[cfg(feature = "codec")]
use scale_info::{
    TypeInfo,
    prelude::vec::Vec,
    scale::{Decode, Encode, EncodeLike, Error, Input, Output},
};

/// A value that is known not to equal zero.
#[derive(Clone, Copy)]
#[cfg_attr(feature = "codec", derive(TypeInfo))]
#[repr(transparent)]
pub struct NonZeroU256(U256);

macro_rules! impl_nonzero_fmt {
    ($Trait:ident) => {
        impl fmt::$Trait for NonZeroU256 {
            #[inline]
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.get().fmt(f)
            }
        }
    };
}

impl_nonzero_fmt!(Debug);
impl_nonzero_fmt!(Display);
impl_nonzero_fmt!(LowerHex);
impl_nonzero_fmt!(UpperHex);

impl PartialEq for NonZeroU256 {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl Eq for NonZeroU256 {}

impl PartialOrd for NonZeroU256 {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }

    #[inline]
    fn lt(&self, other: &Self) -> bool {
        self.get() < other.get()
    }

    #[inline]
    fn le(&self, other: &Self) -> bool {
        self.get() <= other.get()
    }

    #[inline]
    fn gt(&self, other: &Self) -> bool {
        self.get() > other.get()
    }

    #[inline]
    fn ge(&self, other: &Self) -> bool {
        self.get() >= other.get()
    }
}

impl Ord for NonZeroU256 {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.get().cmp(&other.get())
    }

    #[inline]
    fn max(self, other: Self) -> Self {
        // SAFETY: The maximum of two non-zero values is still non-zero.
        Self(self.get().max(other.get()))
    }

    #[inline]
    fn min(self, other: Self) -> Self {
        // SAFETY: The minimum of two non-zero values is still non-zero.
        Self(self.get().min(other.get()))
    }

    #[inline]
    fn clamp(self, min: Self, max: Self) -> Self {
        // SAFETY: A non-zero value clamped between two non-zero values is still non-zero.
        Self(self.get().clamp(min.get(), max.get()))
    }
}

impl Hash for NonZeroU256 {
    #[inline]
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.get().hash(state)
    }
}

/// Get a reference to the underlying little-endian words.
impl AsRef<[u64]> for NonZeroU256 {
    #[inline]
    fn as_ref(&self) -> &[u64] {
        self.0.as_ref()
    }
}

impl<'a> From<&'a NonZeroU256> for NonZeroU256 {
    fn from(x: &'a NonZeroU256) -> NonZeroU256 {
        *x
    }
}

impl NonZeroU256 {
    /// The smallest value that can be represented by this non-zero
    pub const MIN: NonZeroU256 = unsafe { NonZeroU256::new_unchecked(U256::one()) };
    /// The largest value that can be represented by this non-zero
    pub const MAX: NonZeroU256 = unsafe { NonZeroU256::new_unchecked(U256::MAX) };

    /// Creates a non-zero if the given value is not zero.
    #[must_use]
    #[inline]
    pub const fn new(n: U256) -> Option<Self> {
        if n.is_zero() { None } else { Some(Self(n)) }
    }

    /// Creates a non-zero without checking whether the value is non-zero.
    /// This results in undefined behaviour if the value is zero.
    ///
    /// # Safety
    ///
    /// The value must not be zero.
    #[must_use]
    #[inline]
    pub const unsafe fn new_unchecked(n: U256) -> Self {
        // SAFETY: The caller guarantees that `n` is non-zero
        unsafe { transmute(n) }
    }

    /// Returns the contained value as a primitive type.
    #[inline]
    pub const fn get(self) -> U256 {
        // FIXME: This can be changed to simply `self.0` once LLVM supports `!range` metadata
        // for function arguments: https://github.com/llvm/llvm-project/issues/76628
        //
        // Rustc can set range metadata only if it loads `self` from
        // memory somewhere. If the value of `self` was from by-value argument
        // of some not-inlined function, LLVM don't have range metadata
        // to understand that the value cannot be zero.
        match Self::new(self.0) {
            Some(Self(n)) => n,
            None => {
                // SAFETY: `NonZero` is guaranteed to only contain non-zero values, so this is unreachable.
                unreachable!()
            }
        }
    }

    /// Adds an unsigned integer to a non-zero value.
    /// Checks for overflow and returns [`None`] on overflow.
    /// As a consequence, the result cannot wrap to zero.
    #[inline]
    pub fn checked_add(self, other: U256) -> Option<Self> {
        // SAFETY:
        // - `checked_add` returns `None` on overflow
        // - `self` is non-zero
        // - the only way to get zero from an addition without overflow is for both
        //   sides to be zero
        //
        // So the result cannot be zero.
        self.get()
            .checked_add(other)
            .map(|result| unsafe { Self::new_unchecked(result) })
    }

    /// Adds an unsigned integer to a non-zero value.
    #[inline]
    pub fn saturating_add(self, other: U256) -> Self {
        // SAFETY:
        // - `saturating_add` returns `u*::MAX` on overflow, which is non-zero
        // - `self` is non-zero
        // - the only way to get zero from an addition without overflow is for both
        //   sides to be zero
        //
        // So the result cannot be zero.
        unsafe { Self::new_unchecked(self.get().saturating_add(other)) }
    }

    /// Addition which overflows and returns a flag if it does.
    #[inline(always)]
    pub fn overflowing_add(self, other: U256) -> (Self, bool) {
        let result = self.get().overflowing_add(other);
        if result.0.is_zero() {
            (Self::MIN, true)
        } else {
            unsafe { (Self::new_unchecked(result.0), result.1) }
        }
    }

    /// Checked subtraction. Returns `None` if overflow occurred.
    pub fn checked_sub(self, other: U256) -> Option<Self> {
        match self.get().overflowing_sub(other) {
            (_, true) => None,
            (val, _) => Self::new(val),
        }
    }

    /// Subtraction which saturates at MIN.
    pub fn saturating_sub(self, other: U256) -> Self {
        match self.get().overflowing_sub(other) {
            (_, true) => Self::MIN,
            (val, false) => Self::new(val).unwrap_or(Self::MIN),
        }
    }

    /// Subtraction which underflows and returns a flag if it does.
    #[inline(always)]
    pub fn overflowing_sub(self, other: U256) -> (Self, bool) {
        let result = self.get().overflowing_sub(other);
        if result.0.is_zero() {
            (Self::MAX, true)
        } else {
            unsafe { (Self::new_unchecked(result.0), result.1) }
        }
    }

    /// Multiplies two non-zero integers together.
    /// Checks for overflow and returns [`None`] on overflow.
    /// As a consequence, the result cannot wrap to zero.
    #[inline]
    pub fn checked_mul(self, other: Self) -> Option<Self> {
        // SAFETY:
        // - `checked_mul` returns `None` on overflow
        // - `self` and `other` are non-zero
        // - the only way to get zero from a multiplication without overflow is for one
        //   of the sides to be zero
        //
        // So the result cannot be zero.
        self.get()
            .checked_mul(other.get())
            .map(|result| unsafe { Self::new_unchecked(result) })
    }

    /// Multiplies two non-zero integers together.
    #[inline]
    pub fn saturating_mul(self, other: Self) -> Self {
        // SAFETY:
        // - `saturating_mul` returns `u*::MAX`/`i*::MAX`/`i*::MIN` on overflow/underflow,
        //   all of which are non-zero
        // - `self` and `other` are non-zero
        // - the only way to get zero from a multiplication without overflow is for one
        //   of the sides to be zero
        //
        // So the result cannot be zero.
        unsafe { Self::new_unchecked(self.get().saturating_mul(other.get())) }
    }

    /// Multiply with overflow, returning a flag if it does.
    #[inline(always)]
    pub fn overflowing_mul(self, other: Self) -> (Self, bool) {
        let result = self.get().overflowing_mul(other.get());
        if result.0.is_zero() {
            (Self::MAX, true)
        } else {
            unsafe { (Self::new_unchecked(result.0), result.1) }
        }
    }

    /// Raises non-zero value to an integer power.
    /// Checks for overflow and returns [`None`] on overflow.
    /// As a consequence, the result cannot wrap to zero.
    #[inline]
    pub fn checked_pow(self, other: U256) -> Option<Self> {
        // SAFETY:
        // - `checked_pow` returns `None` on overflow/underflow
        // - `self` is non-zero
        // - the only way to get zero from an exponentiation without overflow is
        //   for base to be zero
        //
        // So the result cannot be zero.
        self.get()
            .checked_pow(other)
            .map(|result| unsafe { Self::new_unchecked(result) })
    }

    /// Raise non-zero value to an integer power.
    #[inline]
    pub fn overflowing_pow(self, other: U256) -> (Self, bool) {
        let result = self.get().overflowing_pow(other);
        if result.0.is_zero() {
            (Self::MAX, true)
        } else {
            unsafe { (Self::new_unchecked(result.0), result.1) }
        }
    }

    /// Fast exponentiation by squaring
    /// <https://en.wikipedia.org/wiki/Exponentiation_by_squaring>
    ///
    /// # Panics
    ///
    /// Panics if the result overflows the type.
    #[inline]
    pub fn pow(self, other: U256) -> Self {
        // SAFETY:
        // - `pow` panics on overflow/underflow
        // - `self` is non-zero
        // - the only way to get zero from an exponentiation without overflow is
        //   for base to be zero
        //
        // So the result cannot be zero.
        unsafe { Self::new_unchecked(self.get().pow(other)) }
    }
}

impl From<NonZeroU256> for U256 {
    #[inline]
    fn from(nonzero: NonZeroU256) -> Self {
        // Call `get` method to keep range information.
        nonzero.get()
    }
}

macro_rules! impl_map_from {
    ($from:ty) => {
        impl From<$from> for NonZeroU256 {
            fn from(value: $from) -> Self {
                unsafe { Self::new_unchecked(U256::from(value.get())) }
            }
        }
    };
}

impl_map_from!(NonZero<u8>);
impl_map_from!(NonZero<u16>);
impl_map_from!(NonZero<u32>);
impl_map_from!(NonZero<u64>);
impl_map_from!(NonZero<u128>);

macro_rules! impl_try_from {
    ($from:ty) => {
        impl TryFrom<$from> for NonZeroU256 {
            type Error = &'static str;

            #[inline]
            fn try_from(value: $from) -> Result<NonZeroU256, &'static str> {
                NonZeroU256::new(U256::from(value)).ok_or("integer value is zero")
            }
        }
    };
}

impl_try_from!(u8);
impl_try_from!(u16);
impl_try_from!(u32);
impl_try_from!(u64);
impl_try_from!(u128);

#[doc(hidden)]
macro_rules! panic_on_overflow {
    ($name: expr) => {
        if $name {
            panic!("arithmetic operation overflow")
        }
    };
}

impl<T> core::ops::Add<T> for NonZeroU256
where
    T: Into<U256>,
{
    type Output = NonZeroU256;

    fn add(self, other: T) -> NonZeroU256 {
        let (result, overflow) = self.overflowing_add(other.into());
        panic_on_overflow!(overflow);
        result
    }
}

impl<T> core::ops::Add<T> for &NonZeroU256
where
    T: Into<U256>,
{
    type Output = NonZeroU256;

    fn add(self, other: T) -> NonZeroU256 {
        *self + other
    }
}

impl<T> core::ops::AddAssign<T> for NonZeroU256
where
    T: Into<U256>,
{
    fn add_assign(&mut self, other: T) {
        let (result, overflow) = self.overflowing_add(other.into());
        panic_on_overflow!(overflow);
        *self = result
    }
}

impl<T> core::ops::Sub<T> for NonZeroU256
where
    T: Into<U256>,
{
    type Output = NonZeroU256;

    #[inline]
    fn sub(self, other: T) -> NonZeroU256 {
        let (result, overflow) = self.overflowing_sub(other.into());
        panic_on_overflow!(overflow);
        result
    }
}

impl<T> core::ops::Sub<T> for &NonZeroU256
where
    T: Into<U256>,
{
    type Output = NonZeroU256;

    fn sub(self, other: T) -> NonZeroU256 {
        *self - other
    }
}

impl<T> core::ops::SubAssign<T> for NonZeroU256
where
    T: Into<U256>,
{
    fn sub_assign(&mut self, other: T) {
        let (result, overflow) = self.overflowing_sub(other.into());
        panic_on_overflow!(overflow);
        *self = result
    }
}

impl core::ops::Mul<NonZeroU256> for NonZeroU256 {
    type Output = NonZeroU256;

    fn mul(self, other: NonZeroU256) -> NonZeroU256 {
        let (result, overflow) = self.overflowing_mul(other);
        panic_on_overflow!(overflow);
        result
    }
}

impl<'a> core::ops::Mul<&'a NonZeroU256> for NonZeroU256 {
    type Output = NonZeroU256;

    fn mul(self, other: &'a NonZeroU256) -> NonZeroU256 {
        let (result, overflow) = self.overflowing_mul(*other);
        panic_on_overflow!(overflow);
        result
    }
}

impl<'a> core::ops::Mul<&'a NonZeroU256> for &'a NonZeroU256 {
    type Output = NonZeroU256;

    fn mul(self, other: &'a NonZeroU256) -> NonZeroU256 {
        let (result, overflow) = self.overflowing_mul(*other);
        panic_on_overflow!(overflow);
        result
    }
}

impl core::ops::Mul<NonZeroU256> for &NonZeroU256 {
    type Output = NonZeroU256;

    fn mul(self, other: NonZeroU256) -> NonZeroU256 {
        let (result, overflow) = self.overflowing_mul(other);
        panic_on_overflow!(overflow);
        result
    }
}

impl core::ops::MulAssign<NonZeroU256> for NonZeroU256 {
    fn mul_assign(&mut self, other: NonZeroU256) {
        let result = *self * other;
        *self = result
    }
}

#[cfg(feature = "codec")]
macro_rules! impl_for_non_zero {
    ( $( $name:ty ),* $(,)? ) => {
        $(
            impl Encode for $name {
                fn size_hint(&self) -> usize {
                    self.get().size_hint()
                }

                fn encode_to<W: Output + ?Sized>(&self, dest: &mut W) {
                    self.get().encode_to(dest)
                }

                fn encode(&self) -> Vec<u8> {
                    self.get().encode()
                }

                fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
                    self.get().using_encoded(f)
                }
            }

            impl EncodeLike for $name {}

            impl Decode for $name {
                fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
                    Self::new(Decode::decode(input)?)
                        .ok_or_else(|| Error::from("cannot create non-zero number from 0"))
                }
            }
        )*
    }
}

#[cfg(feature = "codec")]
impl_for_non_zero!(NonZeroU256);

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::format;

    #[test]
    fn nonzero_u256_from_to_u256() {
        let u256 = U256::from(42u64);
        let nz = NonZeroU256::new(u256).unwrap();
        assert_eq!(u256, nz.into());
        assert_eq!(format!("{u256}"), format!("{nz}"));
        assert_eq!(format!("{u256:?}"), format!("{nz:?}"));
    }

    #[test]
    fn nonzero_u256_from_nz64() {
        let nzu64 = NonZero::<u64>::new(42u64).unwrap();
        let nz: NonZeroU256 = nzu64.into();
        assert_eq!(U256::from(nzu64.get()), nz.get());
    }

    #[test]
    fn nonzero_u256_from_zero() {
        let zero = 0u64;
        let opt = NonZeroU256::new(U256::from(zero));
        assert_eq!(None, opt);
        let res = TryInto::<NonZeroU256>::try_into(zero);
        assert_eq!(Err("integer value is zero"), res);
    }

    #[test]
    fn nonzero_u256_overflowing_add() {
        let nzu256 = NonZeroU256::MAX;
        let result = nzu256.overflowing_add(1u64.into());
        assert_eq!((NonZeroU256::MIN, true), result);
    }

    #[test]
    fn nonzero_u256_overflowing_sub() {
        let nzu256 = NonZeroU256::MIN;
        let result = nzu256.overflowing_sub(1u64.into());
        assert_eq!((NonZeroU256::MAX, true), result);
    }

    #[test]
    fn nonzero_u256_overflowing_mul() {
        let mut nzu256 = NonZeroU256::from(NonZero::<u128>::MAX);
        nzu256 += 1;
        let result = nzu256.overflowing_mul(nzu256);
        assert_eq!((NonZeroU256::MAX, true), result);
    }

    #[test]
    fn nonzero_u256_overflowing_pow() {
        let mut nzu256 = NonZeroU256::from(NonZero::<u128>::MAX);
        nzu256 += 1;
        let result: (NonZeroU256, bool) = nzu256.overflowing_pow(2.into());
        assert_eq!((NonZeroU256::MAX, true), result);
    }
}
