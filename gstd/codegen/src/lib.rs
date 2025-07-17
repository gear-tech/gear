// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! Provides macros for async runtime of Gear programs.

use core::fmt::Display;
use gprimitives::ActorId;
use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{ToTokens, quote};
use std::{collections::BTreeSet, str::FromStr};
use syn::{
    Path, Token,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

mod utils;

/// A global flag, determining if `handle_reply` already was generated.
static mut HANDLE_REPLY_FLAG: Flag = Flag(false);

/// A global flag, determining if `handle_signal` already was generated.
#[cfg(not(feature = "ethexe"))]
static mut HANDLE_SIGNAL_FLAG: Flag = Flag(false);

#[cfg(feature = "ethexe")]
static mut HANDLE_SIGNAL_FLAG: Flag = Flag(true);

fn literal_to_actor_id(literal: syn::LitStr) -> syn::Result<TokenStream> {
    let actor_id: [u8; 32] = ActorId::from_str(&literal.value())
        .map_err(|err| syn::Error::new_spanned(literal, err))?
        .into();

    let actor_id_array = format!("{actor_id:?}")
        .parse::<proc_macro2::TokenStream>()
        .expect("failed to parse token stream");

    Ok(quote! { gstd::ActorId::new(#actor_id_array) }.into())
}

/// Macro to declare `ActorId` from hexadecimal and ss58 format.
///
/// # Example
/// ```
/// use gstd::{actor_id, ActorId};
///
/// # fn main() {
/// //polkadot address
/// let alice_1: ActorId = actor_id!("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY");
/// //vara address
/// let alice_2: ActorId = actor_id!("kGkLEU3e3XXkJp2WK4eNpVmSab5xUNL9QtmLPh8QfCL2EgotW");
/// //hex address
/// let alice_3: ActorId =
///     actor_id!("0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d");
///
/// assert_eq!(alice_1, alice_2);
/// assert_eq!(alice_2, alice_3);
/// # }
/// ```
#[proc_macro]
pub fn actor_id(input: TokenStream) -> TokenStream {
    literal_to_actor_id(syn::parse_macro_input!(input as syn::LitStr))
        .unwrap_or_else(|err| err.to_compile_error().into())
}

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
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
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
                #[cfg(not(feature = "ethexe"))]
                "handle_signal" => {
                    attrs.handle_signal = Some(path);
                }
                #[cfg(feature = "ethexe")]
                "handle_signal" => {
                    return Err(syn::Error::new_spanned(
                        name,
                        "`handle_signal` is forbidden with `ethexe` feature on",
                    ));
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
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
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
    #[allow(clippy::deref_addrof)] // https://github.com/rust-lang/rust-clippy/issues/13783
    let reply_generated = unsafe { (*&raw mut HANDLE_REPLY_FLAG).get_and_set() };
    if !reply_generated {
        let handle_reply: TokenStream = quote!(
            #[unsafe(no_mangle)]
            extern "C" fn handle_reply() {
                gstd::handle_reply_with_hook();
                #attr ();
            }
        )
        .into();
        code.extend([handle_reply]);
    }

    code
}

fn generate_handle_signal_if_required(mut code: TokenStream, attr: Option<Path>) -> TokenStream {
    #[allow(clippy::deref_addrof)] // https://github.com/rust-lang/rust-clippy/issues/13783
    let signal_generated = unsafe { (*&raw mut HANDLE_SIGNAL_FLAG).get_and_set() };
    if !signal_generated {
        let handle_signal: TokenStream = quote!(
            #[unsafe(no_mangle)]
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
/// If you need to specify the `handle` function explicitly, don't use this
/// macro.
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
/// # fn main() {}
/// ```
///
/// Use `handle_reply` and `handle_signal` parameters to specify corresponding
/// handlers. Note that custom reply and signal handlers derive their default
/// behavior.
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

        #[unsafe(no_mangle)]
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
/// Use `handle_reply` and `handle_signal` parameters to specify corresponding
/// handlers. Note that custom reply and signal handlers derive their default
/// behavior.
///
/// ```ignore
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
        #[unsafe(no_mangle)]
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
/// use gcore::errors::Result;
///
/// #[wait_for_reply]
/// pub fn send_bytes<T: AsRef<[u8]>>(program: ActorId, payload: T, value: u128) -> Result<MessageId> {
///   gcore::msg::send(program.into(), payload.as_ref(), value)
/// }
/// ```
///
/// outputs:
///
/// ```ignore
/// /// Same as [`send_bytes`](self::send_bytes), but the program
/// /// will interrupt until the reply is received.
/// ///
/// /// Argument `reply_deposit: u64` used to provide gas for
/// /// future reply handling (skipped if zero).
/// ///
/// /// # See also
/// ///
/// /// - [`send_bytes_for_reply_as`](self::send_bytes_for_reply_as)
/// pub fn send_bytes_for_reply<T: AsRef<[u8]>>(
///     program: ActorId,
///     payload: T,
///     value: u128,
///     reply_deposit: u64
/// ) -> Result<crate::msg::MessageFuture> {
///     // Function call.
///     let waiting_reply_to = send_bytes(program, payload, value)?;
///
///     // Depositing gas for future reply handling if not zero.
///     if reply_deposit != 0 {
///         crate::exec::reply_deposit(waiting_reply_to, reply_deposit)?;
///     }
///
///     // Registering signal.
///     crate::async_runtime::signals().register_signal(waiting_reply_to);
///
///     Ok(crate::msg::MessageFuture { waiting_reply_to })
/// }
///
/// /// Same as [`send_bytes`](self::send_bytes), but the program
/// /// will interrupt until the reply is received.
/// ///
/// /// Argument `reply_deposit: u64` used to provide gas for
/// /// future reply handling (skipped if zero).
/// ///
/// /// The output should be decodable via SCALE codec.
/// ///
/// /// # See also
/// ///
/// /// - [`send_bytes_for_reply`](self::send_bytes_for_reply)
/// /// - <https://docs.substrate.io/reference/scale-codec>
/// pub fn send_bytes_for_reply_as<T: AsRef<[u8]>, D: crate::codec::Decode>(
///     program: ActorId,
///     payload: T,
///     value: u128,
///     reply_deposit: u64,
/// ) -> Result<crate::msg::CodecMessageFuture<D>> {
///     // Function call.
///     let waiting_reply_to = send_bytes(program, payload, value)?;
///
///     // Depositing gas for future reply handling if not zero.
///     if reply_deposit != 0 {
///         crate::exec::reply_deposit(waiting_reply_to, reply_deposit)?;
///     }
///
///     // Registering signal.
///     crate::async_runtime::signals().register_signal(waiting_reply_to);
///
///     Ok(crate::msg::CodecMessageFuture::<D> {
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
    let style = if !attr.is_empty() {
        utils::DocumentationStyle::Method
    } else {
        utils::DocumentationStyle::Function
    };

    let (for_reply_docs, for_reply_as_docs) = utils::wait_for_reply_docs(ident.to_string(), style);

    // Generate arguments.
    #[cfg_attr(feature = "ethexe", allow(unused_mut))]
    let (mut inputs, variadic) = (function.sig.inputs.clone(), function.sig.variadic.clone());
    let args = utils::get_args(&inputs);

    // Add `reply_deposit` argument.
    #[cfg(not(feature = "ethexe"))]
    inputs.push(syn::parse_quote!(reply_deposit: u64));

    // Generate generics.
    let decodable_ty = utils::ident("D");
    let decodable_traits = vec![syn::parse_quote!(crate::codec::Decode)];
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

    match () {
        #[cfg(not(feature = "ethexe"))]
        () => quote! {
            #function

            #[doc = #for_reply_docs]
            pub fn #for_reply #for_reply_generics ( #inputs #variadic ) -> Result<crate::msg::MessageFuture> {
                // Function call.
                let waiting_reply_to = #ident #args ?;

                // Depositing gas for future reply handling if not zero.
                if reply_deposit != 0 {
                    crate::exec::reply_deposit(waiting_reply_to, reply_deposit)?;
                }

                // Registering signal.
                crate::async_runtime::signals().register_signal(waiting_reply_to);

            Ok(crate::msg::MessageFuture { waiting_reply_to, reply_deposit })
        }

            #[doc = #for_reply_as_docs]
            pub fn #for_reply_as #for_reply_as_generics ( #inputs #variadic ) -> Result<crate::msg::CodecMessageFuture<D>> {
                // Function call.
                let waiting_reply_to = #ident #args ?;

                // Depositing gas for future reply handling if not zero.
                if reply_deposit != 0 {
                    crate::exec::reply_deposit(waiting_reply_to, reply_deposit)?;
                }

                // Registering signal.
                crate::async_runtime::signals().register_signal(waiting_reply_to);

                Ok(crate::msg::CodecMessageFuture::<D> { waiting_reply_to, reply_deposit, _marker: Default::default() })
            }
        },
        #[cfg(feature = "ethexe")]
        () => quote! {
            #function

            #[doc = #for_reply_docs]
            pub fn #for_reply #for_reply_generics ( #inputs #variadic ) -> Result<crate::msg::MessageFuture> {
                // Function call.
                let waiting_reply_to = #ident #args ?;

                // Registering signal.
                crate::async_runtime::signals().register_signal(waiting_reply_to);

                Ok(crate::msg::MessageFuture { waiting_reply_to, reply_deposit: 0 })
            }

            #[doc = #for_reply_as_docs]
            pub fn #for_reply_as #for_reply_as_generics ( #inputs #variadic ) -> Result<crate::msg::CodecMessageFuture<D>> {
                // Function call.
                let waiting_reply_to = #ident #args ?;

                // Registering signal.
                crate::async_runtime::signals().register_signal(waiting_reply_to);

                Ok(crate::msg::CodecMessageFuture::<D> { waiting_reply_to, reply_deposit: 0, _marker: Default::default() })
            }
        },
    }.into()
}

/// Similar to [`macro@wait_for_reply`], but works with functions that create
/// programs: It returns a message id with a newly created program id.
#[proc_macro_attribute]
pub fn wait_create_program_for_reply(attr: TokenStream, item: TokenStream) -> TokenStream {
    let function = syn::parse_macro_input!(item as syn::ItemFn);

    let function_ident = &function.sig.ident;

    let ident = if !attr.is_empty() {
        assert_eq!(
            attr.to_string(),
            "Self",
            "Proc macro attribute should be used only to specify Self source of the function"
        );

        quote! { Self::#function_ident }
    } else {
        quote! { #function_ident }
    };

    // Generate functions' idents.
    let (for_reply, for_reply_as) = (
        utils::with_suffix(&function.sig.ident, "_for_reply"),
        utils::with_suffix(&function.sig.ident, "_for_reply_as"),
    );

    // Generate docs.
    let style = if !attr.is_empty() {
        utils::DocumentationStyle::Method
    } else {
        utils::DocumentationStyle::Function
    };

    let (for_reply_docs, for_reply_as_docs) =
        utils::wait_for_reply_docs(function_ident.to_string(), style);

    // Generate arguments.
    #[cfg_attr(feature = "ethexe", allow(unused_mut))]
    let (mut inputs, variadic) = (function.sig.inputs.clone(), function.sig.variadic.clone());
    let args = utils::get_args(&inputs);

    // Add `reply_deposit` argument.
    #[cfg(not(feature = "ethexe"))]
    inputs.push(syn::parse_quote!(reply_deposit: u64));

    // Generate generics.
    let decodable_ty = utils::ident("D");
    let decodable_traits = vec![syn::parse_quote!(crate::codec::Decode)];
    let (for_reply_generics, for_reply_as_generics) = (
        function.sig.generics.clone(),
        utils::append_generic(
            function.sig.generics.clone(),
            decodable_ty,
            decodable_traits,
        ),
    );

    match () {
        #[cfg(not(feature = "ethexe"))]
        () => quote! {
            #function

            #[doc = #for_reply_docs]
            pub fn #for_reply #for_reply_generics ( #inputs #variadic ) -> Result<crate::msg::CreateProgramFuture> {
                // Function call.
                let (waiting_reply_to, program_id) = #ident #args ?;

                // Depositing gas for future reply handling if not zero.
                if reply_deposit != 0 {
                    crate::exec::reply_deposit(waiting_reply_to, reply_deposit)?;
                }

                // Registering signal.
                crate::async_runtime::signals().register_signal(waiting_reply_to);

            Ok(crate::msg::CreateProgramFuture { waiting_reply_to, program_id, reply_deposit })
        }

            #[doc = #for_reply_as_docs]
            pub fn #for_reply_as #for_reply_as_generics ( #inputs #variadic ) -> Result<crate::msg::CodecCreateProgramFuture<D>> {
                // Function call.
                let (waiting_reply_to, program_id) = #ident #args ?;

                // Depositing gas for future reply handling if not zero.
                if reply_deposit != 0 {
                    crate::exec::reply_deposit(waiting_reply_to, reply_deposit)?;
                }

                // Registering signal.
                crate::async_runtime::signals().register_signal(waiting_reply_to);

                Ok(crate::msg::CodecCreateProgramFuture::<D> { waiting_reply_to, program_id, reply_deposit, _marker: Default::default() })
            }
        },
        #[cfg(feature = "ethexe")]
        () => quote! {
            #function

            #[doc = #for_reply_docs]
            pub fn #for_reply #for_reply_generics ( #inputs #variadic ) -> Result<crate::msg::CreateProgramFuture> {
                // Function call.
                let (waiting_reply_to, program_id) = #ident #args ?;

                // Registering signal.
                crate::async_runtime::signals().register_signal(waiting_reply_to);

                Ok(crate::msg::CreateProgramFuture { waiting_reply_to, program_id, reply_deposit: 0 })
            }

            #[doc = #for_reply_as_docs]
            pub fn #for_reply_as #for_reply_as_generics ( #inputs #variadic ) -> Result<crate::msg::CodecCreateProgramFuture<D>> {
                // Function call.
                let (waiting_reply_to, program_id) = #ident #args ?;

                // Registering signal.
                crate::async_runtime::signals().register_signal(waiting_reply_to);

                Ok(crate::msg::CodecCreateProgramFuture::<D> { waiting_reply_to, program_id, reply_deposit: 0, _marker: Default::default() })
            }
        },
    }.into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn ui() {
        let t = trybuild::TestCases::new();

        #[cfg(not(feature = "ethexe"))]
        {
            t.pass("tests/ui/async_init_works.rs");
            t.pass("tests/ui/async_main_works.rs");
            t.compile_fail("tests/ui/signal_double_definition_not_work.rs");
            t.compile_fail("tests/ui/reply_double_definition_not_work.rs");
        }

        #[cfg(feature = "ethexe")]
        {
            t.compile_fail("tests/ui/signal_doesnt_work_with_ethexe.rs");
        }
    }
}
