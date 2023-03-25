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

//! Utility functions.

pub use scale_info::MetaType;
use scale_info::{scale::Encode, PortableRegistry, Registry};

use crate::prelude::{Box, String, Vec};

/// Generate a registry from given meta types and encode it to hex.
pub fn to_hex_registry(meta_types: Vec<MetaType>) -> String {
    let mut registry = Registry::new();
    registry.register_types(meta_types);

    let registry: PortableRegistry = registry.into();
    hex::encode(registry.encode())
}

/// Convert a given reference to a raw pointer.
pub fn to_wasm_ptr<T: AsRef<[u8]>>(bytes: T) -> *mut [i32; 2] {
    Box::into_raw(Box::new([
        bytes.as_ref().as_ptr() as _,
        bytes.as_ref().len() as _,
    ]))
}

/// Convert a given vector to a raw pointer and prevent its deallocating.
///
/// It operates similarly to [`to_wasm_ptr`] except that it consumes the input
/// and make it leak by calling [`core::mem::forget`].
pub fn to_leak_ptr(bytes: impl Into<Vec<u8>>) -> *mut [i32; 2] {
    let bytes = bytes.into();
    let ptr = Box::into_raw(Box::new([bytes.as_ptr() as _, bytes.len() as _]));
    core::mem::forget(bytes);
    ptr
}
