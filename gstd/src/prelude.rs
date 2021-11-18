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

//! The `gstd` default prelude.

extern crate alloc;

pub use core::prelude::v1::*;

// Public module re-exports
pub use alloc::borrow;
pub use alloc::boxed;
pub use alloc::collections;
pub use alloc::fmt;
pub use alloc::format;
pub use alloc::rc;
pub use alloc::slice;
pub use alloc::string;
pub use alloc::vec;
pub use core::any;
pub use core::cell;
pub use core::clone;
pub use core::cmp;
pub use core::convert;
pub use core::default;
pub use core::future;
pub use core::hash;
pub use core::hint;
pub use core::iter;
pub use core::marker;
pub use core::mem;
pub use core::ops;
pub use core::pin;
pub use core::ptr;

// Re-exported types and traits
pub use alloc::str::FromStr;
pub use borrow::ToOwned;
pub use boxed::Box;
pub use codec::{Decode, Encode};
pub use collections::{BTreeMap, BTreeSet, VecDeque};
pub use convert::{Into, TryInto};
pub use scale_info::TypeInfo;
pub use string::{String, ToString};
pub use vec::Vec;
