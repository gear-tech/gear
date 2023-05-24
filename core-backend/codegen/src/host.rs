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
use quote::{quote, ToTokens};
use syn::{parse_quote, Block, Expr, ItemFn, Signature};

/// Host function builder
pub struct HostFn {
    item: ItemFn,
    fallible: bool,
    wgas: bool,
    runtime_costs: Option<Expr>,
}

impl HostFn {
    /// Create a new host function builder.
    pub fn new(item: ItemFn) -> Self {
        Self {
            item,
            fallible: false,
            wgas: false,
            runtime_costs: None,
        }
    }

    /// Set the function as fallible.
    pub fn fallible(mut self) -> Self {
        self.fallible = true;
        self
    }

    /// Set the function as wgas.
    pub fn wgas(mut self) -> Self {
        self.wgas = true;
        self
    }

    /// Set the runtime cost of the function.
    pub fn runtime_costs(mut self, expr: Expr) -> Self {
        self.runtime_costs = Some(expr);
        self
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
        let mut var_str = self.item.sig.ident.to_string();
        var_str
            .get_mut(0..1)
            .expect("Function name must be valid")
            .make_ascii_uppercase();
        let var = Ident::new(&var_str, Span::call_site());

        self.runtime_costs
            .clone()
            .unwrap_or_else(|| parse_quote! { RuntimeCost::#var })
    }

    fn build_block(&self) -> Box<Block> {
        let inner_block = self.item.block.clone();
        let inner_args = self
            .item
            .sig
            .inputs
            .clone()
            .into_iter()
            .skip(1)
            .collect::<Vec<_>>();

        let costs: Expr = {
            // let mut var_str = name.to_string();
            // var_str
            //     .get_mut(0..1)
            //     .expect("Function name must be valid")
            //     .make_ascii_uppercase();
            // let var = Ident::new(&var_str, Span::call_site());
            // self.runtime_costs
            //     .unwrap_or_else(|| parse_quote! { RuntimeCost::#var })

            parse_quote!(RuntimeCosts::send(len))
        };

        parse_quote! ({
            let func = move |
                caller: Caller<'_, HostState<E>>,
                #(#inner_args,)*
                err_mid_ptr: u32,
            | -> EmptyOutput {
                // syscall_trace!(name.to_string(), pid_value_ptr, payload_ptr, len, delay, err_mid_ptr);
                let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

                ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::Send(len), |ctx| {
                    #inner_block.map_err(Into::into)
                })
            };

            Func::wrap(store, func)
        })
    }
}

impl From<ItemFn> for HostFn {
    fn from(item: ItemFn) -> Self {
        Self::new(item)
    }
}

impl From<HostFn> for TokenStream {
    fn from(host_fn: HostFn) -> Self {
        host_fn.build()
    }
}
