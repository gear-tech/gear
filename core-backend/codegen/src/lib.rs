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

use host::{HostFn, HostFnMeta};
use proc_macro::TokenStream;
use syn::ItemFn;

mod host;

/// Apply host state wrapper to host functions.
///
/// Supported meta attributes:
/// * fallible             - if the host function executes fallible call.
/// * state_taken          - if the host function executes state taken call.
/// * fallible_state_taken - if the host function executes fallible state taken call.
/// * wgas                 - if the host function supports with-gas version.
/// * cost                 - RuntimeCosts definition, for example `#[host(cost = RuntimeCosts::Null)]`
/// * err_len              - Error length definition, for example `#[host(err_len = LengthBytes)]`
///
/// # Example
///
/// ```ignore
/// #[host(fallible, wgas, cost = RuntimeCosts::Reply(len))]
/// pub fn reply(
///     ctx: CallerWrap<E>,
///     payload_ptr: u32,
///     len: u32,
///     value_ptr: u32,
///     delay: u32,
/// ) -> Func {
///     let read_payload = ctx.register_read(payload_ptr, len);
///     let value = ctx.register_and_read_value(value_ptr)?;
///     let payload = ctx.read(read_payload)?.try_into()?;
///
///     let state = ctx.host_state_mut();
///     state.ext.reply(ReplyPacket::new(payload, value), delay)
/// }
/// ```
///
/// will generate
///
/// ```ignore
/// pub fn reply(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
///     let func = move |caller: Caller<'_, HostState<E>>,
///         payload_ptr: u32,
///         len: u32,
///         value_ptr: u32,
///         delay: u32,
///         err_mid_ptr: u32|
///     -> EmptyOutput {
///         syscall_trace!("reply", payload_ptr, len, value_ptr, delay, err_mid_ptr);
///
///         ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::Reply(len), |ctx| {
///             // ...
///         })
///     };
///
///     Func::wrap(store, func)
/// }
///
/// pub fn reply_wgas(
///     store: &mut Store<HostState<E>>,
///     forbidden: bool,
///     memory: WasmiMemory,
/// ) -> Func {
///     let func = move |caller: Caller<'_, HostState<E>>,
///         payload_ptr: u32,
///         len: u32,
///         gas_limit: u64,
///         value_ptr: u32,
///         delay: u32,
///         err_mid_ptr: u32|
///     -> EmptyOutput {
///         // ...
///
///         ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::ReplyWGas(len), |ctx| {
///                 // ...
///
///                 state.ext.reply(
///                     ReplyPacket::new_with_gas(payload, gas_limit, value), delay
///                 ).map_err(Into::into)
///             },
///         )
///     };
///
///     Func::wrap(store, func)
/// }
/// ```
#[proc_macro_attribute]
pub fn host(meta: TokenStream, item: TokenStream) -> TokenStream {
    let meta: HostFnMeta = syn::parse_macro_input!(meta);
    let item: ItemFn = syn::parse_macro_input!(item);

    HostFn::new(meta, item).into()
}
