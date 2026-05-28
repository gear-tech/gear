// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Module for limiter implementation.
//!
//! Limiter provides API for continually descending value.
//! Could be used to prevent (limit) some value from overflow.

use crate::storage::ValueStorage;
use core::marker::PhantomData;

/// Represents logic of limiting value decreasing.
pub trait Limiter {
    /// Limiter stored type.
    type Value;

    /// Decreases stored value.
    ///
    /// Should be safe from overflow.
    fn decrease(value: Self::Value);

    /// Returns stored value, if present,
    /// or default/nullable value.
    fn get() -> Self::Value;

    /// Puts given value into stored, to start from
    /// new one for future decreasing.
    fn put(value: Self::Value);
}

/// `Limiter` implementation based on `ValueStorage`.
///
/// Generic parameter `T` represents inner storing type.
pub struct LimiterImpl<T, VS: ValueStorage<Value = T>>(PhantomData<VS>);

/// Crate local macro for repeating `Limiter` implementation
/// over `ValueStorage` with Rust's numeric types.
macro_rules! impl_limiter {
    ($($t: ty), +) => { $(
        impl<VS: ValueStorage<Value = $t>> Limiter for LimiterImpl<$t, VS> {
            type Value = VS::Value;

            fn decrease(value: Self::Value) {
                VS::mutate(|opt_val| {
                    if let Some(val) = opt_val {
                        *val = val.saturating_sub(value);
                    }
                });
            }

            fn get() -> Self::Value {
                VS::get().unwrap_or(0)
            }

            fn put(value: Self::Value) {
                VS::put(value);
            }
        }
    ) + };
}

// Implementation for unsigned integers.
impl_limiter!(u8, u16, u32, u64, u128);

// Implementation for signed integers.
impl_limiter!(i8, i16, i32, i64, i128);
