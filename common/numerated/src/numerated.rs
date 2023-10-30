use num_traits::{bounds::UpperBounded, One, PrimInt, Unsigned};

#[derive(Clone, Copy, PartialEq, Eq)]
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

pub trait Numerated: Copy + Sized + Ord + Eq {
    type N: PrimInt + Unsigned;
    type B: Bound<Self>;
    // +_+_+ rename to add_if_le
    fn raw_add_if_lt(self, num: Self::N, other: Self) -> Option<Self>;
    fn raw_sub_if_gt(self, num: Self::N, other: Self) -> Option<Self>;
    fn sub(self, other: Self) -> Option<Self::N>;
    fn inc_if_lt(self, other: Self) -> Option<Self> {
        self.raw_add_if_lt(Self::N::one(), other).map(|res| {
            debug_assert!(res > self && res <= other);
            res
        })
    }
    fn dec_if_gt(self, other: Self) -> Option<Self> {
        self.raw_sub_if_gt(Self::N::one(), other).map(|res| {
            debug_assert!(res < self && res >= other);
            res
        })
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
            fn raw_add_if_lt(self, num: Self::N, other: Self) -> Option<Self> {
                if num == 0 {
                    return Some(self);
                }
                if self < other {
                    if other - self >= num {
                        Some(self + num)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            fn raw_sub_if_gt(self, num: Self::N, other: Self) -> Option<Self> {
                if num == 0 {
                    return Some(self);
                }
                if self > other {
                    if self - other >= num {
                        Some(self - num)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            fn sub(self, other: Self) -> Option<$t> {
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
                fn raw_add_if_lt(self, num: $u, other: Self) -> Option<Self> {
                    if num == 0 {
                        return Some(self);
                    }
                    let a = toggle_msb!(self) as $u;
                    let b = toggle_msb!(other) as $u;
                    if a < b && b - a >= num {
                        Some(toggle_msb!(a + num) as $s)
                    } else {
                        None
                    }
                }
                fn raw_sub_if_gt(self, num: Self::N, other: Self) -> Option<Self> {
                    if num == 0 {
                        return Some(self);
                    }
                    let a = toggle_msb!(self) as $u;
                    let b = toggle_msb!(other) as $u;
                    if a > b && a - b >= num {
                        Some(toggle_msb!(a - num) as $s)
                    } else {
                        None
                    }
                }
                fn sub(self, other: Self) -> Option<$u> {
                    let a = toggle_msb!(self) as $u;
                    let b = toggle_msb!(other) as $u;
                    a.checked_sub(b)
                }
            }
        )*
    };
}

impl_for_signed!(i8 => u8, i16 => u16, i32 => u32, i64 => u64, i128 => u128, isize => usize);
