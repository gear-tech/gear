// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use proc_macro::TokenStream;
use syn::ItemFn;

mod at_block;

/// Generate query for the latest state for functions that
/// query something at specified block.
///
/// # Note
///
/// - the docs must be end with `at specified block.`
/// - the function name must be end with `_at`.
/// - the last argument must be `Option<H256>`.
///
/// # Example
///
/// ```ignore
/// /// Imdocs at specified block.
/// #[at_block]
/// pub fn query_at(addr: Address, block_hash: Option<H256>) -> R {
///     // ...
/// }
/// ```
///
/// will generate functions
///
/// ```ignore
/// /// Imdocs at specified block.
/// pub fn query_at(addr: Address, block_hash: impl Into<Option<H256>>) -> R {
///     // ...
/// }
///
/// /// Imdocs.
/// pub fn query(addr: Address) -> R {
///     query_at(addr, None)
/// }
/// ```
#[proc_macro_attribute]
pub fn at_block(_: TokenStream, item: TokenStream) -> TokenStream {
    let raw: ItemFn = syn::parse_macro_input!(item);
    at_block::AtBlockBuilder::from(raw).build()
}
