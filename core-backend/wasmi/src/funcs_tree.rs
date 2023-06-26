// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

//! `build` function that collects all the syscalls.

use crate::{runtime::CallerWrap, state::HostState, wasmi::Caller};
use alloc::collections::{BTreeMap, BTreeSet};
use gear_backend_common::{
    funcs::FuncsHandler as CommonFuncsHandler, BackendAllocExternalitiesError,
    BackendExternalities, BackendExternalitiesError,
};
use gear_wasm_instrument::syscalls::SysCallName::{self, *};
use wasmi::{core::Trap, Func, Memory, Store};

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

#[allow(unused_macros)]
macro_rules! wrap_common_func_internal_ret {
    ($func:path, $($arg_name:ident),*) => {
        |store: &mut Store<_>, forbidden, memory| {
            let func = move |caller: Caller<'_, HostState<Ext>>, $($arg_name,)*| -> Result<(_, ), Trap>
            {
                let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;
                $func(&mut ctx, $($arg_name,)*).map(|ret| (ret,))
            };
            Func::wrap(store, func)
        }
    }
}

macro_rules! wrap_common_func_internal_no_ret {
    ($func:path, $($arg_name:ident),*) => {
        |store: &mut Store<_>, forbidden, memory| {
            let func = move |caller: Caller<'_, HostState<Ext>>, $($arg_name,)*| -> Result<(), Trap>
            {
                let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;
                $func(&mut ctx, $($arg_name,)*)
            };
            Func::wrap(store, func)
        }
    }
}

#[rustfmt::skip]
macro_rules! wrap_common_func {
    ($func:path, () -> ()) =>   { wrap_common_func_internal_no_ret!($func,) };
    ($func:path, (1) -> ()) =>  { wrap_common_func_internal_no_ret!($func, a) };
    ($func:path, (2) -> ()) =>  { wrap_common_func_internal_no_ret!($func, a, b) };
    ($func:path, (3) -> ()) =>  { wrap_common_func_internal_no_ret!($func, a, b, c) };
    ($func:path, (4) -> ()) =>  { wrap_common_func_internal_no_ret!($func, a, b, c, d) };
    ($func:path, (5) -> ()) =>  { wrap_common_func_internal_no_ret!($func, a, b, c, d, e) };
    ($func:path, (6) -> ()) =>  { wrap_common_func_internal_no_ret!($func, a, b, c, d, e, f) };
    ($func:path, (7) -> ()) =>  { wrap_common_func_internal_no_ret!($func, a, b, c, d, e, f, g) };
    ($func:path, (8) -> ()) =>  { wrap_common_func_internal_no_ret!($func, a, b, c, d, e, f, g, h) };

    ($func:path, () -> (1)) =>  { wrap_common_func_internal_ret!($func,)};
    ($func:path, (1) -> (1)) => { wrap_common_func_internal_ret!($func, a)};
    ($func:path, (2) -> (1)) => { wrap_common_func_internal_ret!($func, a, b)};
    ($func:path, (3) -> (1)) => { wrap_common_func_internal_ret!($func, a, b, c)};
    ($func:path, (4) -> (1)) => { wrap_common_func_internal_ret!($func, a, b, c, d)};
    ($func:path, (5) -> (1)) => { wrap_common_func_internal_ret!($func, a, b, c, d, e)};
    ($func:path, (6) -> (1)) => { wrap_common_func_internal_ret!($func, a, b, c, d, e, f)};
    ($func:path, (7) -> (1)) => { wrap_common_func_internal_ret!($func, a, b, c, d, e, f, g)};
    ($func:path, (8) -> (1)) => { wrap_common_func_internal_ret!($func, a, b, c, d, e, f, g, h)};
}

pub(crate) fn build<Ext>(
    store: &mut Store<HostState<Ext>>,
    memory: Memory,
    forbidden_funcs: BTreeSet<SysCallName>,
) -> BTreeMap<SysCallName, Func>
where
    Ext: BackendExternalities + 'static,
    Ext::Error: BackendExternalitiesError,
    Ext::AllocError: BackendAllocExternalitiesError<ExtError = Ext::Error>,
{
    let f = FunctionBuilder(forbidden_funcs);

    #[rustfmt::skip]
    let funcs: BTreeMap<_, _> = [
        f.build(Send, |forbidden| wrap_common_func!(CommonFuncsHandler::send, (5) -> ())(store, forbidden, memory)),
        f.build(SendWGas, |forbidden| wrap_common_func!(CommonFuncsHandler::send_wgas, (6) -> ())(store, forbidden, memory)),
        f.build(SendCommit, |forbidden| wrap_common_func!(CommonFuncsHandler::send_commit, (4) -> ())(store, forbidden, memory)),
        f.build(SendCommitWGas, |forbidden| wrap_common_func!(CommonFuncsHandler::send_commit_wgas, (5) -> ())(store, forbidden, memory)),
        f.build(SendInit, |forbidden| wrap_common_func!(CommonFuncsHandler::send_init, (1) -> ())(store, forbidden, memory)),
        f.build(SendPush, |forbidden| wrap_common_func!(CommonFuncsHandler::send_push, (4) -> ())(store, forbidden, memory)),
        f.build(Read, |forbidden| wrap_common_func!(CommonFuncsHandler::read, (4) -> ())(store, forbidden, memory)),
        f.build(Size, |forbidden| wrap_common_func!(CommonFuncsHandler::size, (1) -> ())(store, forbidden, memory)),
        f.build(Exit, |forbidden| wrap_common_func!(CommonFuncsHandler::exit, (1) -> ())(store, forbidden, memory)),
        f.build(ReplyCode, |forbidden| wrap_common_func!(CommonFuncsHandler::reply_code, (1) -> ())(store, forbidden, memory)),
        f.build(SignalCode, |forbidden| wrap_common_func!(CommonFuncsHandler::signal_code, (1) -> ())(store, forbidden, memory)),
        f.build(Alloc, |forbidden| wrap_common_func!(CommonFuncsHandler::alloc, (1) -> (1))(store, forbidden, memory)),
        f.build(Free, |forbidden| wrap_common_func!(CommonFuncsHandler::free, (1) -> (1))(store, forbidden, memory)),
        f.build(BlockHeight, |forbidden| wrap_common_func!(CommonFuncsHandler::block_height, (1) -> ())(store, forbidden, memory)),
        f.build(BlockTimestamp, |forbidden| wrap_common_func!(CommonFuncsHandler::block_timestamp, (1) -> ())(store, forbidden, memory)),
        f.build(ReservationSend, |forbidden| wrap_common_func!(CommonFuncsHandler::reservation_send, (5) -> ())(store, forbidden, memory)),
        f.build(ReservationSendWGas, |forbidden| wrap_common_func!(CommonFuncsHandler::reservation_send_wgas, (6) -> ())(store, forbidden, memory)),
        f.build(ReservationSendCommit, |forbidden| wrap_common_func!(CommonFuncsHandler::reservation_send_commit, (4) -> ())(store, forbidden, memory)),
        f.build(ReservationSendCommitWGas, |forbidden| wrap_common_func!(CommonFuncsHandler::reservation_send_commit_wgas, (5) -> ())(store, forbidden, memory)),
        f.build(Reply, |forbidden| wrap_common_func!(CommonFuncsHandler::reply, (4) -> ())(store, forbidden, memory)),
        f.build(ReplyWGas, |forbidden| wrap_common_func!(CommonFuncsHandler::reply_wgas, (5) -> ())(store, forbidden, memory)),
        f.build(ReplyCommit, |forbidden| wrap_common_func!(CommonFuncsHandler::reply_commit, (2) -> ())(store, forbidden, memory)),
        f.build(ReplyCommitWGas, |forbidden| wrap_common_func!(CommonFuncsHandler::reply_commit_wgas, (3) -> ())(store, forbidden, memory)),
        f.build(ReplyTo, |forbidden| wrap_common_func!(CommonFuncsHandler::reply_to, (1) -> ())(store, forbidden, memory)),
        f.build(SignalFrom, |forbidden| wrap_common_func!(CommonFuncsHandler::signal_from, (1) -> ())(store, forbidden, memory)),
        f.build(ReplyPush, |forbidden| wrap_common_func!(CommonFuncsHandler::reply_push, (3) -> ())(store, forbidden, memory)),
        f.build(ReplyInput, |forbidden| wrap_common_func!(CommonFuncsHandler::reply_input, (4) -> ())(store, forbidden, memory)),
        f.build(ReplyPushInput, |forbidden| wrap_common_func!(CommonFuncsHandler::reply_push_input, (3) -> ())(store, forbidden, memory)),
        f.build(ReplyInputWGas, |forbidden| wrap_common_func!(CommonFuncsHandler::reply_input_wgas, (5) -> ())(store, forbidden, memory)),
        f.build(SendInput, |forbidden| wrap_common_func!(CommonFuncsHandler::send_input, (5) -> ())(store, forbidden, memory)),
        f.build(SendPushInput, |forbidden| wrap_common_func!(CommonFuncsHandler::send_push_input, (4) -> ())(store, forbidden, memory)),
        f.build(SendInputWGas, |forbidden| wrap_common_func!(CommonFuncsHandler::send_input_wgas, (6) -> ())(store, forbidden, memory)),
        f.build(Debug, |forbidden| wrap_common_func!(CommonFuncsHandler::debug, (2) -> ())(store, forbidden, memory)),
        f.build(Panic, |forbidden| wrap_common_func!(CommonFuncsHandler::panic, (2) -> ())(store, forbidden, memory)),
        f.build(OomPanic, |forbidden| wrap_common_func!(CommonFuncsHandler::oom_panic, () -> ())(store, forbidden, memory)),
        f.build(GasAvailable, |forbidden| wrap_common_func!(CommonFuncsHandler::gas_available, (1) -> ())(store, forbidden, memory)),
        f.build(MessageId, |forbidden| wrap_common_func!(CommonFuncsHandler::message_id, (1) -> ())(store, forbidden, memory)),
        f.build(ReservationReply, |forbidden| wrap_common_func!(CommonFuncsHandler::reservation_reply, (4) -> ())(store, forbidden, memory)),
        f.build(ReservationReplyWGas, |forbidden| wrap_common_func!(CommonFuncsHandler::reservation_reply_wgas, (5) -> ())(store, forbidden, memory)),
        f.build(ReservationReplyCommit, |forbidden| wrap_common_func!(CommonFuncsHandler::reservation_reply_commit, (2) -> ())(store, forbidden, memory)),
        f.build(ReservationReplyCommitWGas, |forbidden| wrap_common_func!(CommonFuncsHandler::reservation_reply_commit_wgas, (3) -> ())(store, forbidden, memory)),
        f.build(PayProgramRent, |forbidden| wrap_common_func!(CommonFuncsHandler::pay_program_rent, (2) -> ())(store, forbidden, memory)),
        f.build(ProgramId, |forbidden| wrap_common_func!(CommonFuncsHandler::program_id, (1) -> ())(store, forbidden, memory)),
        f.build(Source, |forbidden| wrap_common_func!(CommonFuncsHandler::source, (1) -> ())(store, forbidden, memory)),
        f.build(Value, |forbidden| wrap_common_func!(CommonFuncsHandler::value, (1) -> ())(store, forbidden, memory)),
        f.build(ValueAvailable, |forbidden| wrap_common_func!(CommonFuncsHandler::value_available, (1) -> ())(store, forbidden, memory)),
        f.build(Random, |forbidden| wrap_common_func!(CommonFuncsHandler::random, (2) -> ())(store, forbidden, memory)),
        f.build(Leave, |forbidden| wrap_common_func!(CommonFuncsHandler::leave, () -> ())(store, forbidden, memory)),
        f.build(Wait, |forbidden| wrap_common_func!(CommonFuncsHandler::wait, () -> ())(store, forbidden, memory)),
        f.build(WaitFor, |forbidden| wrap_common_func!(CommonFuncsHandler::wait_for, (1) -> ())(store, forbidden, memory)),
        f.build(WaitUpTo, |forbidden| wrap_common_func!(CommonFuncsHandler::wait_up_to, (1) -> ())(store, forbidden, memory)),
        f.build(Wake, |forbidden| wrap_common_func!(CommonFuncsHandler::wake, (3) -> ())(store, forbidden, memory)),
        f.build(CreateProgram, |forbidden| wrap_common_func!(CommonFuncsHandler::create_program, (7) -> ())(store, forbidden, memory)),
        f.build(CreateProgramWGas, |forbidden| wrap_common_func!(CommonFuncsHandler::create_program_wgas, (8) -> ())(store, forbidden, memory)),
        f.build(ReserveGas, |forbidden| wrap_common_func!(CommonFuncsHandler::reserve_gas, (3) -> ())(store, forbidden, memory)),
        f.build(ReplyDeposit, |forbidden| wrap_common_func!(CommonFuncsHandler::reply_deposit, (3) -> ())(store, forbidden, memory)),
        f.build(UnreserveGas, |forbidden| wrap_common_func!(CommonFuncsHandler::unreserve_gas, (2) -> ())(store, forbidden, memory)),
        f.build(OutOfGas, |_| wrap_common_func!(CommonFuncsHandler::out_of_gas, () -> ())(store, false, memory)),
        f.build(OutOfAllowance, |_| wrap_common_func!(CommonFuncsHandler::out_of_allowance, () -> ())(store, false, memory)),
        f.build(SystemReserveGas, |forbidden| wrap_common_func!(CommonFuncsHandler::system_reserve_gas, (2) -> ())(store, forbidden, memory)),
    ]
    .into();

    assert_eq!(
        funcs.len(),
        SysCallName::count(),
        "Not all existing sys-calls were added to the module's env."
    );

    funcs
}
