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
use quote::ToTokens;
use syn::{
    parse::Parse, parse_quote, punctuated::Punctuated, Block, Expr, ExprPath, FnArg, ItemFn, Meta,
    Pat, PatType, Path, Signature, Token,
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

    fn build_block(&self) -> Box<Block> {
        let name = self.item.sig.ident.clone().to_string();
        let cost = self.meta.runtime_costs.clone();
        let err_len = self.meta.err_len.clone();
        let inner_block = self.item.block.clone();
        let mut inputs = self.item.sig.inputs.iter().cloned().collect::<Vec<_>>();
        let mut skip = 1;
        let mut output = parse_quote!(-> EmptyOutput);

        let run: Expr = match self.meta.call_type {
            CallType::InFallible => {
                parse_quote! {
                    ctx.run(#cost, |ctx| {
                        #inner_block.map_err(Into::into)
                    })
                }
            }
            CallType::Fallible => {
                inputs.push(parse_quote!(err_mid_ptr: u32));
                parse_quote! {
                    ctx.run_fallible::<_, _, #err_len>(err_mid_ptr, #cost, |ctx| {
                        #inner_block.map_err(Into::into)
                    })
                }
            }
            CallType::StateTaken => {
                skip = 2;
                output = self.item.sig.output.clone();
                parse_quote! {
                    ctx.run_state_taken(#cost, |ctx, state| {
                        #inner_block.map_err(Into::into)
                    })
                }
            }
        };

        let inner_args = inputs.clone().into_iter().skip(skip).collect::<Vec<_>>();
        let mut log_args: Vec<Expr> = vec![parse_quote!(#name)];
        log_args.extend(
            inputs
                .into_iter()
                .skip(skip)
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
            let func = move |
                caller: Caller<'_, HostState<E>>,
                #(#inner_args),*
            | #output {
                syscall_trace!(#(#log_args),*);

                let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

                #run
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

/// Call type of the host function.
#[derive(Default)]
pub enum CallType {
    #[default]
    InFallible,
    Fallible,
    StateTaken,
}

pub struct HostFnMeta {
    /// Call type of the host function.
    pub call_type: CallType,
    /// If the host function is wgas.
    pub wgas: bool,
    /// The runtime costs of the host function.
    pub runtime_costs: Expr,
    /// The length of the error.
    pub err_len: Expr,
}

impl HostFnMeta {
    /// If the host function is infallible.
    pub fn infallible(&self) -> bool {
        matches!(self.call_type, CallType::InFallible)
    }

    /// If the host function is fallible.
    pub fn fallible(&self) -> bool {
        matches!(self.call_type, CallType::Fallible)
    }

    /// If the host function requires state taken.
    pub fn state_taken(&self) -> bool {
        matches!(self.call_type, CallType::StateTaken)
    }
}

impl Parse for HostFnMeta {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut call_type = Default::default();
        let mut wgas = false;
        let mut runtime_costs = None;
        let mut err_len = None;

        let meta_list = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;
        for meta in meta_list {
            let ident = meta.path().get_ident().expect("Missing ident");
            match ident.to_string().as_ref() {
                "fallible" => call_type = CallType::Fallible,
                "state_taken" => call_type = CallType::StateTaken,
                "wgas" => wgas = true,
                "cost" => runtime_costs = Some(meta.require_name_value()?.value.clone()),
                "err_len" => err_len = Some(meta.require_name_value()?.value.clone()),
                _ => {}
            }
        }

        Ok(Self {
            call_type,
            wgas,
            runtime_costs: runtime_costs.expect("Missing runtime cost"),
            err_len: err_len.unwrap_or(parse_quote!(LengthWithHash)),
        })
    }
}
