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

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{quote, ToTokens};

fn compile_error<T: ToTokens>(tokens: T, msg: &str) -> TokenStream {
    syn::Error::new_spanned(tokens, msg)
        .to_compile_error()
        .into()
}

/// Mark async function to be the program entry point.
///
/// ## Usage
///
/// ```ignore
/// #[gstd_async::main]
/// async fn main() {
///     gstd::ext::debug("Hello world");
/// }
/// ```
#[proc_macro_attribute]
pub fn main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let main_fn = syn::parse_macro_input!(item as syn::ItemFn);
    if main_fn.sig.ident != "main" || !main_fn.sig.inputs.is_empty() {
        return compile_error(&main_fn.sig.ident, "wrong main function name and arguments");
    }
    if main_fn.sig.asyncness.is_none() {
        return compile_error(
            &main_fn.sig.fn_token,
            "`async` keyword is missing from the function declaration",
        );
    }

    let body = &main_fn.block;
    let result = quote! {
        #[no_mangle]
        pub unsafe extern "C" fn handle() {
            gstd_async::main_loop(handle_async());
        }
        async fn handle_async() #body
    };
    result.into()
}
