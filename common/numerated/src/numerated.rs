// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

//! [Numerated], [Bound] traits definition and implementation for integer types.

use num_traits::{bounds::UpperBounded, One, PrimInt, Unsigned};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundValue<T: Sized> {
    /// The bound is a value.
    Value(T),
    /// The bound is an upper bound. Contains `T` max value.
    Upper(T),
}

/// For any type `T`, `Bound<T>` is a type, which has set of values bigger than `T` by one element.
/// - Each value from `T` has unambiguous mapping to `Bound<T>`.
/// - Each value from `Bound<T>`, except one called __upper__, has unambiguous mapping to `T`.
/// - __upper__ value has no mapping to `T`, but can be used to get `T` max value.
///
/// # Examples
/// 1) For any `T`, which max value can be get by calling some static live time function,
/// Option<T> can be used as `Bound<T>`. `None` is __upper__. Mapping: Some(t) -> t, t -> Some(t).
///
/// 2) When `inner` field max value is always smaller than `inner` type max value, then we can use this variant:
/// ```
/// use numerated::{Bound, BoundValue};
///
/// /// `inner` is a value from 0 to 99.
/// struct T { inner: u32 }
///
/// /// `inner` is a value from 0 to 100.
/// #[derive(Clone, Copy)]
/// struct B { inner: u32 }
///
/// impl From<T> for B {
///     fn from(t: T) -> Self {
///         Self { inner: t.inner }
///     }
/// }
///
/// impl Bound<T> for B {
///    fn unbound(self) -> BoundValue<T> {
///        if self.inner == 100 {
///            BoundValue::Upper(T { inner: 99 })
///        } else {
///            BoundValue::Value(T { inner: self.inner })
///        }
///    }
/// }
/// ```
pub trait Bound<T: Sized>: From<T> + Copy {
    /// Unbound means mapping bound back to value if possible.
    /// - In case bound is __upper__, then returns Upper(max), where `max` is `T` max value.
    /// - Otherwise returns Value(value).
    fn unbound(self) -> BoundValue<T>;
    fn get(self) -> Option<T> {
        match self.unbound() {
            BoundValue::Value(v) => Some(v),
            BoundValue::Upper(_) => None,
        }
    }
}

/// Numerated type is a type, which has type for distances between any two values of `Self`,
/// and provide an interface to add/subtract distance to/from value.
pub trait Numerated: Copy + Sized + Ord + Eq {
    /// Numerate type: type that describes the distances between two values of `Self`.
    type N: PrimInt + Unsigned;
    /// Bound type: type for which any value can be mapped to `Self`,
    /// and also has __upper__ value, which is bigger than any value of `Self`.
    type B: Bound<Self>;
    /// Adds `num` to `self` if `self + num` is between `self` and `other`.
    fn add_if_between(self, num: Self::N, other: Self) -> Option<Self>;
    /// Subtracts `num` from `self` if `self - num` is between `self` and `other`.
    fn sub_if_between(self, num: Self::N, other: Self) -> Option<Self>;
    /// Returns `self - other` if `self >= other`.
    fn distance(self, other: Self) -> Option<Self::N>;
    /// Increments `self` if `self < other`.
    fn inc_if_lt(self, other: Self) -> Option<Self> {
        self.add_if_between(Self::N::one(), other)
    }
    /// Decrements `self` if `self > other`.
    fn dec_if_gt(self, other: Self) -> Option<Self> {
        self.sub_if_between(Self::N::one(), other)
    }
    fn is_between(self, a: Self, b: Self) -> bool {
        self <= a.max(b) && self >= a.min(b)
    }
}

impl<T> From<T> for BoundValue<T> {
    fn from(value: T) -> Self {
        Self::Value(value)
    }
}

impl<T: UpperBounded> From<Option<T>> for BoundValue<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Value(value),
            None => Self::Upper(T::max_value()),
        }
    }
}

impl<T: Copy> Bound<T> for BoundValue<T> {
    fn unbound(self) -> BoundValue<T> {
        self
    }
}

macro_rules! impl_for_unsigned {
    ($($t:ty)*) => ($(
        impl Numerated for $t {
            type N = $t;
            type B = BoundValue<$t>;
            fn add_if_between(self, num: Self::N, other: Self) -> Option<Self> {
                self.checked_add(num).and_then(|res| res.is_between(self, other).then_some(res))
            }
            fn sub_if_between(self, num: Self::N, other: Self) -> Option<Self> {
                self.checked_sub(num).and_then(|res| res.is_between(self, other).then_some(res))
            }
            fn distance(self, other: Self) -> Option<$t> {
                self.checked_sub(other)
            }
        }
    )*)
}

impl_for_unsigned!(u8 u16 u32 u64 u128 usize);

/// Toggles/inverts the most significant bit.
macro_rules! toggle_msb {
    ($num:expr) => {
        $num ^ (1 << (core::mem::size_of_val(&$num) * 8 - 1))
    };
}

macro_rules! impl_for_signed {
    ($($s:ty => $u:ty),*) => {
        $(
            impl Numerated for $s {
                type N = $u;
                type B = BoundValue<$s>;
                fn add_if_between(self, num: $u, other: Self) -> Option<Self> {
                    let a = toggle_msb!(self) as $u;
                    let b = toggle_msb!(other) as $u;
                    a.checked_add(num).and_then(|res| res.is_between(a, b).then_some(toggle_msb!(res) as $s))
                }
                fn sub_if_between(self, num: Self::N, other: Self) -> Option<Self> {
                    let a = toggle_msb!(self) as $u;
                    let b = toggle_msb!(other) as $u;
                    a.checked_sub(num).and_then(|res| res.is_between(a, b).then_some(toggle_msb!(res) as $s))
                }
                fn distance(self, other: Self) -> Option<$u> {
                    let a = toggle_msb!(self) as $u;
                    let b = toggle_msb!(other) as $u;
                    a.checked_sub(b)
                }
            }
        )*
    };
}

impl_for_signed!(i8 => u8, i16 => u16, i32 => u32, i64 => u64, i128 => u128, isize => usize);
