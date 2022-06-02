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

#![no_std]

#[macro_use]
extern crate quote;
extern crate alloc;
extern crate proc_macro;
extern crate syn;

use alloc::string::ToString;
use proc_macro::TokenStream;

/// Derive macro for default implementation of RuntimeReason.
#[proc_macro_derive(RuntimeReason)]
pub fn derive_runtime_reason(input: TokenStream) -> TokenStream {
    let s = input.to_string();
    let ast = syn::parse_derive_input(&s).unwrap();
    let gen = impl_runtime_reason(&ast);
    gen.parse().unwrap()
}

fn impl_runtime_reason(ast: &syn::DeriveInput) -> quote::Tokens {
    let name = &ast.ident;
    quote! { impl RuntimeReason for #name {} }
}

/// Derive macro for default implementation of SystemReason.
#[proc_macro_derive(SystemReason)]
pub fn derive_system_reason(input: TokenStream) -> TokenStream {
    let s = input.to_string();
    let ast = syn::parse_derive_input(&s).unwrap();
    let gen = impl_system_reason(&ast);
    gen.parse().unwrap()
}

fn impl_system_reason(ast: &syn::DeriveInput) -> quote::Tokens {
    let name = &ast.ident;
    quote! { impl SystemReason for #name {} }
}
