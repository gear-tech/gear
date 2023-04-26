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

//! The `gstd` default prelude. Re-imports default `std` modules and traits.
//! `std` can be safely replaced to `gstd` in the Rust programs.

extern crate alloc;

pub use core::prelude::v1::*;

// Public module re-exports
pub use alloc::{borrow, boxed, collections, fmt, format, rc, slice, string, vec};
pub use core::{
    any, cell, clone, cmp, convert, default, future, hash, hint, iter, marker, mem, ops, pin, ptr,
};

// Re-exported types and traits
pub use alloc::str::FromStr;
pub use borrow::ToOwned;
pub use boxed::Box;
pub use collections::{BTreeMap, BTreeSet, VecDeque};
pub use convert::{Into, TryInto};
pub use hashbrown::HashMap;
pub use scale_info::{
    self,
    scale::{self as codec, Decode, Encode},
    TypeInfo,
};
pub use string::{String, ToString};
pub use vec::Vec;
