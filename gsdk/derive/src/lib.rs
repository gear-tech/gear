// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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
use proc_macro2::Span;
use quote::{quote, ToTokens};
use syn::{
    parse_quote, punctuated::Punctuated, Expr, ExprLit, FnArg, Ident, ItemFn, Lit, LitStr, Meta,
    MetaNameValue, PatType,
};

const DOC_SUFFIX: &str = " at specified block";

/// Converts query_storage_at functions into short functions
/// without block_hash parameter.
#[proc_macro_attribute]
pub fn short(_: TokenStream, item: TokenStream) -> TokenStream {
    let long: ItemFn = syn::parse_macro_input!(item);
    let mut short = long.clone();

    // reset function docs.
    short.attrs.iter_mut().find_map(|attr| {
        if let Meta::NameValue(MetaNameValue {
            value:
                Expr::Lit(ExprLit {
                    attrs: _,
                    lit: Lit::Str(lit_str),
                }),
            ..
        }) = &mut attr.meta
        {
            *lit_str = LitStr::new(&lit_str.value().replace(DOC_SUFFIX, ""), lit_str.span());
            return Some(());
        }

        None
    });

    // reset function name.
    short.sig.ident = Ident::new(
        short.sig.ident.to_string().trim_end_matches("_at"),
        long.sig.ident.span(),
    );

    // reset function inputs.
    short.sig.inputs = Punctuated::from_iter(short.sig.inputs.into_iter().filter(|v| {
        if let FnArg::Typed(PatType { pat, .. }) = v {
            return !pat.to_token_stream().to_string().contains("block_hash");
        }

        true
    }));

    // reset function block.
    {
        let fn_at = &long.sig.ident;
        let args = short
            .sig
            .inputs
            .iter()
            .filter_map(|v| {
                if let FnArg::Typed(PatType { pat, .. }) = v {
                    Some(Ident::new(
                        &pat.to_token_stream().to_string(),
                        Span::call_site(),
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<Ident>>();

        short.block.stmts = parse_quote! {
            self.#fn_at(#(#args,)* None).await
        };
    }

    quote! {
        #long

        #short
    }
    .into()
}
