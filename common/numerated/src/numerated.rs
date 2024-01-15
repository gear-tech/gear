// This file is part of Gear.

// Copyright (C) 2023-2024 Gear Technologies Inc.
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

/// Represents a value or upper bound. Can be in two states:
/// - Value: contains value.
/// - Upper: contains max value for `T`.
///
/// See also trait [Bound].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundValue<T: Sized> {
    /// The bound is a value. Contains `T` value.
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
/// `Option<T>`` can be used as `Bound<T>`. `None` is __upper__. Mapping: Some(t) -> t, t -> Some(t).
///
/// 2) When `inner` field max value is always smaller than `inner` type max value, then we can use this variant:
/// ```
/// use numerated::{Bound, BoundValue};
///
/// /// `inner` is a value from 0 to 99.
/// struct Number { inner: u32 }
///
/// /// `inner` is a value from 0 to 100.
/// #[derive(Clone, Copy)]
/// struct BoundForNumber { inner: u32 }
///
/// impl From<Number> for BoundForNumber {
///     fn from(t: Number) -> Self {
///         Self { inner: t.inner }
///     }
/// }
///
/// impl Bound<Number> for BoundForNumber {
///    fn unbound(self) -> BoundValue<Number> {
///        if self.inner == 100 {
///            BoundValue::Upper(Number { inner: 99 })
///        } else {
///            BoundValue::Value(Number { inner: self.inner })
///        }
///    }
/// }
/// ```
pub trait Bound<T: Sized>: From<T> + Copy {
    /// Unbound means mapping bound back to value if possible.
    /// - In case bound is __upper__, then returns Upper(max), where `max` is `T` max value.
    /// - Otherwise returns Value(value).
    fn unbound(self) -> BoundValue<T>;
    /// Returns `T` if `self` is value, otherwise (self is __upper__) returns `None`.
    fn get(self) -> Option<T> {
        match self.unbound() {
            BoundValue::Value(v) => Some(v),
            BoundValue::Upper(_) => None,
        }
    }
}

/// Numerated type is a type, which has type for distances between any two values of `Self`,
/// and provide an interface to add/subtract distance to/from value.
///
/// Default implementation is provided for all integer types:
/// [i8] [u8] [i16] [u16] [i32] [u32] [i64] [u64] [i128] [u128] [isize] [usize].
pub trait Numerated: Copy + Sized + Ord + Eq {
    /// Numerate type: type that describes the distances between two values of `Self`.
    type N: PrimInt + Unsigned;
    /// Bound type: type for which any value can be mapped to `Self`,
    /// and also has __upper__ value, which is bigger than any value of `Self`.
    type B: Bound<Self>;
    /// Adds `num` to `self`, if `self + num` is enclosed by `self` and `other`.
    ///
    /// # Guaranties
    /// - iff `self + num` is enclosed by `self` and `other`, then returns `Some(_)`.
    /// - iff `self.add_if_enclosed_by(num, other) == Some(a)`,
    /// then `a.sub_if_enclosed_by(num, self) == Some(self)`.
    fn add_if_enclosed_by(self, num: Self::N, other: Self) -> Option<Self>;
    /// Subtracts `num` from `self`, if `self - num` is enclosed by `self` and `other`.
    ///
    /// # Guaranties
    /// - iff `self - num` is enclosed by `self` and `other`, then returns `Some(_)`.
    /// - iff `self.sub_if_enclosed_by(num, other) == Some(a)`,
    /// then `a.add_if_enclosed_by(num, self) == Some(self)`.
    fn sub_if_enclosed_by(self, num: Self::N, other: Self) -> Option<Self>;
    /// Returns `self - other`, if `self ≥ other`.
    ///
    /// # Guaranties
    /// - iff `self ≥ other`, then returns `Some(_)`.
    /// - iff `self == other`, then returns `Some(0)`.
    /// - iff `self.distance(other) == Some(a)`, then
    ///   - `self.sub_if_enclosed_by(a, other) == Some(other)`
    ///   - `other.add_if_enclosed_by(a, self) == Some(self)`
    fn distance(self, other: Self) -> Option<Self::N>;
    /// Increments `self`, if `self < other`.
    fn inc_if_lt(self, other: Self) -> Option<Self> {
        self.add_if_enclosed_by(Self::N::one(), other)
    }
    /// Decrements `self`, if `self` > `other`.
    fn dec_if_gt(self, other: Self) -> Option<Self> {
        self.sub_if_enclosed_by(Self::N::one(), other)
    }
    /// Returns `true`, if `self` is enclosed by `a` and `b`.
    fn enclosed_by(self, a: &Self, b: &Self) -> bool {
        self <= *a.max(b) && self >= *a.min(b)
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
            fn add_if_enclosed_by(self, num: Self::N, other: Self) -> Option<Self> {
                self.checked_add(num).and_then(|res| res.enclosed_by(&self, &other).then_some(res))
            }
            fn sub_if_enclosed_by(self, num: Self::N, other: Self) -> Option<Self> {
                self.checked_sub(num).and_then(|res| res.enclosed_by(&self, &other).then_some(res))
            }
            fn distance(self, other: Self) -> Option<$t> {
                self.checked_sub(other)
            }
        }
    )*)
}

impl_for_unsigned!(u8 u16 u32 u64 u128 usize);

macro_rules! impl_for_signed {
    ($($s:ty => $u:ty),*) => {
        $(
            impl Numerated for $s {
                type N = $u;
                type B = BoundValue<$s>;
                fn add_if_enclosed_by(self, num: $u, other: Self) -> Option<Self> {
                    let res = (self as $u).wrapping_add(num) as $s;
                    res.enclosed_by(&self, &other).then_some(res)
                }
                fn sub_if_enclosed_by(self, num: Self::N, other: Self) -> Option<Self> {
                    let res = (self as $u).wrapping_sub(num) as $s;
                    res.enclosed_by(&self, &other).then_some(res)
                }
                fn distance(self, other: Self) -> Option<$u> {
                    (self >= other).then_some(self.abs_diff(other))
                }
            }
        )*
    };
}

impl_for_signed!(i8 => u8, i16 => u16, i32 => u32, i64 => u64, i128 => u128, isize => usize);
