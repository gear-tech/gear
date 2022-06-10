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
use quote::{quote, ToTokens};

mod utils;

/// A global flag, determining if `handle_reply` already was generated.
static mut HANDLE_REPLY_FLAG: Flag = Flag(false);

struct Flag(bool);

impl Flag {
    fn get_and_set(&mut self) -> bool {
        let ret = self.0;
        self.0 = true;
        ret
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
            format!("function must be called `{}`", name),
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
            &function.sig.fn_token,
            "function must be async",
        ));
    }

    Ok(())
}

fn generate_handle_reply_if_required(mut code: TokenStream) -> TokenStream {
    let reply_generated = unsafe { HANDLE_REPLY_FLAG.get_and_set() };
    if !reply_generated {
        let handle_reply: TokenStream = quote!(
            #[no_mangle]
            pub unsafe extern "C" fn handle_reply() {
                gstd::record_reply();
            }
        )
        .into();
        code.extend([handle_reply]);
    }

    code
}

/// This is the procedural macro for your convenience.
/// It marks the main async function to be the program entry point.
/// Functions `handle`, `handle_reply` cannot be specified if this macro is used.
/// If you need to specify `handle`, `handle_reply` explicitly don't use this macro.
///
/// ## Usage
///
/// ```ignore
/// #[gstd::async_main]
/// async fn main() {
///     gstd::debug!("Hello world!");
/// }
/// ```
#[proc_macro_attribute]
pub fn async_main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let function = syn::parse_macro_input!(item as syn::ItemFn);
    if let Err(tokens) = check_signature("main", &function) {
        return tokens;
    }

    let body = &function.block;
    let code: TokenStream = quote!(

        fn __main_safe() {
            gstd::message_loop(async #body);
        }

        #[no_mangle]
        pub unsafe extern "C" fn handle() {
            __main_safe();
        }
    )
    .into();

    generate_handle_reply_if_required(code)
}

/// Mark async function to be the program initialization method.
/// Can be used together with [`async_main`].
/// Functions `init`, `handle_reply` cannot be specified if this macro is used.
/// If you need to specify `init`, `handle_reply` explicitly don't use this macro.
///
/// ## Usage
///
/// ```ignore
/// #[gstd::async_init]
/// async fn init() {
///     gstd::debug!("Hello world!");
/// }
/// ```
#[proc_macro_attribute]
pub fn async_init(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let function = syn::parse_macro_input!(item as syn::ItemFn);
    if let Err(tokens) = check_signature("init", &function) {
        return tokens;
    }

    let body = &function.block;
    let code: TokenStream = quote!(
        #[no_mangle]
        pub unsafe extern "C" fn init() {
            gstd::message_loop(async #body);
        }
    )
    .into();

    generate_handle_reply_if_required(code)
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
/// ```
/// /// Same as [`reply_bytes`](crate::msg::basic::reply_bytes), but the program
/// /// will interrupt until the reply is received.
/// ///
/// /// # See also
/// ///
/// /// - [`reply_bytes_for_reply_as`](crate::msg::basic::reply_bytes_for_reply_as)
/// pub async fn reply_bytes_for_reply(
///     payload: impl AsRef<[u8]>,
///     value: u128,
/// ) -> Result<Vec<u8>> {
///     let waiting_reply_to = reply_bytes(payload, value)?;
///     signals().register_signal(waiting_reply_to);
///     MessageFuture { waiting_reply_to }.await
/// }
///
/// /// Same as [`reply_bytes`](crate::msg::basic::reply_bytes), but the program
/// /// will interrupt until the reply is received.
/// ///
/// /// The output should be decodable via [`SCALE CODEC`].
/// ///
/// /// # See also
/// ///
/// /// - [`reply_bytes_for_reply`](crate::msg::basic::reply_bytes_for_reply)
/// /// - https://docs.substrate.io/v3/advanced/scale-codec
/// pub async fn reply_bytes_for_reply_as<D: Decode>(
///     payload: impl AsRef<[u8]>,
///     value: u128,
/// ) -> Result<D> {
///     D::decode(&mut reply_bytes_for_reply(payload, value).await?.as_ref())
///         .map_err(ContractError::Decode)
/// }
/// ```
#[proc_macro_attribute]
pub fn wait_for_reply(_: TokenStream, item: TokenStream) -> TokenStream {
    let function = syn::parse_macro_input!(item as syn::ItemFn);
    let ident = function.sig.ident.clone();

    // generate functions' idents
    let (for_reply, for_reply_as) = (
        utils::with_suffix(&function.sig.ident, "_for_reply"),
        utils::with_suffix(&function.sig.ident, "_for_reply_as"),
    );

    // generate docs
    let (for_reply_docs, for_reply_as_docs) = utils::wait_for_reply_docs(ident.to_string());

    // generate arguments
    let (inputs, variadic) = (function.sig.inputs.clone(), function.sig.variadic.clone());
    let args = utils::get_args(&inputs);

    // generate generics
    let decodeable_ty = utils::ident("D");
    let decodeable_traits = vec![utils::ident("Decode")];
    let (for_reply_generics, for_reply_as_generics) = (
        function.sig.generics.clone(),
        utils::append_generic(
            function.sig.generics.clone(),
            decodeable_ty,
            decodeable_traits,
        ),
    );

    quote! {
        #function

        #[doc = #for_reply_docs]
        pub async fn #for_reply #for_reply_generics ( #inputs #variadic ) -> Result<Vec<u8>> {
            let waiting_reply_to = #ident #args ?;
            signals().register_signal(waiting_reply_to);

            MessageFuture { waiting_reply_to }.await
        }

        #[doc = #for_reply_as_docs]
        pub async fn #for_reply_as #for_reply_as_generics ( #inputs #variadic ) -> Result<D> {
            D::decode(&mut #for_reply #args .await?.as_ref() ).map_err(ContractError::Decode)
        }
    }
    .into()
}
