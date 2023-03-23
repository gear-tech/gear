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

//! Provides macros for async runtime of Gear contracts.

extern crate proc_macro;

use core::fmt::Display;
use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{quote, ToTokens};
use std::collections::BTreeSet;
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    Path, Token,
};

mod utils;

/// A global flag, determining if `handle_reply` already was generated.
static mut HANDLE_REPLY_FLAG: Flag = Flag(false);

/// A global flag, determining if `handle_signal` already was generated.
static mut HANDLE_SIGNAL_FLAG: Flag = Flag(false);

struct Flag(bool);

impl Flag {
    fn get_and_set(&mut self) -> bool {
        let ret = self.0;
        self.0 = true;
        ret
    }
}

struct MainAttrs {
    handle_reply: Option<Path>,
    handle_signal: Option<Path>,
}

impl MainAttrs {
    fn check_attrs_not_exist(&self) -> Result<(), TokenStream> {
        let Self {
            handle_reply,
            handle_signal,
        } = self;

        for (path, flag) in unsafe {
            [
                (handle_reply, HANDLE_REPLY_FLAG.0),
                (handle_signal, HANDLE_SIGNAL_FLAG.0),
            ]
        } {
            if let (Some(path), true) = (path, flag) {
                return Err(compile_error(path, "parameter already defined"));
            }
        }

        Ok(())
    }
}

impl Parse for MainAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let punctuated: Punctuated<MainAttr, Token![,]> = Punctuated::parse_terminated(input)?;
        let mut attrs = MainAttrs {
            handle_reply: None,
            handle_signal: None,
        };
        let mut existing_attrs = BTreeSet::new();

        for MainAttr { name, path, .. } in punctuated {
            let name = name.to_string();
            if existing_attrs.contains(&name) {
                return Err(syn::Error::new_spanned(name, "parameter already defined"));
            }

            match &*name {
                "handle_reply" => {
                    attrs.handle_reply = Some(path);
                }
                "handle_signal" => {
                    attrs.handle_signal = Some(path);
                }
                _ => return Err(syn::Error::new_spanned(name, "unknown parameter")),
            }

            existing_attrs.insert(name);
        }

        Ok(attrs)
    }
}

struct MainAttr {
    name: Ident,
    path: Path,
}

impl Parse for MainAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        let _: Token![=] = input.parse()?;
        let path: Path = input.parse()?;

        Ok(Self { name, path })
    }
}

fn compile_error<T: ToTokens, U: Display>(tokens: T, msg: U) -> TokenStream {
    syn::Error::new_spanned(tokens, msg)
        .to_compile_error()
        .into()
}

fn check_signature(name: &str, function: &syn::ItemFn) -> Result<(), TokenStream> {
    if function.sig.ident != name {
        return Err(compile_error(
            &function.sig.ident,
            format!("function must be called `{name}`"),
        ));
    }

    if !function.sig.inputs.is_empty() {
        return Err(compile_error(
            &function.sig.ident,
            "function must have no arguments",
        ));
    }

    if function.sig.asyncness.is_none() {
        return Err(compile_error(
            function.sig.fn_token,
            "function must be async",
        ));
    }

    Ok(())
}

fn generate_handle_reply_if_required(mut code: TokenStream, attr: Option<Path>) -> TokenStream {
    let reply_generated = unsafe { HANDLE_REPLY_FLAG.get_and_set() };
    if !reply_generated {
        let handle_reply: TokenStream = quote!(
            #[no_mangle]
            extern "C" fn handle_reply() {
                gstd::record_reply();
                #attr ();
            }
        )
        .into();
        code.extend([handle_reply]);
    }

    code
}

fn generate_handle_signal_if_required(mut code: TokenStream, attr: Option<Path>) -> TokenStream {
    let signal_generated = unsafe { HANDLE_SIGNAL_FLAG.get_and_set() };
    if !signal_generated {
        let handle_signal: TokenStream = quote!(
            #[no_mangle]
            extern "C" fn handle_signal() {
                gstd::handle_signal();
                #attr ();
            }
        )
        .into();
        code.extend([handle_signal]);
    }

    code
}

fn generate_if_required(code: TokenStream, attrs: MainAttrs) -> TokenStream {
    let code = generate_handle_reply_if_required(code, attrs.handle_reply);
    generate_handle_signal_if_required(code, attrs.handle_signal)
}

/// Mark the main async function to be the program entry point.
///
/// Can be used together with [`macro@async_init`].
///
/// When this macro is used, it’s not possible to specify the `handle` function.
/// If you need to specify the `handle` function explicitly, don't use this macro.
///
/// # Examples
///
/// Simple async handle function:
///
/// ```
/// #[gstd::async_main]
/// async fn main() {
///     gstd::debug!("Hello world!");
/// }
///
/// # fn main() {}
/// ```
///
/// Use `handle_reply` and `handle_signal` parameters to specify corresponding handlers.
/// Note that custom reply and signal handlers derive their default behavior.
///
/// ```
/// #[gstd::async_main(handle_reply = my_handle_reply)]
/// async fn main() {
///     // ...
/// }
///
/// fn my_handle_reply() {
///     // ...
/// }
///
/// # fn main() {}
/// ```
#[proc_macro_attribute]
pub fn async_main(attr: TokenStream, item: TokenStream) -> TokenStream {
    let function = syn::parse_macro_input!(item as syn::ItemFn);
    if let Err(tokens) = check_signature("main", &function) {
        return tokens;
    }

    let attrs = syn::parse_macro_input!(attr as MainAttrs);
    if let Err(tokens) = attrs.check_attrs_not_exist() {
        return tokens;
    }

    let body = &function.block;
    let code: TokenStream = quote!(

        fn __main_safe() {
            gstd::message_loop(async #body);
        }

        #[no_mangle]
        extern "C" fn handle() {
            __main_safe();
        }
    )
    .into();

    generate_if_required(code, attrs)
}

/// Mark async function to be the program initialization method.
///
/// Can be used together with [`macro@async_main`].
///
/// The `init` function cannot be specified if this macro is used.
/// If you need to specify the `init` function explicitly, don't use this macro.
///
///
/// # Examples
///
/// Simple async init function:
///
/// ```
/// #[gstd::async_init]
/// async fn init() {
///     gstd::debug!("Hello world!");
/// }
/// ```
///
/// Use `handle_reply` and `handle_signal` parameters to specify corresponding handlers.
/// Note that custom reply and signal handlers derive their default behavior.
///
/// ```
/// #[gstd::async_init(handle_signal = my_handle_signal)]
/// async fn init() {
///     // ...
/// }
///
/// fn my_handle_signal() {
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn async_init(attr: TokenStream, item: TokenStream) -> TokenStream {
    let function = syn::parse_macro_input!(item as syn::ItemFn);
    if let Err(tokens) = check_signature("init", &function) {
        return tokens;
    }

    let attrs = syn::parse_macro_input!(attr as MainAttrs);
    if let Err(tokens) = attrs.check_attrs_not_exist() {
        return tokens;
    }

    let body = &function.block;
    let code: TokenStream = quote!(
        #[no_mangle]
        extern "C" fn init() {
            gstd::message_loop(async #body);
        }
    )
    .into();

    generate_if_required(code, attrs)
}

/// Extends async methods `for_reply` and `for_reply_as` for sending
/// methods.
///
/// # Usage
///
/// ```ignore
/// #[wait_for_reply]
/// pub fn send_bytes<T: AsRef<[u8]>>(program: ActorId, payload: T, value: u128) -> Result<MessageId> {
///   gcore::msg::send(program.into(), payload.as_ref(), value).into_contract_result()
/// }
/// ```
///
/// outputs:
///
/// ```ignore
/// /// Same as [`send_bytes`](crate::msg::send_bytes), but the program
/// /// will interrupt until the reply is received.
/// ///
/// /// # See also
/// ///
/// /// - [`send_bytes_for_reply_as`](crate::msg::send_bytes_for_reply_as)
/// pub fn send_bytes_for_reply<T: AsRef<[u8]>>(
///     program: ActorId,
///     payload: T,
///     value: u128,
/// ) -> Result<MessageFuture> {
///     let waiting_reply_to = send_bytes(program, payload, value)?;
///     signals().register_signal(waiting_reply_to);
///
///     Ok(MessageFuture { waiting_reply_to })
/// }
///
/// /// Same as [`send_bytes`](crate::msg::send_bytes), but the program
/// /// will interrupt until the reply is received.
/// ///
/// /// The output should be decodable via SCALE codec.
/// ///
/// /// # See also
/// ///
/// /// - [`send_bytes_for_reply`](crate::msg::send_bytes_for_reply)
/// /// - <https://docs.substrate.io/v3/advanced/scale-codec>
/// pub fn send_bytes_for_reply_as<T: AsRef<[u8]>, D: Decode>(
///     program: ActorId,
///     payload: T,
///     value: u128,
/// ) -> Result<CodecMessageFuture<D>> {
///     let waiting_reply_to = send_bytes(program, payload, value)?;
///     signals().register_signal(waiting_reply_to);
///
///     Ok(CodecMessageFuture::<D> {
///         waiting_reply_to,
///         _marker: Default::default(),
///     })
/// }
/// ```
#[proc_macro_attribute]
pub fn wait_for_reply(attr: TokenStream, item: TokenStream) -> TokenStream {
    let function = syn::parse_macro_input!(item as syn::ItemFn);
    let ident = &function.sig.ident;

    // Generate functions' idents.
    let (for_reply, for_reply_as) = (
        utils::with_suffix(ident, "_for_reply"),
        utils::with_suffix(ident, "_for_reply_as"),
    );

    // Generate docs.
    let (for_reply_docs, for_reply_as_docs) = utils::wait_for_reply_docs(ident.to_string());

    // Generate arguments.
    let (inputs, variadic) = (function.sig.inputs.clone(), function.sig.variadic.clone());
    let args = utils::get_args(&inputs);

    // Generate generics.
    let decodable_ty = utils::ident("D");
    let decodable_traits = vec![utils::ident("Decode")];
    let (for_reply_generics, for_reply_as_generics) = (
        function.sig.generics.clone(),
        utils::append_generic(
            function.sig.generics.clone(),
            decodable_ty,
            decodable_traits,
        ),
    );

    let ident = if !attr.is_empty() {
        assert_eq!(
            attr.to_string(),
            "self",
            "Proc macro attribute should be used only to specify self source of the function"
        );

        quote! { self.#ident }
    } else {
        quote! { #ident }
    };

    quote! {
        #function

        #[doc = #for_reply_docs]
        pub fn #for_reply #for_reply_generics ( #inputs #variadic ) -> Result<MessageFuture> {
            let waiting_reply_to = #ident #args ?;
            signals().register_signal(waiting_reply_to);

            Ok(MessageFuture { waiting_reply_to })
        }

        #[doc = #for_reply_as_docs]
        pub fn #for_reply_as #for_reply_as_generics ( #inputs #variadic ) -> Result<CodecMessageFuture<D>> {
            let waiting_reply_to = #ident #args ?;
            signals().register_signal(waiting_reply_to);

            Ok(CodecMessageFuture::<D> { waiting_reply_to, _marker: Default::default() })
        }
    }
    .into()
}

/// Similar to `wait_for_reply`, but works with functions that create programs:
/// It returns a message id with a newly created program id.
#[proc_macro_attribute]
pub fn wait_create_program_for_reply(attr: TokenStream, item: TokenStream) -> TokenStream {
    let function = syn::parse_macro_input!(item as syn::ItemFn);

    let ident = &function.sig.ident;

    let ident = if !attr.is_empty() {
        assert_eq!(
            attr.to_string(),
            "Self",
            "Proc macro attribute should be used only to specify Self source of the function"
        );

        quote! { Self::#ident }
    } else {
        quote! { #ident }
    };

    // Generate functions' idents.
    let (for_reply, for_reply_as) = (
        utils::with_suffix(&function.sig.ident, "_for_reply"),
        utils::with_suffix(&function.sig.ident, "_for_reply_as"),
    );

    // Generate docs.
    let (for_reply_docs, for_reply_as_docs) = utils::wait_for_reply_docs(ident.to_string());

    // Generate arguments.
    let (inputs, variadic) = (function.sig.inputs.clone(), function.sig.variadic.clone());
    let args = utils::get_args(&inputs);

    // Generate generics.
    let decodable_ty = utils::ident("D");
    let decodable_traits = vec![utils::ident("Decode")];
    let (for_reply_generics, for_reply_as_generics) = (
        function.sig.generics.clone(),
        utils::append_generic(
            function.sig.generics.clone(),
            decodable_ty,
            decodable_traits,
        ),
    );

    quote! {
        #function

        #[doc = #for_reply_docs]
        pub fn #for_reply #for_reply_generics ( #inputs #variadic ) -> Result<CreateProgramFuture> {
            let (waiting_reply_to, program_id) = #ident #args ?;
            signals().register_signal(waiting_reply_to);

            Ok(CreateProgramFuture { waiting_reply_to, program_id })
        }

        #[doc = #for_reply_as_docs]
        pub fn #for_reply_as #for_reply_as_generics ( #inputs #variadic ) -> Result<CodecCreateProgramFuture<D>> {
            let (waiting_reply_to, program_id) = #ident #args ?;
            signals().register_signal(waiting_reply_to);

            Ok(CodecCreateProgramFuture::<D> { waiting_reply_to, program_id, _marker: Default::default() })
        }
    }
    .into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn ui() {
        let t = trybuild::TestCases::new();
        t.pass("tests/ui/async_init_works.rs");
        t.pass("tests/ui/async_main_works.rs");
        t.compile_fail("tests/ui/signal_double_definition_not_work.rs");
        t.compile_fail("tests/ui/reply_double_definition_not_work.rs");
    }
}
