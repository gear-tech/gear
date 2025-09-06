// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

//! [`Numerated`], [`Bound`] traits definition and implementations for integer types.
//! Also [`OptionBound`] type is defined, which can be used as [`Bound`] for any type `T: Numerated`.

use core::cmp::Ordering;
use num_traits::{One, PrimInt, Unsigned};

/// For any type `T`, `Bound<T>` is a type, which has set of values bigger than `T` by one.
/// - Each value from `T` has unambiguous mapping to `Bound<T>`.
/// - Each value from `Bound<T>`, except one called __upper__, has unambiguous mapping to `T`.
/// - __upper__ value has no mapping to `T`, but can be considered as value equal to `T::max_value + 1`.
///
/// # Examples
/// 1) For any `T`, which max value can be get by calling some static live time function,
///    `Option<T>` can be used as `Bound<T>`. `None` is __upper__. Mapping: Some(t) -> t, t -> Some(t).
///
/// 2) When `inner` field max value is always smaller than `inner` type max value, then we can use this variant:
/// ```
/// use numerated::Bound;
///
/// /// `inner` is a value from 0 to 99.
/// struct Number { inner: u32 }
///
/// /// `inner` is a value from 0 to 100.
/// #[derive(Clone, Copy)]
/// struct BoundForNumber { inner: u32 }
///
/// impl From<Option<Number>> for BoundForNumber {
///    fn from(t: Option<Number>) -> Self {
///       Self { inner: t.map(|t| t.inner).unwrap_or(100) }
///    }
/// }
///
/// impl Bound<Number> for BoundForNumber {
///    fn unbound(self) -> Option<Number> {
///        (self.inner < 100).then_some(Number { inner: self.inner })
///    }
/// }
/// ```
pub trait Bound<T: Sized>: From<Option<T>> + Copy {
    /// Unbound means mapping bound back to value if possible.
    /// - In case bound is __upper__, then returns [`None`].
    /// - Otherwise returns `Some(p)`, `p: T`.
    fn unbound(self) -> Option<T>;
}

/// Numerated type is a type, which has type for distances between any two values of `Self`,
/// and provide an interface to add/subtract distance to/from value.
///
/// Default implementation is provided for all integer types:
/// [i8] [u8] [i16] [u16] [i32] [u32] [i64] [u64] [i128] [u128] [isize] [usize].
pub trait Numerated: Copy + Sized + Ord + Eq {
    /// Numerate type: type that describes the distances between two values of `Self`.
    type Distance: PrimInt + Unsigned;
    /// Bound type: type for which any value can be mapped to `Self`,
    /// and also has __upper__ value, which is bigger than any value of `Self`.
    type Bound: Bound<Self>;
    /// Adds `num` to `self`, if `self + num` is enclosed by `self` and `other`.
    ///
    /// # Guaranties
    /// - iff `self + num` is enclosed by `self` and `other`, then returns `Some(_)`.
    /// - iff `self.add_if_enclosed_by(num, other) == Some(a)`,
    ///   then `a.sub_if_enclosed_by(num, self) == Some(self)`.
    fn add_if_enclosed_by(self, num: Self::Distance, other: Self) -> Option<Self>;
    /// Subtracts `num` from `self`, if `self - num` is enclosed by `self` and `other`.
    ///
    /// # Guaranties
    /// - iff `self - num` is enclosed by `self` and `other`, then returns `Some(_)`.
    /// - iff `self.sub_if_enclosed_by(num, other) == Some(a)`,
    ///   then `a.add_if_enclosed_by(num, self) == Some(self)`.
    fn sub_if_enclosed_by(self, num: Self::Distance, other: Self) -> Option<Self>;
    /// Returns a distance between `self` and `other`
    ///
    /// # Guaranties
    /// - iff `self == other`, then returns `0`.
    /// - `self.distance(other) == other.distance(self)`.
    /// - iff `self.distance(other) == a` and `self ≥ other` then
    ///   - `self.sub_if_enclosed_by(a, other) == Some(other)`
    ///   - `other.add_if_enclosed_by(a, self) == Some(self)`
    fn distance(self, other: Self) -> Self::Distance;
    /// Increments `self`, if `self < other`.
    fn inc_if_lt(self, other: Self) -> Option<Self> {
        self.add_if_enclosed_by(Self::Distance::one(), other)
    }
    /// Decrements `self`, if `self` > `other`.
    fn dec_if_gt(self, other: Self) -> Option<Self> {
        self.sub_if_enclosed_by(Self::Distance::one(), other)
    }
    /// Returns `true`, if `self` is enclosed by `a` and `b`.
    fn enclosed_by(self, a: &Self, b: &Self) -> bool {
        self <= *a.max(b) && self >= *a.min(b)
    }
}

/// Bound type for `Option<T>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::From)]
pub struct OptionBound<T>(Option<T>);

impl<T> From<T> for OptionBound<T> {
    fn from(value: T) -> Self {
        Some(value).into()
    }
}

impl<T: Copy> Bound<T> for OptionBound<T> {
    fn unbound(self) -> Option<T> {
        self.0
    }
}

impl<T> PartialOrd for OptionBound<T>
where
    T: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self.0.as_ref(), other.0.as_ref()) {
            (None, None) => Some(Ordering::Equal),
            (None, Some(_)) => Some(Ordering::Greater),
            (Some(_), None) => Some(Ordering::Less),
            (Some(a), Some(b)) => a.partial_cmp(b),
        }
    }
}

impl<T> Ord for OptionBound<T>
where
    T: Ord,
{
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.0.as_ref(), other.0.as_ref()) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => a.cmp(b),
        }
    }
}

impl<T> PartialEq<T> for OptionBound<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &T) -> bool {
        self.0.as_ref().map(|a| a.eq(other)).unwrap_or(false)
    }
}

impl<T> PartialEq<Option<T>> for OptionBound<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Option<T>) -> bool {
        self.0 == *other
    }
}

impl<T> PartialOrd<T> for OptionBound<T>
where
    T: PartialOrd,
{
    fn partial_cmp(&self, other: &T) -> Option<Ordering> {
        self.0
            .as_ref()
            .map(|a| a.partial_cmp(other))
            .unwrap_or(Some(Ordering::Greater))
    }
}

macro_rules! impl_for_unsigned {
    ($($t:ty)*) => ($(
        impl Numerated for $t {
            type Distance = $t;
            type Bound = OptionBound<$t>;
            fn add_if_enclosed_by(self, num: Self::Distance, other: Self) -> Option<Self> {
                self.checked_add(num).and_then(|res| res.enclosed_by(&self, &other).then_some(res))
            }
            fn sub_if_enclosed_by(self, num: Self::Distance, other: Self) -> Option<Self> {
                self.checked_sub(num).and_then(|res| res.enclosed_by(&self, &other).then_some(res))
            }
            fn distance(self, other: Self) -> $t {
                self.abs_diff(other)
            }
        }
    )*)
}

impl_for_unsigned!(u8 u16 u32 u64 u128 usize);

macro_rules! impl_for_signed {
    ($($s:ty => $u:ty),*) => {
        $(
            impl Numerated for $s {
                type Distance = $u;
                type Bound = OptionBound<$s>;
                fn add_if_enclosed_by(self, num: Self::Distance, other: Self) -> Option<Self> {
                    let res = (self as $u).wrapping_add(num) as $s;
                    res.enclosed_by(&self, &other).then_some(res)
                }
                fn sub_if_enclosed_by(self, num: Self::Distance, other: Self) -> Option<Self> {
                    let res = (self as $u).wrapping_sub(num) as $s;
                    res.enclosed_by(&self, &other).then_some(res)
                }
                fn distance(self, other: Self) -> $u {
                    self.abs_diff(other)
                }
            }
        )*
    };
}

impl_for_signed!(i8 => u8, i16 => u16, i32 => u32, i64 => u64, i128 => u128, isize => usize);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_option_bound() {
        let a = OptionBound::from(1);
        let b = OptionBound::from(2);
        let c = OptionBound::from(None);
        assert_eq!(a.unbound(), Some(1));
        assert_eq!(b.unbound(), Some(2));
        assert_eq!(c.unbound(), None);
        assert_eq!(a.partial_cmp(&b), Some(Ordering::Less));
        assert_eq!(b.partial_cmp(&a), Some(Ordering::Greater));
        assert_eq!(a.partial_cmp(&c), Some(Ordering::Less));
        assert_eq!(c.partial_cmp(&a), Some(Ordering::Greater));
        assert_eq!(c.partial_cmp(&c), Some(Ordering::Equal));
        assert_eq!(a.partial_cmp(&2), Some(Ordering::Less));
        assert_eq!(b.partial_cmp(&2), Some(Ordering::Equal));
        assert_eq!(c.partial_cmp(&2), Some(Ordering::Greater));
        assert_eq!(a, 1);
        assert_eq!(b, 2);
        assert_eq!(c, None);
        assert_eq!(a, Some(1));
        assert_eq!(b, Some(2));
    }

    #[test]
    fn test_u8() {
        let a = 1u8;
        let b = 2u8;
        assert_eq!(a.add_if_enclosed_by(1, b), Some(2));
        assert_eq!(a.sub_if_enclosed_by(1, b), None);
        assert_eq!(a.distance(b), 1);
        assert_eq!(a.inc_if_lt(b), Some(2));
        assert_eq!(a.dec_if_gt(b), None);
    }

    #[test]
    fn test_i8() {
        let a = -1i8;
        let b = 1i8;
        assert_eq!(a.add_if_enclosed_by(2, b), Some(1));
        assert_eq!(a.sub_if_enclosed_by(1, b), None);
        assert_eq!(a.distance(b), 2);
        assert_eq!(a.inc_if_lt(b), Some(0));
        assert_eq!(a.dec_if_gt(b), None);
    }
}
