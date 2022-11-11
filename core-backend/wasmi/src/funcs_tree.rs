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

use crate::state::HostState;
use alloc::collections::{BTreeMap, BTreeSet};
use codec::Encode;
use gear_backend_common::{error_processor::IntoExtError, AsTerminationReason, IntoExtInfo};
use gear_core::env::Ext;
use gear_wasm_instrument::{IMPORT_NAME_OUT_OF_ALLOWANCE, IMPORT_NAME_OUT_OF_GAS};
use wasmi::{Func, Memory, Store};

struct FunctionBuilder<'a>(Option<&'a BTreeSet<&'a str>>);

impl<'a> FunctionBuilder<'a> {
    fn build<'b, Handler>(&self, name: &'b str, handler: Handler) -> (&'b str, Func)
    where
        Handler: FnOnce(bool) -> Func,
    {
        let forbidden = self.0.map(|set| set.contains(name)).unwrap_or(false);
        (name, handler(forbidden))
    }
}

pub fn build<'a, E>(
    store: &'a mut Store<HostState<E>>,
    memory: Memory,
    forbidden_funcs: Option<BTreeSet<&'a str>>,
) -> BTreeMap<&'a str, Func>
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    E::Error: Encode + AsTerminationReason + IntoExtError,
{
    use crate::funcs::FuncsHandler as F;

    let f = FunctionBuilder(forbidden_funcs.as_ref());

    let funcs: BTreeMap<&str, Func> = [
        f.build("gr_send", |forbidden| F::send(store, forbidden, memory)),
        f.build("gr_send_wgas", |forbidden| {
            F::send_wgas(store, forbidden, memory)
        }),
        f.build("gr_send_commit", |forbidden| {
            F::send_commit(store, forbidden, memory)
        }),
        f.build("gr_send_commit_wgas", |forbidden| {
            F::send_commit_wgas(store, forbidden, memory)
        }),
        f.build("gr_send_init", |forbidden| {
            F::send_init(store, forbidden, memory)
        }),
        f.build("gr_send_push", |forbidden| {
            F::send_push(store, forbidden, memory)
        }),
        f.build("gr_reservation_send", |forbidden| {
            F::reservation_send(store, forbidden, memory)
        }),
        f.build("gr_reservation_send_commit", |forbidden| {
            F::reservation_send_commit(store, forbidden, memory)
        }),
        f.build("gr_read", |forbidden| F::read(store, forbidden, memory)),
        f.build("gr_size", |forbidden| F::size(store, forbidden)),
        f.build("gr_exit", |forbidden| F::exit(store, forbidden, memory)),
        f.build("gr_exit_code", |forbidden| {
            F::exit_code(store, forbidden, memory)
        }),
        f.build("alloc", |forbidden| F::alloc(store, forbidden, memory)),
        f.build("free", |forbidden| F::free(store, forbidden)),
        f.build("gr_block_height", |forbidden| {
            F::block_height(store, forbidden)
        }),
        f.build("gr_block_timestamp", |forbidden| {
            F::block_timestamp(store, forbidden)
        }),
        f.build("gr_origin", |forbidden| F::origin(store, forbidden, memory)),
        f.build("gr_reply", |forbidden| F::reply(store, forbidden, memory)),
        f.build("gr_reply_wgas", |forbidden| {
            F::reply_wgas(store, forbidden, memory)
        }),
        f.build("gr_reply_commit", |forbidden| {
            F::reply_commit(store, forbidden, memory)
        }),
        f.build("gr_reply_commit_wgas", |forbidden| {
            F::reply_commit_wgas(store, forbidden, memory)
        }),
        f.build("gr_reply_to", |forbidden| {
            F::reply_to(store, forbidden, memory)
        }),
        f.build("gr_reply_push", |forbidden| {
            F::reply_push(store, forbidden, memory)
        }),
        f.build("gr_debug", |forbidden| F::debug(store, forbidden, memory)),
        f.build("gr_gas_available", |forbidden| {
            F::gas_available(store, forbidden)
        }),
        f.build("gr_message_id", |forbidden| {
            F::message_id(store, forbidden, memory)
        }),
        f.build("gr_program_id", |forbidden| {
            F::program_id(store, forbidden, memory)
        }),
        f.build("gr_source", |forbidden| F::source(store, forbidden, memory)),
        f.build("gr_value", |forbidden| F::value(store, forbidden, memory)),
        f.build("gr_value_available", |forbidden| {
            F::value_available(store, forbidden, memory)
        }),
        f.build("gr_random", |forbidden| F::random(store, forbidden, memory)),
        f.build("gr_leave", |forbidden| F::leave(store, forbidden)),
        f.build("gr_wait", |forbidden| F::wait(store, forbidden)),
        f.build("gr_wait_for", |forbidden| F::wait_for(store, forbidden)),
        f.build("gr_wait_up_to", |forbidden| F::wait_up_to(store, forbidden)),
        f.build("gr_wake", |forbidden| F::wake(store, forbidden, memory)),
        f.build("gr_create_program", |forbidden| {
            F::create_program(store, forbidden, memory)
        }),
        f.build("gr_create_program_wgas", |forbidden| {
            F::create_program_wgas(store, forbidden, memory)
        }),
        f.build("gr_error", |forbidden| F::error(store, forbidden, memory)),
        f.build("gr_reserve_gas", |forbidden| {
            F::reserve_gas(store, forbidden, memory)
        }),
        f.build("gr_unreserve_gas", |forbidden| {
            F::unreserve_gas(store, forbidden, memory)
        }),
        f.build(IMPORT_NAME_OUT_OF_GAS, |_| F::out_of_gas(store)),
        f.build(IMPORT_NAME_OUT_OF_ALLOWANCE, |_| F::out_of_allowance(store)),
    ]
    .into();

    funcs
}
