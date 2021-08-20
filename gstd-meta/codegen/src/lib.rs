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

extern crate alloc;
extern crate proc_macro;

use alloc::string::{String, ToString};
use proc_macro::*;

#[proc_macro_attribute]
pub fn gear_data(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let derive_string = String::from("#[derive(Deserialize, Serialize, TypeInfo)]");
    let item_string = item.to_string();

    let mut token = String::with_capacity(derive_string.len() + item_string.len());

    token.push_str(&derive_string);
    token.push_str(&item_string);

    if let Ok(token) = token.parse() {
        return token;
    }

    core::panic!("An error occured while deriving")
}
