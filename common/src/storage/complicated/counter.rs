// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use crate::storage::ValueStorage;
use core::marker::PhantomData;

pub trait Counter {
    type Value;

    fn decrease();

    fn get() -> Self::Value;

    fn increase();

    fn reset();
}

pub struct CounterImpl<T, VS: ValueStorage<Value = T>>(PhantomData<VS>);

macro_rules! impl_counter {
    ($($t: ty), +) => { $(
        impl<VS: ValueStorage<Value = $t>> Counter for CounterImpl<$t, VS> {
            type Value = VS::Value;

            fn decrease() {
                VS::mutate(|opt_val| {
                    if let Some(val) = opt_val {
                        *val = val.saturating_sub(1);
                    }
                });
            }

            fn get() -> Self::Value {
                VS::get().unwrap_or(0)
            }

            fn increase() {
                VS::mutate(|opt_val| {
                    if let Some(val) = opt_val {
                        *val = val.saturating_add(1);
                    } else {
                        *opt_val = Some(1)
                    }
                });
            }

            fn reset() {
                VS::put(0);
            }
        }
    ) + };
}

impl_counter!(u8, u16, u32, u64, u128);
impl_counter!(i8, i16, i32, i64, i128);
