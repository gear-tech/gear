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

//! Gear macros.

mod bail;
mod debug;
mod export;
mod metadata;

pub mod util {
    use crate::prelude::{Box, String, Vec};
    use codec::Encode;
    use scale_info::{PortableRegistry, Registry};

    pub use scale_info::MetaType;

    pub fn to_hex_registry(meta_types: Vec<MetaType>) -> String {
        let mut registry = Registry::new();
        registry.register_types(meta_types);

        let registry: PortableRegistry = registry.into();
        hex::encode(registry.encode())
    }

    pub fn to_wasm_ptr<T: AsRef<[u8]>>(bytes: T) -> *mut [i32; 2] {
        Box::into_raw(Box::new([
            bytes.as_ref().as_ptr() as _,
            bytes.as_ref().len() as _,
        ]))
    }
}
