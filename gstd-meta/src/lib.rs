// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

#![no_std]
#![cfg_attr(feature = "strict", deny(warnings))]

extern crate alloc;

pub(crate) use alloc::{boxed::Box, collections::BTreeMap, string::ToString, vec::Vec};

pub use alloc::{string::String, vec};
pub use scale_info::{MetaType, TypeInfo};

mod declare;
mod internal;
mod meta;

mod interaction;
pub use interaction::*;

pub fn to_slice<T>(slice: &[T]) -> *mut [i32; 2] {
    Box::into_raw(Box::new([slice.as_ptr() as _, slice.len() as _]))
}
