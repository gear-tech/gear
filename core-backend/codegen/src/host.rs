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
use proc_macro2::{Ident, Span};
use quote::ToTokens;
use syn::{
    parse::Parse, parse_quote, punctuated::Punctuated, Block, Expr, FnArg, ItemFn, Meta, Pat,
    PatType, Signature, Token,
};

/// Host function builder
pub struct HostFn {
    item: ItemFn,
    meta: HostFnMeta,
}

impl HostFn {
    /// Create a new host function builder.
    pub fn new(meta: HostFnMeta, item: ItemFn) -> Self {
        Self { item, meta }
    }

    /// Build the host function.
    pub fn build(self) -> TokenStream {
        ItemFn {
            attrs: self.item.attrs.clone(),
            vis: self.item.vis.clone(),
            sig: self.build_sig(),
            block: self.build_block(),
        }
        .to_token_stream()
        .into()
    }

    /// Build the signature of the function.
    fn build_sig(&self) -> Signature {
        let name = self.item.sig.ident.clone();
        parse_quote! {
            fn #name(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func
        }
    }

    /// Build the runtime costs of the function.
    fn build_runtime_costs(&self) -> Expr {
        if let Some(runtime_costs) = self.meta.runtime_costs.clone() {
            return runtime_costs;
        }

        let mut var_str = self.item.sig.ident.to_string();
        var_str
            .get_mut(0..1)
            .expect("Function name must be valid")
            .make_ascii_uppercase();
        let var = Ident::new(&var_str, Span::call_site());

        parse_quote!(RuntimeCost::#var)
    }

    fn build_block(&self) -> Box<Block> {
        let name = self.item.sig.ident.clone().to_string();
        let inner_block = self.item.block.clone();
        let inputs = self.item.sig.inputs.iter();
        let inner_args = inputs.clone().skip(1).collect::<Vec<_>>();
        let log_args = inputs
            .clone()
            .skip(1)
            .filter_map(|a| match a {
                FnArg::Typed(PatType { pat, .. }) => match pat.as_ref() {
                    Pat::Ident(ident) => Some(ident.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();

        let cost = self.build_runtime_costs();

        // TODO: check fallible

        parse_quote! ({
            let func = move |
                caller: Caller<'_, HostState<E>>,
                #(#inner_args,)*
                err_mid_ptr: u32,
            | -> EmptyOutput {
                syscall_trace!(#name, #(#log_args,)* err_mid_ptr);
                let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

                ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, #cost, |ctx| {
                    #inner_block.map_err(Into::into)
                })
            };

            Func::wrap(store, func)
        })
    }
}

impl From<HostFn> for TokenStream {
    fn from(host_fn: HostFn) -> Self {
        host_fn.build()
    }
}

pub struct HostFnMeta {
    /// If the host function is fallible.
    pub fallible: bool,
    /// If the host function is wgas.
    pub wgas: bool,
    /// The runtime costs of the host function.
    pub runtime_costs: Option<Expr>,
}

impl Parse for HostFnMeta {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut fallible = false;
        let mut wgas = false;
        let mut runtime_costs = None;

        let meta_list = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;
        for meta in meta_list {
            if meta.path().is_ident("fallible") {
                fallible = true;
            } else if meta.path().is_ident("wgas") {
                wgas = true;
            } else if meta.path().is_ident("cost") {
                runtime_costs = Some(meta.require_name_value()?.value.clone());
            }
        }

        Ok(Self {
            fallible,
            wgas,
            runtime_costs,
        })
    }
}
