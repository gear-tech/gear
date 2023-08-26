// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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
use syn::{
    fold::Fold, parse::Parse, parse_quote, punctuated::Punctuated, Block, Expr, ExprCall, ExprPath,
    FnArg, ItemFn, Meta, Pat, PatType, Path, Signature, Token,
};

/// Host function builder.
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
    pub fn build(mut self) -> TokenStream {
        let maybe_wgas = self.meta.fold_item_fn(ItemFn {
            attrs: self.item.attrs.clone(),
            vis: self.item.vis.clone(),
            sig: self.build_sig(),
            block: self.build_block(),
        });

        if !self.meta.wgas {
            return maybe_wgas.into_token_stream().into();
        }

        self.meta.wgas = false;
        let without_gas = ItemFn {
            attrs: self.item.attrs.clone(),
            vis: self.item.vis.clone(),
            sig: self.build_sig(),
            block: self.build_block(),
        };

        quote! {
            #without_gas

            #maybe_wgas
        }
        .into()
    }

    /// Build inputs from the function signature.
    fn build_inputs(&self) -> Vec<FnArg> {
        let mut inputs = self.item.sig.inputs.iter().cloned().collect::<Vec<_>>();

        if matches!(self.meta.call_type, CallType::Fallible) {
            inputs.push(parse_quote!(err_mid_ptr: u32));
        }

        if !self.meta.wgas {
            return inputs;
        }

        let mut injected = false;
        let mut new_inputs = vec![];
        inputs.into_iter().for_each(|a| {
            if let FnArg::Typed(PatType { pat, .. }) = a.clone() {
                if let Pat::Ident(ident) = pat.as_ref() {
                    // TODO #2722
                    if !injected && (ident.ident == "value_ptr" || ident.ident == "delay") {
                        new_inputs.push(parse_quote!(gas_limit: u64));
                        injected = true;
                    }
                }
            }

            new_inputs.push(a);
        });

        new_inputs
    }

    /// Build the signature of the function.
    fn build_sig(&self) -> Signature {
        let name = self.item.sig.ident.clone();
        let inputs = self.build_inputs();
        let output = self.item.sig.output.clone().into_token_stream();

        parse_quote! {
            fn #name(#(#inputs),*) #output
        }
    }

    /// Build the function body.
    fn build_block(&self) -> Box<Block> {
        let mut name = self.item.sig.ident.clone().to_string();
        if self.meta.wgas {
            name += "_wgas";
        }

        let cost = self.meta.runtime_costs.clone();
        let err = self.meta.err.clone();
        let inner_block = self.item.block.clone();
        let inputs = self.build_inputs();

        let run: Expr = match self.meta.call_type {
            CallType::Any => {
                parse_quote! {
                    ctx.run_any(gas, #cost, |ctx| {
                        #inner_block
                    })
                }
            }
            CallType::Fallible => {
                parse_quote! {
                    ctx.run_fallible::<_, _, #err>(gas, err_mid_ptr, #cost, |ctx| {
                        #inner_block
                    })
                }
            }
        };

        let mut log_args: Vec<Expr> = vec![parse_quote!(#name)];
        log_args.extend(
            inputs
                .into_iter()
                .skip(1)
                .filter_map(|a| match a {
                    FnArg::Typed(PatType { pat, .. }) => match pat.as_ref() {
                        Pat::Ident(ident) => Some(Expr::Path(ExprPath {
                            attrs: Default::default(),
                            qself: None,
                            path: Path::from(ident.clone().ident),
                        })),
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Vec<_>>(),
        );

        parse_quote! ({
            syscall_trace!(#(#log_args),*);

            #run
        })
    }
}

impl From<HostFn> for TokenStream {
    fn from(host_fn: HostFn) -> Self {
        host_fn.build()
    }
}

/// Call type of the host function.
#[derive(Default)]
pub enum CallType {
    #[default]
    Any,
    Fallible,
}

/// Attribute meta of the host function.
pub struct HostFnMeta {
    /// Call type of the host function.
    pub call_type: CallType,
    /// If the host function is wgas.
    pub wgas: bool,
    /// The runtime costs of the host function.
    runtime_costs: Expr,
    /// The length of the error.
    pub err: Expr,
}

impl Parse for HostFnMeta {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut call_type = Default::default();
        let mut wgas = false;
        let mut runtime_costs = parse_quote!(RuntimeCosts::Null);
        let mut err = parse_quote!(ErrorWithHash);

        let meta_list = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;
        for meta in meta_list {
            let ident = meta.path().get_ident().expect("Missing ident");
            match ident.to_string().as_ref() {
                "fallible" => call_type = CallType::Fallible,
                "wgas" => wgas = true,
                "cost" => runtime_costs = meta.require_name_value()?.value.clone(),
                "err" => err = meta.require_name_value()?.value.clone(),
                _ => {}
            }
        }

        Ok(Self {
            call_type,
            wgas,
            runtime_costs,
            err,
        })
    }
}

impl Fold for HostFnMeta {
    fn fold_item_fn(&mut self, mut item: ItemFn) -> ItemFn {
        if !self.wgas {
            return item;
        }

        item.sig.ident = Ident::new(
            &(item.sig.ident.to_token_stream().to_string() + "_wgas"),
            Span::call_site(),
        );

        item.block = Box::new(self.fold_block(*item.block));
        item
    }

    fn fold_expr_call(&mut self, mut expr: ExprCall) -> ExprCall {
        if !self.wgas {
            return expr;
        }

        if let Expr::Path(ExprPath {
            path: Path { segments, .. },
            ..
        }) = &mut *expr.func
        {
            if segments
                .first()
                .map(|i| i.to_token_stream().to_string().ends_with("Packet"))
                != Some(true)
            {
                return expr;
            }

            if let Some(new) = segments.last_mut() {
                new.ident = Ident::new("new_with_gas", Span::call_site());
            }
        }

        if let Some(value) = expr.args.pop() {
            expr.args.push(parse_quote!(gas_limit));
            expr.args.push(value.value().clone());
        }

        expr
    }
}
