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
use gear_backend_common::{BackendAllocExtError, BackendExt, BackendExtError};
use gear_wasm_instrument::syscalls::SysCallName::{self, *};
use wasmi::{Func, Memory, Store};

struct FunctionBuilder(BTreeSet<SysCallName>);

impl FunctionBuilder {
    fn build<Handler>(&self, name: SysCallName, handler: Handler) -> (SysCallName, Func)
    where
        Handler: FnOnce(bool) -> Func,
    {
        let forbidden = self.0.contains(&name);
        (name, handler(forbidden))
    }
}

pub(crate) fn build<E>(
    store: &mut Store<HostState<E>>,
    memory: Memory,
    forbidden_funcs: BTreeSet<SysCallName>,
) -> BTreeMap<SysCallName, Func>
where
    E: BackendExt + 'static,
    E::Error: BackendExtError,
    E::AllocError: BackendAllocExtError<ExtError = E::Error>,
{
    use crate::funcs::FuncsHandler as F;

    let f = FunctionBuilder(forbidden_funcs);

    let funcs: BTreeMap<SysCallName, Func> = [
        f.build(Send, |forbidden| F::send(store, forbidden, memory)),
        f.build(SendWGas, |forbidden| F::send_wgas(store, forbidden, memory)),
        f.build(SendCommit, |forbidden| {
            F::send_commit(store, forbidden, memory)
        }),
        f.build(SendCommitWGas, |forbidden| {
            F::send_commit_wgas(store, forbidden, memory)
        }),
        f.build(SendInit, |forbidden| F::send_init(store, forbidden, memory)),
        f.build(SendPush, |forbidden| F::send_push(store, forbidden, memory)),
        f.build(Read, |forbidden| F::read(store, forbidden, memory)),
        f.build(Size, |forbidden| F::size(store, forbidden, memory)),
        f.build(Exit, |forbidden| F::exit(store, forbidden, memory)),
        f.build(StatusCode, |forbidden| {
            F::status_code(store, forbidden, memory)
        }),
        f.build(Alloc, |forbidden| F::alloc(store, forbidden, memory)),
        f.build(Free, |forbidden| F::free(store, forbidden, memory)),
        f.build(BlockHeight, |forbidden| {
            F::block_height(store, forbidden, memory)
        }),
        f.build(BlockTimestamp, |forbidden| {
            F::block_timestamp(store, forbidden, memory)
        }),
        f.build(ReservationSend, |forbidden| {
            F::reservation_send(store, forbidden, memory)
        }),
        f.build(ReservationSendCommit, |forbidden| {
            F::reservation_send_commit(store, forbidden, memory)
        }),
        f.build(Origin, |forbidden| F::origin(store, forbidden, memory)),
        f.build(Reply, |forbidden| F::reply(store, forbidden, memory)),
        f.build(ReplyWGas, |forbidden| {
            F::reply_wgas(store, forbidden, memory)
        }),
        f.build(ReplyCommit, |forbidden| {
            F::reply_commit(store, forbidden, memory)
        }),
        f.build(ReplyCommitWGas, |forbidden| {
            F::reply_commit_wgas(store, forbidden, memory)
        }),
        f.build(ReplyTo, |forbidden| F::reply_to(store, forbidden, memory)),
        f.build(SignalFrom, |forbidden| {
            F::signal_from(store, forbidden, memory)
        }),
        f.build(ReplyPush, |forbidden| {
            F::reply_push(store, forbidden, memory)
        }),
        f.build(ReplyInput, |forbidden| {
            F::reply_input(store, forbidden, memory)
        }),
        f.build(ReplyPushInput, |forbidden| {
            F::reply_push_input(store, forbidden, memory)
        }),
        f.build(ReplyInputWGas, |forbidden| {
            F::reply_input_wgas(store, forbidden, memory)
        }),
        f.build(SendInput, |forbidden| {
            F::send_input(store, forbidden, memory)
        }),
        f.build(SendPushInput, |forbidden| {
            F::send_push_input(store, forbidden, memory)
        }),
        f.build(SendInputWGas, |forbidden| {
            F::send_input_wgas(store, forbidden, memory)
        }),
        f.build(Debug, |forbidden| F::debug(store, forbidden, memory)),
        f.build(Panic, |forbidden| F::panic(store, forbidden, memory)),
        f.build(OomPanic, |forbidden| F::oom_panic(store, forbidden, memory)),
        f.build(GasAvailable, |forbidden| {
            F::gas_available(store, forbidden, memory)
        }),
        f.build(MessageId, |forbidden| {
            F::message_id(store, forbidden, memory)
        }),
        f.build(ReservationReply, |forbidden| {
            F::reservation_reply(store, forbidden, memory)
        }),
        f.build(ReservationReplyCommit, |forbidden| {
            F::reservation_reply_commit(store, forbidden, memory)
        }),
        f.build(ProgramId, |forbidden| {
            F::program_id(store, forbidden, memory)
        }),
        f.build(Source, |forbidden| F::source(store, forbidden, memory)),
        f.build(Value, |forbidden| F::value(store, forbidden, memory)),
        f.build(ValueAvailable, |forbidden| {
            F::value_available(store, forbidden, memory)
        }),
        f.build(Random, |forbidden| F::random(store, forbidden, memory)),
        f.build(Leave, |forbidden| F::leave(store, forbidden, memory)),
        f.build(Wait, |forbidden| F::wait(store, forbidden, memory)),
        f.build(WaitFor, |forbidden| F::wait_for(store, forbidden, memory)),
        f.build(WaitUpTo, |forbidden| {
            F::wait_up_to(store, forbidden, memory)
        }),
        f.build(Wake, |forbidden| F::wake(store, forbidden, memory)),
        f.build(CreateProgram, |forbidden| {
            F::create_program(store, forbidden, memory)
        }),
        f.build(CreateProgramWGas, |forbidden| {
            F::create_program_wgas(store, forbidden, memory)
        }),
        f.build(Error, |forbidden| F::error(store, forbidden, memory)),
        f.build(ReserveGas, |forbidden| {
            F::reserve_gas(store, forbidden, memory)
        }),
        f.build(UnreserveGas, |forbidden| {
            F::unreserve_gas(store, forbidden, memory)
        }),
        f.build(OutOfGas, |_| F::out_of_gas(store)),
        f.build(OutOfAllowance, |_| F::out_of_allowance(store)),
        f.build(SystemReserveGas, |forbidden| {
            F::system_reserve_gas(store, forbidden, memory)
        }),
    ]
    .into();

    assert_eq!(
        funcs.len(),
        SysCallName::count(),
        "Not all existing sys-calls were added to the module's env."
    );

    funcs
}
