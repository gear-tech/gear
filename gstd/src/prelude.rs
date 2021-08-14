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

#[global_allocator]
pub static ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

pub use core::{mem, panic, ptr};

extern crate alloc;

pub use alloc::{
    borrow::ToOwned,
    boxed::Box,
    collections::{BTreeMap, VecDeque},
    format,
    str::FromStr,
    string::{String, ToString},
    vec,
    vec::Vec,
};

pub mod meta {
    pub use crate::prelude::{vec, BTreeMap, Box, String, Vec};

    pub use scale_info::{IntoPortable, PortableRegistry, Registry, TypeInfo};

    pub use serde_json::{json, Value};
}
