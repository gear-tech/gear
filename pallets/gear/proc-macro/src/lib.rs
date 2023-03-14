// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! Proc macros used in the gear module.

#![no_std]

extern crate alloc;

use alloc::string::ToString;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::{parse_macro_input, spanned::Spanned, Data, DeriveInput, Ident};
/// This derives `Debug` for a struct where each field must be of some numeric type.
/// It interprets each field as its represents some weight and formats it as times so that
/// it is readable by humans.
#[proc_macro_derive(WeightDebug)]
pub fn derive_weight_debug(input: TokenStream) -> TokenStream {
    derive_debug(input, format_weight)
}

/// This is basically identical to the std libs Debug derive but without adding any
/// bounds to existing generics.
#[proc_macro_derive(ScheduleDebug)]
pub fn derive_schedule_debug(input: TokenStream) -> TokenStream {
    derive_debug(input, format_default)
}

fn derive_debug(input: TokenStream, fmt: impl Fn(&Ident) -> TokenStream2) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let data = if let Data::Struct(data) = &input.data {
        data
    } else {
        return quote_spanned! {
            name.span() =>
            compile_error!("WeightDebug is only supported for structs.");
        }
        .into();
    };

    #[cfg(feature = "full")]
    let fields = iterate_fields(data, fmt);

    #[cfg(not(feature = "full"))]
    let fields = {
        drop(fmt);
        drop(data);
        TokenStream2::new()
    };

    let tokens = quote! {
        impl #impl_generics core::fmt::Debug for #name #ty_generics #where_clause {
            fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                use ::sp_runtime::{FixedPointNumber, FixedU128 as Fixed};
                let mut formatter = formatter.debug_struct(stringify!(#name));
                #fields
                formatter.finish()
            }
        }
    };

    tokens.into()
}

/// This is only used when the `full` feature is activated.
#[cfg(feature = "full")]
fn iterate_fields(data: &syn::DataStruct, fmt: impl Fn(&Ident) -> TokenStream2) -> TokenStream2 {
    use syn::Fields;

    match &data.fields {
        Fields::Named(fields) => {
            let recurse = fields.named.iter().filter_map(|f| {
                let name = f.ident.as_ref()?;
                if name.to_string().starts_with('_') {
                    return None;
                }
                let value = fmt(name);
                let ret = quote_spanned! { f.span() =>
                    formatter.field(stringify!(#name), #value);
                };
                Some(ret)
            });
            quote! {
                #( #recurse )*
            }
        }
        Fields::Unnamed(fields) => quote_spanned! {
            fields.span() =>
            compile_error!("Unnamed fields are not supported")
        },
        Fields::Unit => quote!(),
    }
}

fn format_weight(field: &Ident) -> TokenStream2 {
    quote_spanned! { field.span() =>
        &if self.#field.ref_time() > 1_000_000_000 {
            format!(
                "{:.1?} ms, {} bytes",
                Fixed::saturating_from_rational(self.#field.ref_time(), 1_000_000_000).to_float(),
                self.#field.proof_size()
            )
        } else if self.#field.ref_time() > 1_000_000 {
            format!(
                "{:.1?} µs, {} bytes",
                Fixed::saturating_from_rational(self.#field.ref_time(), 1_000_000).to_float(),
                self.#field.proof_size()
            )
        } else if self.#field.ref_time() > 1_000 {
            format!(
                "{:.1?} ns, {} bytes",
                Fixed::saturating_from_rational(self.#field.ref_time(), 1_000).to_float(),
                self.#field.proof_size()
            )
        } else {
            format!("{} ps, {} bytes", self.#field.ref_time(), self.#field.proof_size())
        }
    }
}

fn format_default(field: &Ident) -> TokenStream2 {
    quote_spanned! { field.span() =>
        &self.#field
    }
}
