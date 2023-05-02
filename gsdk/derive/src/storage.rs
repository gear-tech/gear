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
    MetaNameValue, PatType, Type,
};

const FULL_DOC_SUFFIX: &str = " at specified block";

/// Storage query builder for generating
/// - basic private shared query
/// - full query with block hash
/// - short query with block hash None.
pub struct StorageQueryBuilder(ItemFn);

impl StorageQueryBuilder {
    /// Build private storage query.
    fn private(&self) -> ItemFn {
        let mut private = self.0.clone();
        private.sig.ident = private.sig.ident.to_priv();

        private
    }

    /// Build full storage query.
    fn full(&self) -> ItemFn {
        let mut full = self.0.clone();

        // reset function docs.
        //
        // - `PatType(block_hash: Option<H256>)` -> `block_hash: H256`.
        full.sig.inputs.iter_mut().find_map(|v| {
            if let FnArg::Typed(PatType { pat, ty, .. }) = v {
                if !pat.to_token_stream().to_string().contains("block_hash") {
                    return None;
                }

                *ty = Box::new(Type::Verbatim(quote! { H256 }));
                return Some(());
            }

            None
        });

        // reset function block.
        //
        // ```ignore
        // {
        //   self.storage_query_at(..args, Some(block_hash));
        // }
        // ```
        {
            let fn_at = &self.0.sig.ident.to_priv();
            let mut args = full.typed_args();
            // # Safty
            //
            // panic here is expected bcz the process will be failed anyway
            // if we don't have `block_hash` as the last argument.
            let block_hash = args.pop().expect("block_hash is not found");

            full.block.stmts = parse_quote! {
                self.#fn_at(#(#args,)* Some(#block_hash)).await
            };
        }

        full
    }

    /// Build short storage query.
    fn short(&self) -> ItemFn {
        let mut short = self.0.clone();

        // reset function docs.
        //
        // - remove `at specified block` suffix.
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
                *lit_str = LitStr::new(
                    &lit_str.value().replace(FULL_DOC_SUFFIX, ""),
                    lit_str.span(),
                );
                return Some(());
            }

            None
        });

        // reset function name.
        //
        // - `storage_query_at` -> `storage_query`
        short.sig.ident = short.sig.ident.to_short();

        // reset function inputs.
        //
        // - remove `block_hash: Option<H256>`
        short.sig.inputs = Punctuated::from_iter(short.sig.inputs.into_iter().filter(|v| {
            if let FnArg::Typed(PatType { pat, .. }) = v {
                return !pat.to_token_stream().to_string().contains("block_hash");
            }

            true
        }));

        // reset function block.
        //
        // ```ignore
        // {
        //   self.storage_query(..args, None);
        // }
        // ```
        {
            let fn_at = &self.0.sig.ident.to_priv();
            let args = short.typed_args();

            short.block.stmts = parse_quote! {
                self.#fn_at(#(#args,)* None).await
            };
        }

        short
    }

    /// Build all storage queries.
    pub fn build(&self) -> TokenStream {
        let (private, full, short) = (self.private(), self.full(), self.short());
        quote! {
            #private

            #full

            #short
        }
        .into()
    }
}

impl From<ItemFn> for StorageQueryBuilder {
    fn from(full: ItemFn) -> Self {
        Self(full)
    }
}

trait QueryIdentConversion {
    const PRIV_PREFIX: &'static str = "_";
    const FULL_SUFFIX: &'static str = "_at";

    fn to_priv(&self) -> Ident;

    fn to_short(&self) -> Ident;
}

impl QueryIdentConversion for Ident {
    fn to_priv(&self) -> Ident {
        Ident::new(&format!("{}{}", Self::PRIV_PREFIX, self), self.span())
    }

    fn to_short(&self) -> Ident {
        Ident::new(self.to_string().trim_end_matches("_at"), self.span())
    }
}

trait Function {
    fn typed_args(&self) -> Vec<Ident>;
}

impl Function for ItemFn {
    fn typed_args(&self) -> Vec<Ident> {
        self.sig
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
            .collect::<Vec<Ident>>()
    }
}
