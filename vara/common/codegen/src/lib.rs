// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![no_std]

#[macro_use]
extern crate quote;
extern crate proc_macro;
extern crate syn;

use proc_macro::TokenStream;
use syn::DeriveInput;

/// Derive macro for default implementation of RuntimeReason.
#[proc_macro_derive(RuntimeReason)]
pub fn derive_runtime_reason(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();
    let name = &ast.ident;

    quote! { impl RuntimeReason for #name {} }.into()
}

/// Derive macro for default implementation of SystemReason.
#[proc_macro_derive(SystemReason)]
pub fn derive_system_reason(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();
    let name = &ast.ident;

    quote! { impl SystemReason for #name {} }.into()
}
