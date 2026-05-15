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
use proc_macro2::Span;
use quote::{ToTokens, quote};
use syn::{
    Expr, ExprLit, FnArg, Ident, ItemFn, Lit, LitStr, Meta, MetaNameValue, PatType, parse_quote,
    punctuated::Punctuated,
};

const AT_DOC_SUFFIX: &str = " at specified block";
const AT_SUFFIX: &str = "_at";
const AT_BLOCK_HASH: &str = "Option<H256>";

/// At-block query builder for generating
///
/// - query from the latest state.
/// - query at block hash.
pub struct AtBlockBuilder(ItemFn);

impl AtBlockBuilder {
    /// Build query at specified block with and without Option.
    fn at(&self) -> ItemFn {
        let mut at = self.0.clone();

        // reset block hash argument.
        //
        // - `value: Option<H256>` ->  `value: impl Into<Option<H256>>`
        let ident = if let Some(FnArg::Typed(PatType { ty, pat, .. })) =
            at.sig.inputs.iter_mut().next_back()
        {
            *ty = parse_quote! {
                impl Into<Option<H256>>
            };

            Ident::new(&pat.to_token_stream().to_string(), Span::call_site())
        } else {
            unreachable!("Checked before in function validate");
        };

        // reset function block.
        //
        // - push `let #ident = #ident.into();` to the top of the block.
        let mut stmts = vec![];
        stmts.push(parse_quote! {
            let #ident = #ident.into();
        });
        at.block.stmts = [stmts, at.block.stmts].concat();

        at
    }

    /// Build query for the latest state.
    fn latest(&self) -> ItemFn {
        let mut latest = self.0.clone();

        // reset function docs.
        //
        // - remove `at specified block` suffix.
        latest.attrs.iter_mut().for_each(|attr| {
            if let Meta::NameValue(MetaNameValue {
                value:
                    Expr::Lit(ExprLit {
                        attrs: _,
                        lit: Lit::Str(lit_str),
                    }),
                ..
            }) = &mut attr.meta
            {
                *lit_str = LitStr::new(&lit_str.value().replace(AT_DOC_SUFFIX, ""), lit_str.span());
            }
        });

        // reset function name.
        //
        // - `query_at` -> `query`
        latest.sig.ident = Ident::new(
            latest.sig.ident.to_string().trim_end_matches(AT_SUFFIX),
            latest.sig.ident.span(),
        );

        // reset function inputs.
        //
        // - remove `block_hash: Option<H256>`
        latest.sig.inputs = Punctuated::from_iter(latest.sig.inputs.into_iter().filter(|v| {
            if let FnArg::Typed(PatType { ty, .. }) = v {
                return !ty
                    .to_token_stream()
                    .to_string()
                    .replace(' ', "")
                    .contains(AT_BLOCK_HASH);
            }

            true
        }));

        // reset function block.
        //
        // ```ignore
        // {
        //   self.query(..args, None);
        // }
        // ```
        {
            let fn_at = &self.0.sig.ident;
            let args = latest
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

            latest.block.stmts = parse_quote! {
                self.#fn_at(#(#args,)* None).await
            };
        }

        latest
    }

    /// Build all queries.
    pub fn build(&self) -> TokenStream {
        let (at, latest) = (self.at(), self.latest());
        quote! {
            #at

            #latest
        }
        .into()
    }

    /// This function validates the input of the query
    /// function, follows the rules below:
    ///
    /// - the docs must be end with `at specified block.`
    /// - the function name must be end with `_at`.
    /// - the last argument must be `Option<H256>`.
    fn validate(fun: &ItemFn) {
        // validate the function docs.
        if !fun.attrs.iter().any(|attr| {
            attr.path().is_ident("doc")
                && attr
                    .meta
                    .require_name_value()
                    .expect("doc attribute must be name value")
                    .value
                    .to_token_stream()
                    .to_string()
                    .ends_with(&(AT_DOC_SUFFIX.to_string() + ".\""))
        }) {
            panic!("the docs must be end with `{AT_DOC_SUFFIX}`");
        }

        // validate the function name.
        if !fun.sig.ident.to_string().ends_with(AT_SUFFIX) {
            panic!("the function name must be end with `_at`");
        }

        // validate the last argument.
        if let Some(FnArg::Typed(PatType { ty, pat, .. })) = fun.sig.inputs.iter().next_back() {
            if !pat.to_token_stream().to_string().contains("block_hash") {
                panic!("the last argument's name must be `block_hash`");
            }

            if ty.to_token_stream().to_string().replace(' ', "") != "Option<H256>" {
                panic!("the last argument's type must be `Option<H256>`");
            };
        } else {
            panic!("the last argument must be `block_hash: Option<H256>`");
        }
    }
}

impl From<ItemFn> for AtBlockBuilder {
    fn from(at: ItemFn) -> Self {
        Self::validate(&at);
        Self(at)
    }
}
