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
use proc_macro2::Span;
use syn::{parse_quote, punctuated::Punctuated, token::Comma, Expr, Ident};

/// Appends suffix to ident
pub fn with_suffix(ident: &Ident, suffix: &str) -> Ident {
    let mut name = ident.to_string();
    name.push_str(suffix);
    Ident::new(&name, Span::call_site())
}

/// Get arguments from the inputs for function signature
pub fn get_args(inputs: &Punctuated<syn::FnArg, syn::token::Comma>) -> Expr {
    let idents = inputs.iter().filter_map(|param| {
        if let syn::FnArg::Typed(pat_type) = param {
            if let syn::Pat::Ident(pat_ident) = *pat_type.pat.clone() {
                return Some(pat_ident.ident);
            }
        }
        None
    });

    let mut punctuated: Punctuated<syn::Ident, Comma> = Punctuated::new();
    idents.for_each(|ident| punctuated.push(ident));

    parse_quote!(( #punctuated ))
}
