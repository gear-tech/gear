// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

//! Syscall implementations generic over wasmi and sandbox backends.

use crate::{
    error::{
        ActorTerminationReason, BackendAllocSyscallError, BackendSyscallError, RunFallibleError,
        TrapExplanation, UndefinedTerminationReason, UnrecoverableExecutionError,
        UnrecoverableMemoryError,
    },
    memory::{ExecutorMemory, MemoryAccessError, WasmMemoryRead},
    runtime::CallerWrap,
    state::HostState,
    BackendExternalities,
};
use alloc::string::{String, ToString};
use blake2_rfc::blake2b::blake2b;
use core::marker::PhantomData;
use gear_core::{
    buffer::{RuntimeBuffer, RuntimeBufferSizeError},
    costs::RuntimeCosts,
    env::{DropPayloadLockBound, Externalities},
    gas::CounterType,
    ids::{MessageId, ProgramId},
    message::{
        HandlePacket, InitPacket, MessageWaitedType, Payload, PayloadSizeError, ReplyPacket,
    },
    pages::{PageNumber, PageU32Size, WasmPage},
};
use gear_core_errors::{MessageError, ReplyCode, SignalCode};
use gear_sandbox::{ReturnValue, Value};
use gear_sandbox_env::{HostError, WasmReturnValue};
use gsys::{
    BlockNumberWithHash, ErrorBytes, ErrorWithBlockNumberAndValue, ErrorWithGas, ErrorWithHandle,
    ErrorWithHash, ErrorWithReplyCode, ErrorWithSignalCode, ErrorWithTwoHashes, Gas, Hash,
    HashWithValue, TwoHashesWithValue,
};

const PTR_SPECIAL: u32 = u32::MAX;

/// Actually just wrapper around [`Value`] to implement conversions.
#[derive(Clone, Copy)]
struct SysCallValue(Value);

impl From<i32> for SysCallValue {
    fn from(value: i32) -> Self {
        SysCallValue(Value::I32(value))
    }
}

impl From<u32> for SysCallValue {
    fn from(value: u32) -> Self {
        SysCallValue(Value::I32(value as i32))
    }
}

impl From<i64> for SysCallValue {
    fn from(value: i64) -> Self {
        SysCallValue(Value::I64(value))
    }
}

impl TryFrom<SysCallValue> for u32 {
    type Error = HostError;

    fn try_from(val: SysCallValue) -> Result<u32, HostError> {
        if let Value::I32(val) = val.0 {
            Ok(val as u32)
        } else {
            Err(HostError)
        }
    }
}

impl TryFrom<SysCallValue> for u64 {
    type Error = HostError;

    fn try_from(val: SysCallValue) -> Result<u64, HostError> {
        if let Value::I64(val) = val.0 {
            Ok(val as u64)
        } else {
            Err(HostError)
        }
    }
}

/// Actually just wrapper around [`ReturnValue`] to implement conversions.
pub struct SysCallReturnValue(ReturnValue);

impl From<SysCallReturnValue> for ReturnValue {
    fn from(value: SysCallReturnValue) -> Self {
        value.0
    }
}

impl From<()> for SysCallReturnValue {
    fn from((): ()) -> Self {
        Self(ReturnValue::Unit)
    }
}

impl From<i32> for SysCallReturnValue {
    fn from(value: i32) -> Self {
        Self(ReturnValue::Value(Value::I32(value)))
    }
}

impl From<u32> for SysCallReturnValue {
    fn from(value: u32) -> Self {
        Self(ReturnValue::Value(Value::I32(value as i32)))
    }
}

pub(crate) trait SysCallContext: Sized {
    fn from_args(args: &[Value]) -> Result<(Self, &[Value]), HostError>;
}

pub(crate) trait SysCall<Ext, T = ()> {
    type Context: SysCallContext;

    fn execute(
        self,
        caller: &mut CallerWrap<Ext>,
        ctx: Self::Context,
    ) -> Result<(u64, T), HostError>;
}

pub(crate) trait SysCallBuilder<Ext, Args: ?Sized, R, S> {
    fn build(self, args: &[Value]) -> Result<S, HostError>;
}

impl<Ext, R, S, B> SysCallBuilder<Ext, (), R, S> for B
where
    B: FnOnce() -> S,
    S: SysCall<Ext, R>,
{
    fn build(self, args: &[Value]) -> Result<S, HostError> {
        let _: [Value; 0] = args.try_into().map_err(|_| HostError)?;
        Ok((self)())
    }
}

impl<Ext, R, S, B> SysCallBuilder<Ext, [Value], R, S> for B
where
    B: for<'a> FnOnce(&'a [Value]) -> S,
    S: SysCall<Ext, R>,
{
    fn build(self, args: &[Value]) -> Result<S, HostError> {
        Ok((self)(args))
    }
}

// implement [`SysCallBuilder`] for functions with different amount of arguments
macro_rules! impl_syscall_builder {
    ($($generic:ident),+) => {
        #[allow(non_snake_case)]
        impl<Ext, Res, Call, Builder, $($generic),+> SysCallBuilder<Ext, ($($generic,)+), Res, Call>
            for Builder
        where
            Builder: FnOnce($($generic),+) -> Call,
            Call: SysCall<Ext, Res>,
            $( $generic: TryFrom<SysCallValue, Error = HostError>,)+
        {
            fn build(self, args: &[Value]) -> Result<Call, HostError> {
                const ARGS_AMOUNT: usize = impl_syscall_builder!(@count $($generic),+);

                let [$($generic),+]: [Value; ARGS_AMOUNT] = args.try_into().map_err(|_| HostError)?;
                $(
                    let $generic = SysCallValue($generic).try_into()?;
                )+
                Ok((self)($($generic),+))
            }
        }
    };
    (@count $generic:ident) => { 1 };
    (@count $generic:ident, $($generics:ident),+) => { 1 + impl_syscall_builder!(@count $($generics),+) };
}

impl_syscall_builder!(A);
impl_syscall_builder!(A, B);
impl_syscall_builder!(A, B, C);
impl_syscall_builder!(A, B, C, D);
impl_syscall_builder!(A, B, C, D, E);
impl_syscall_builder!(A, B, C, D, E, F);
impl_syscall_builder!(A, B, C, D, E, F, G);

type SimpleSysCall<F> = F;

impl<T, F, Ext> SysCall<Ext, T> for SimpleSysCall<F>
where
    F: FnOnce(&mut CallerWrap<Ext>) -> Result<T, HostError>,
    Ext: BackendExternalities + 'static,
{
    type Context = InfallibleSysCallContext;

    fn execute(
        self,
        caller: &mut CallerWrap<Ext>,
        ctx: Self::Context,
    ) -> Result<(Gas, T), HostError> {
        let res = (self)(caller)?;
        let InfallibleSysCallContext { gas } = ctx;
        Ok((gas, res))
    }
}

struct FallibleSysCallContext {
    gas: Gas,
    res_ptr: u32,
}

impl SysCallContext for FallibleSysCallContext {
    fn from_args(args: &[Value]) -> Result<(Self, &[Value]), HostError> {
        let (gas, args) = args.split_first().ok_or(HostError)?;
        let gas: Gas = SysCallValue(*gas).try_into()?;
        let (res_ptr, args) = args.split_last().ok_or(HostError)?;
        let res_ptr: u32 = SysCallValue(*res_ptr).try_into()?;
        Ok((FallibleSysCallContext { gas, res_ptr }, args))
    }
}

#[derive(Default)]
struct FallibleSysCallError<T>(PhantomData<T>);

type FallibleSysCall<E, F> = (RuntimeCosts, FallibleSysCallError<E>, F);

impl<T, E, F, Ext> SysCall<Ext, ()> for FallibleSysCall<E, F>
where
    F: FnOnce(&mut CallerWrap<Ext>) -> Result<T, RunFallibleError>,
    E: From<Result<T, u32>>,
    Ext: BackendExternalities + 'static,
{
    type Context = FallibleSysCallContext;

    fn execute(
        self,
        caller: &mut CallerWrap<Ext>,
        context: Self::Context,
    ) -> Result<(u64, ()), HostError> {
        let (costs, _error, func) = self;
        let FallibleSysCallContext { gas, res_ptr } = context;
        caller.run_fallible::<T, _, E>(gas, res_ptr, costs, func)
    }
}

pub struct InfallibleSysCallContext {
    gas: Gas,
}

impl SysCallContext for InfallibleSysCallContext {
    fn from_args(args: &[Value]) -> Result<(Self, &[Value]), HostError> {
        let (gas, args) = args.split_first().ok_or(HostError)?;
        let gas: Gas = SysCallValue(*gas).try_into()?;
        Ok((Self { gas }, args))
    }
}

type InfallibleSysCall<F> = (RuntimeCosts, F);

impl<T, F, Ext> SysCall<Ext, T> for InfallibleSysCall<F>
where
    F: Fn(&mut CallerWrap<Ext>) -> Result<T, UndefinedTerminationReason>,
    Ext: BackendExternalities + 'static,
{
    type Context = InfallibleSysCallContext;

    fn execute(
        self,
        caller: &mut CallerWrap<Ext>,
        ctx: Self::Context,
    ) -> Result<(u64, T), HostError> {
        let (costs, func) = self;
        let InfallibleSysCallContext { gas } = ctx;
        caller.run_any::<T, _>(gas, costs, func)
    }
}

pub(crate) struct FuncsHandler<Ext: Externalities + 'static> {
    _phantom: PhantomData<Ext>,
}

impl<Ext> FuncsHandler<Ext>
where
    Ext: BackendExternalities + 'static,
    Ext::UnrecoverableError: BackendSyscallError,
    RunFallibleError: From<Ext::FallibleError>,
    Ext::AllocError: BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
{
    pub fn execute<B, Args, R, S>(
        caller: &mut gear_sandbox::default_executor::Caller<HostState<Ext, ExecutorMemory>>,
        args: &[Value],
        builder: B,
    ) -> Result<WasmReturnValue, HostError>
    where
        B: SysCallBuilder<Ext, Args, R, S>,
        Args: ?Sized,
        S: SysCall<Ext, R>,
        R: Into<SysCallReturnValue>,
    {
        crate::log::trace_syscall::<B>(args);

        let mut caller = CallerWrap::prepare(caller);

        let (ctx, args) = S::Context::from_args(args)?;
        let sys_call = builder.build(args)?;
        let (gas, value) = sys_call.execute(&mut caller, ctx)?;
        let value = value.into();

        Ok(WasmReturnValue {
            gas: gas as i64,
            inner: value.0,
        })
    }

    /// !!! Usage warning: make sure to do it before any other read/write,
    /// because it may contain registered read.
    fn register_and_read_value(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        value_ptr: u32,
    ) -> Result<u128, MemoryAccessError> {
        if value_ptr != PTR_SPECIAL {
            let read_value = ctx.manager.register_read_decoded(value_ptr);
            return ctx.read_decoded(read_value);
        }

        Ok(0)
    }

    fn read_message_payload(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        read_payload: WasmMemoryRead,
    ) -> Result<Payload, RunFallibleError> {
        ctx.read(read_payload)?
            .try_into()
            .map_err(|PayloadSizeError| MessageError::MaxMessageSizeExceed.into())
            .map_err(RunFallibleError::FallibleExt)
    }

    fn send_inner(
        ctx: &mut CallerWrap<Ext>,
        pid_value_ptr: u32,
        payload_ptr: u32,
        len: u32,
        gas_limit: Option<u64>,
        delay: u32,
    ) -> Result<MessageId, RunFallibleError> {
        let read_hash_val = ctx.manager.register_read_as(pid_value_ptr);
        let read_payload = ctx.manager.register_read(payload_ptr, len);
        let HashWithValue {
            hash: destination,
            value,
        } = ctx.read_as(read_hash_val)?;
        let payload = Self::read_message_payload(ctx, read_payload)?;

        ctx.ext_mut()
            .send(
                HandlePacket::maybe_with_gas(destination.into(), payload, gas_limit, value),
                delay,
            )
            .map_err(Into::into)
    }

    pub fn send(pid_value_ptr: u32, payload_ptr: u32, len: u32, delay: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::Send(len),
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                Self::send_inner(ctx, pid_value_ptr, payload_ptr, len, None, delay)
            },
        )
    }

    pub fn send_wgas(
        pid_value_ptr: u32,
        payload_ptr: u32,
        len: u32,
        gas_limit: u64,
        delay: u32,
    ) -> impl SysCall<Ext> {
        (
            RuntimeCosts::SendWGas(len),
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                Self::send_inner(ctx, pid_value_ptr, payload_ptr, len, Some(gas_limit), delay)
            },
        )
    }

    fn send_commit_inner(
        ctx: &mut CallerWrap<Ext>,
        handle: u32,
        pid_value_ptr: u32,
        gas_limit: Option<u64>,
        delay: u32,
    ) -> Result<MessageId, RunFallibleError> {
        let read_pid_value = ctx.manager.register_read_as(pid_value_ptr);
        let HashWithValue {
            hash: destination,
            value,
        } = ctx.read_as(read_pid_value)?;

        ctx.ext_mut()
            .send_commit(
                handle,
                HandlePacket::maybe_with_gas(
                    destination.into(),
                    Default::default(),
                    gas_limit,
                    value,
                ),
                delay,
            )
            .map_err(Into::into)
    }

    pub fn send_commit(handle: u32, pid_value_ptr: u32, delay: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::SendCommit,
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                Self::send_commit_inner(ctx, handle, pid_value_ptr, None, delay)
            },
        )
    }

    pub fn send_commit_wgas(
        handle: u32,
        pid_value_ptr: u32,
        gas_limit: u64,
        delay: u32,
    ) -> impl SysCall<Ext> {
        (
            RuntimeCosts::SendCommitWGas,
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                Self::send_commit_inner(ctx, handle, pid_value_ptr, Some(gas_limit), delay)
            },
        )
    }

    pub fn send_init() -> impl SysCall<Ext> {
        (
            RuntimeCosts::SendInit,
            FallibleSysCallError::<ErrorWithHandle>::default(),
            move |ctx: &mut CallerWrap<Ext>| ctx.ext_mut().send_init().map_err(Into::into),
        )
    }

    pub fn send_push(handle: u32, payload_ptr: u32, len: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::SendPush(len),
            FallibleSysCallError::<ErrorBytes>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                let read_payload = ctx.manager.register_read(payload_ptr, len);
                let payload = ctx.read(read_payload)?;

                ctx.ext_mut()
                    .send_push(handle, &payload)
                    .map_err(Into::into)
            },
        )
    }

    pub fn reservation_send(
        rid_pid_value_ptr: u32,
        payload_ptr: u32,
        len: u32,
        delay: u32,
    ) -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReservationSend(len),
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                let read_rid_pid_value = ctx.manager.register_read_as(rid_pid_value_ptr);
                let read_payload = ctx.manager.register_read(payload_ptr, len);
                let TwoHashesWithValue {
                    hash1: reservation_id,
                    hash2: destination,
                    value,
                } = ctx.read_as(read_rid_pid_value)?;
                let payload = Self::read_message_payload(ctx, read_payload)?;

                ctx.ext_mut()
                    .reservation_send(
                        reservation_id.into(),
                        HandlePacket::new(destination.into(), payload, value),
                        delay,
                    )
                    .map_err(Into::into)
            },
        )
    }

    pub fn reservation_send_commit(
        handle: u32,
        rid_pid_value_ptr: u32,
        delay: u32,
    ) -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReservationSendCommit,
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                let read_rid_pid_value = ctx.manager.register_read_as(rid_pid_value_ptr);
                let TwoHashesWithValue {
                    hash1: reservation_id,
                    hash2: destination,
                    value,
                } = ctx.read_as(read_rid_pid_value)?;

                ctx.ext_mut()
                    .reservation_send_commit(
                        reservation_id.into(),
                        handle,
                        HandlePacket::new(destination.into(), Default::default(), value),
                        delay,
                    )
                    .map_err(Into::into)
            },
        )
    }

    pub fn read(at: u32, len: u32, buffer_ptr: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::Read,
            FallibleSysCallError::<ErrorBytes>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                let payload_lock = ctx.ext_mut().lock_payload(at, len)?;
                payload_lock
                    .drop_with::<MemoryAccessError, _>(|payload_access| {
                        let write_buffer = ctx.manager.register_write(buffer_ptr, len);
                        let write_res = ctx.write(write_buffer, payload_access.as_slice());
                        let unlock_bound = ctx.ext_mut().unlock_payload(payload_access.into_lock());

                        DropPayloadLockBound::from((unlock_bound, write_res))
                    })
                    .into_inner()
                    .map_err(Into::into)
            },
        )
    }

    pub fn size(size_ptr: u32) -> impl SysCall<Ext> {
        (RuntimeCosts::Size, move |ctx: &mut CallerWrap<Ext>| {
            let size = ctx.ext_mut().size()? as u32;

            let write_size = ctx.manager.register_write_as(size_ptr);
            ctx.write_as(write_size, size.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn exit(inheritor_id_ptr: u32) -> impl SysCall<Ext> {
        (RuntimeCosts::Exit, move |ctx: &mut CallerWrap<Ext>| {
            let read_inheritor_id = ctx.manager.register_read_decoded(inheritor_id_ptr);
            let inheritor_id = ctx.read_decoded(read_inheritor_id)?;
            Err(ActorTerminationReason::Exit(inheritor_id).into())
        })
    }

    pub fn reply_code() -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReplyCode,
            FallibleSysCallError::<ErrorWithReplyCode>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                ctx.ext_mut()
                    .reply_code()
                    .map(ReplyCode::to_bytes)
                    .map_err(Into::into)
            },
        )
    }

    pub fn signal_code() -> impl SysCall<Ext> {
        (
            RuntimeCosts::SignalCode,
            FallibleSysCallError::<ErrorWithSignalCode>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                ctx.ext_mut()
                    .signal_code()
                    .map(SignalCode::to_u32)
                    .map_err(Into::into)
            },
        )
    }

    pub fn alloc(pages: u32) -> impl SysCall<Ext, u32> {
        (
            RuntimeCosts::Alloc(pages),
            move |ctx: &mut CallerWrap<Ext>| {
                let res = ctx.alloc(pages);
                let res = ctx.process_alloc_func_result(res)?;

                let page = match res {
                    Ok(page) => {
                        log::trace!("Alloc {pages:?} pages at {page:?}");
                        page.raw()
                    }
                    Err(err) => {
                        log::trace!("Alloc failed: {err}");
                        u32::MAX
                    }
                };
                Ok(page)
            },
        )
    }

    pub fn free(page_no: u32) -> impl SysCall<Ext, i32> {
        (RuntimeCosts::Free, move |ctx: &mut CallerWrap<Ext>| {
            let page = WasmPage::new(page_no).map_err(|_| {
                UndefinedTerminationReason::Actor(ActorTerminationReason::Trap(
                    TrapExplanation::Unknown,
                ))
            })?;

            let res = ctx.ext_mut().free(page);
            let res = ctx.process_alloc_func_result(res)?;

            match &res {
                Ok(()) => {
                    log::trace!("Free {page:?}");
                }
                Err(err) => {
                    log::trace!("Free failed: {err}");
                }
            };

            Ok(res.is_err() as i32)
        })
    }

    pub fn block_height(height_ptr: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::BlockHeight,
            move |ctx: &mut CallerWrap<Ext>| {
                let height = ctx.ext_mut().block_height()?;

                let write_height = ctx.manager.register_write_as(height_ptr);
                ctx.write_as(write_height, height.to_le_bytes())
                    .map_err(Into::into)
            },
        )
    }

    pub fn block_timestamp(timestamp_ptr: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::BlockTimestamp,
            move |ctx: &mut CallerWrap<Ext>| {
                let timestamp = ctx.ext_mut().block_timestamp()?;

                let write_timestamp = ctx.manager.register_write_as(timestamp_ptr);
                ctx.write_as(write_timestamp, timestamp.to_le_bytes())
                    .map_err(Into::into)
            },
        )
    }

    pub fn random(subject_ptr: u32, bn_random_ptr: u32) -> impl SysCall<Ext> {
        (RuntimeCosts::Random, move |ctx: &mut CallerWrap<Ext>| {
            let read_subject = ctx.manager.register_read_decoded(subject_ptr);
            let write_bn_random = ctx.manager.register_write_as(bn_random_ptr);

            let raw_subject: Hash = ctx.read_decoded(read_subject)?;

            let (random, bn) = ctx.ext_mut().random()?;
            let subject = [&raw_subject, random].concat();

            let mut hash = [0; 32];
            hash.copy_from_slice(blake2b(32, &[], &subject).as_bytes());

            ctx.write_as(write_bn_random, BlockNumberWithHash { bn, hash })
                .map_err(Into::into)
        })
    }

    fn reply_inner(
        ctx: &mut CallerWrap<Ext>,
        payload_ptr: u32,
        len: u32,
        gas_limit: Option<u64>,
        value_ptr: u32,
    ) -> Result<MessageId, RunFallibleError> {
        let read_payload = ctx.manager.register_read(payload_ptr, len);
        let value = Self::register_and_read_value(ctx, value_ptr)?;
        let payload = Self::read_message_payload(ctx, read_payload)?;

        ctx.ext_mut()
            .reply(ReplyPacket::maybe_with_gas(payload, gas_limit, value))
            .map_err(Into::into)
    }

    pub fn reply(payload_ptr: u32, len: u32, value_ptr: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::Reply(len),
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                Self::reply_inner(ctx, payload_ptr, len, None, value_ptr)
            },
        )
    }

    pub fn reply_wgas(
        payload_ptr: u32,
        len: u32,
        gas_limit: u64,
        value_ptr: u32,
    ) -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReplyWGas(len),
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                Self::reply_inner(ctx, payload_ptr, len, Some(gas_limit), value_ptr)
            },
        )
    }

    fn reply_commit_inner(
        ctx: &mut CallerWrap<Ext>,
        gas_limit: Option<u64>,
        value_ptr: u32,
    ) -> Result<MessageId, RunFallibleError> {
        let value = Self::register_and_read_value(ctx, value_ptr)?;

        ctx.ext_mut()
            .reply_commit(ReplyPacket::maybe_with_gas(
                Default::default(),
                gas_limit,
                value,
            ))
            .map_err(Into::into)
    }

    pub fn reply_commit(value_ptr: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReplyCommit,
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| Self::reply_commit_inner(ctx, None, value_ptr),
        )
    }

    pub fn reply_commit_wgas(gas_limit: u64, value_ptr: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReplyCommitWGas,
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                Self::reply_commit_inner(ctx, Some(gas_limit), value_ptr)
            },
        )
    }

    pub fn reservation_reply(rid_value_ptr: u32, payload_ptr: u32, len: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReservationReply(len),
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                let read_rid_value = ctx.manager.register_read_as(rid_value_ptr);
                let read_payload = ctx.manager.register_read(payload_ptr, len);
                let HashWithValue {
                    hash: reservation_id,
                    value,
                } = ctx.read_as(read_rid_value)?;
                let payload = Self::read_message_payload(ctx, read_payload)?;

                ctx.ext_mut()
                    .reservation_reply(reservation_id.into(), ReplyPacket::new(payload, value))
                    .map_err(Into::into)
            },
        )
    }

    pub fn reservation_reply_commit(rid_value_ptr: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReservationReplyCommit,
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                let read_rid_value = ctx.manager.register_read_as(rid_value_ptr);
                let HashWithValue {
                    hash: reservation_id,
                    value,
                } = ctx.read_as(read_rid_value)?;

                ctx.ext_mut()
                    .reservation_reply_commit(
                        reservation_id.into(),
                        ReplyPacket::new(Default::default(), value),
                    )
                    .map_err(Into::into)
            },
        )
    }

    pub fn reply_to() -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReplyTo,
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| ctx.ext_mut().reply_to().map_err(Into::into),
        )
    }

    pub fn signal_from() -> impl SysCall<Ext> {
        (
            RuntimeCosts::SignalFrom,
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| ctx.ext_mut().signal_from().map_err(Into::into),
        )
    }

    pub fn reply_push(payload_ptr: u32, len: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReplyPush(len),
            FallibleSysCallError::<ErrorBytes>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                let read_payload = ctx.manager.register_read(payload_ptr, len);
                let payload = ctx.read(read_payload)?;

                ctx.ext_mut().reply_push(&payload).map_err(Into::into)
            },
        )
    }

    fn reply_input_inner(
        ctx: &mut CallerWrap<Ext>,
        offset: u32,
        len: u32,
        gas_limit: Option<u64>,
        value_ptr: u32,
    ) -> Result<MessageId, RunFallibleError> {
        let value = Self::register_and_read_value(ctx, value_ptr)?;

        // Charge for `len` is inside `reply_push_input`
        ctx.ext_mut().reply_push_input(offset, len)?;

        ctx.ext_mut()
            .reply_commit(ReplyPacket::maybe_with_gas(
                Default::default(),
                gas_limit,
                value,
            ))
            .map_err(Into::into)
    }

    pub fn reply_input(offset: u32, len: u32, value_ptr: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReplyInput,
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                Self::reply_input_inner(ctx, offset, len, None, value_ptr)
            },
        )
    }

    pub fn reply_input_wgas(
        offset: u32,
        len: u32,
        gas_limit: u64,
        value_ptr: u32,
    ) -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReplyInputWGas,
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                Self::reply_input_inner(ctx, offset, len, Some(gas_limit), value_ptr)
            },
        )
    }

    pub fn reply_push_input(offset: u32, len: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReplyPushInput,
            FallibleSysCallError::<ErrorBytes>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                ctx.ext_mut()
                    .reply_push_input(offset, len)
                    .map_err(Into::into)
            },
        )
    }

    fn send_input_inner(
        ctx: &mut CallerWrap<Ext>,
        pid_value_ptr: u32,
        offset: u32,
        len: u32,
        gas_limit: Option<u64>,
        delay: u32,
    ) -> Result<MessageId, RunFallibleError> {
        let read_pid_value = ctx.manager.register_read_as(pid_value_ptr);
        let HashWithValue {
            hash: destination,
            value,
        } = ctx.read_as(read_pid_value)?;

        let handle = ctx.ext_mut().send_init()?;
        // Charge for `len` inside `send_push_input`
        ctx.ext_mut().send_push_input(handle, offset, len)?;

        ctx.ext_mut()
            .send_commit(
                handle,
                HandlePacket::maybe_with_gas(
                    destination.into(),
                    Default::default(),
                    gas_limit,
                    value,
                ),
                delay,
            )
            .map_err(Into::into)
    }

    pub fn send_input(pid_value_ptr: u32, offset: u32, len: u32, delay: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::SendInput,
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                Self::send_input_inner(ctx, pid_value_ptr, offset, len, None, delay)
            },
        )
    }

    pub fn send_input_wgas(
        pid_value_ptr: u32,
        offset: u32,
        len: u32,
        gas_limit: u64,
        delay: u32,
    ) -> impl SysCall<Ext> {
        (
            RuntimeCosts::SendInputWGas,
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                Self::send_input_inner(ctx, pid_value_ptr, offset, len, Some(gas_limit), delay)
            },
        )
    }

    pub fn send_push_input(handle: u32, offset: u32, len: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::SendPushInput,
            FallibleSysCallError::<ErrorBytes>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                ctx.ext_mut()
                    .send_push_input(handle, offset, len)
                    .map_err(Into::into)
            },
        )
    }

    pub fn debug(data_ptr: u32, data_len: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::Debug(data_len),
            move |ctx: &mut CallerWrap<Ext>| {
                let read_data = ctx.manager.register_read(data_ptr, data_len);
                let data: RuntimeBuffer = ctx
                    .read(read_data)?
                    .try_into()
                    .map_err(|RuntimeBufferSizeError| {
                        UnrecoverableMemoryError::RuntimeAllocOutOfBounds.into()
                    })
                    .map_err(TrapExplanation::UnrecoverableExt)?;

                let s = String::from_utf8(data.into_vec())
                    .map_err(|_err| UnrecoverableExecutionError::InvalidDebugString.into())
                    .map_err(TrapExplanation::UnrecoverableExt)?;
                ctx.ext_mut().debug(&s)?;

                Ok(())
            },
        )
    }

    pub fn panic(data_ptr: u32, data_len: u32) -> impl SysCall<Ext> {
        (RuntimeCosts::Null, move |ctx: &mut CallerWrap<Ext>| {
            let read_data = ctx.manager.register_read(data_ptr, data_len);
            let data = ctx.read(read_data).unwrap_or_default();

            let s = String::from_utf8_lossy(&data).to_string();

            Err(ActorTerminationReason::Trap(TrapExplanation::Panic(s.into())).into())
        })
    }

    pub fn oom_panic() -> impl SysCall<Ext> {
        (RuntimeCosts::Null, |_ctx: &mut CallerWrap<Ext>| {
            Err(ActorTerminationReason::Trap(TrapExplanation::ProgramAllocOutOfBounds).into())
        })
    }

    pub fn reserve_gas(gas_value: u64, duration: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReserveGas,
            FallibleSysCallError::<ErrorWithHash>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                ctx.ext_mut()
                    .reserve_gas(gas_value, duration)
                    .map_err(Into::into)
            },
        )
    }

    pub fn reply_deposit(message_id_ptr: u32, gas_value: u64) -> impl SysCall<Ext> {
        (
            RuntimeCosts::ReplyDeposit,
            FallibleSysCallError::<ErrorBytes>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                let read_message_id = ctx.manager.register_read_decoded(message_id_ptr);
                let message_id = ctx.read_decoded(read_message_id)?;

                ctx.ext_mut()
                    .reply_deposit(message_id, gas_value)
                    .map_err(Into::into)
            },
        )
    }

    pub fn unreserve_gas(reservation_id_ptr: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::UnreserveGas,
            FallibleSysCallError::<ErrorWithGas>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                let read_reservation_id = ctx.manager.register_read_decoded(reservation_id_ptr);
                let reservation_id = ctx.read_decoded(read_reservation_id)?;

                ctx.ext_mut()
                    .unreserve_gas(reservation_id)
                    .map_err(Into::into)
            },
        )
    }

    pub fn system_reserve_gas(gas_value: u64) -> impl SysCall<Ext> {
        (
            RuntimeCosts::SystemReserveGas,
            FallibleSysCallError::<ErrorBytes>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                ctx.ext_mut()
                    .system_reserve_gas(gas_value)
                    .map_err(Into::into)
            },
        )
    }

    pub fn gas_available(gas_ptr: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::GasAvailable,
            move |ctx: &mut CallerWrap<Ext>| {
                let gas_available = ctx.ext_mut().gas_available()?;

                let write_gas = ctx.manager.register_write_as(gas_ptr);
                ctx.write_as(write_gas, gas_available.to_le_bytes())
                    .map_err(Into::into)
            },
        )
    }

    pub fn message_id(message_id_ptr: u32) -> impl SysCall<Ext> {
        (RuntimeCosts::MsgId, move |ctx: &mut CallerWrap<Ext>| {
            let message_id = ctx.ext_mut().message_id()?;

            let write_message_id = ctx.manager.register_write_as(message_id_ptr);
            ctx.write_as(write_message_id, message_id.into_bytes())
                .map_err(Into::into)
        })
    }

    pub fn program_id(program_id_ptr: u32) -> impl SysCall<Ext> {
        (RuntimeCosts::ProgramId, move |ctx: &mut CallerWrap<Ext>| {
            let program_id = ctx.ext_mut().program_id()?;

            let write_program_id = ctx.manager.register_write_as(program_id_ptr);
            ctx.write_as(write_program_id, program_id.into_bytes())
                .map_err(Into::into)
        })
    }

    pub fn pay_program_rent(rent_pid_ptr: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::PayProgramRent,
            FallibleSysCallError::<ErrorWithBlockNumberAndValue>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                let read_rent_pid = ctx.manager.register_read_as(rent_pid_ptr);

                let HashWithValue {
                    hash: program_id,
                    value: rent,
                } = ctx.read_as(read_rent_pid)?;

                ctx.ext_mut()
                    .pay_program_rent(program_id.into(), rent)
                    .map_err(Into::into)
            },
        )
    }

    pub fn source(source_ptr: u32) -> impl SysCall<Ext> {
        (RuntimeCosts::Source, move |ctx: &mut CallerWrap<Ext>| {
            let source = ctx.ext_mut().source()?;

            let write_source = ctx.manager.register_write_as(source_ptr);
            ctx.write_as(write_source, source.into_bytes())
                .map_err(Into::into)
        })
    }

    pub fn value(value_ptr: u32) -> impl SysCall<Ext> {
        (RuntimeCosts::Value, move |ctx: &mut CallerWrap<Ext>| {
            let value = ctx.ext_mut().value()?;

            let write_value = ctx.manager.register_write_as(value_ptr);
            ctx.write_as(write_value, value.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn value_available(value_ptr: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::ValueAvailable,
            move |ctx: &mut CallerWrap<Ext>| {
                let value_available = ctx.ext_mut().value_available()?;

                let write_value = ctx.manager.register_write_as(value_ptr);
                ctx.write_as(write_value, value_available.to_le_bytes())
                    .map_err(Into::into)
            },
        )
    }

    pub fn leave() -> impl SysCall<Ext> {
        (RuntimeCosts::Leave, move |_ctx: &mut CallerWrap<Ext>| {
            Err(ActorTerminationReason::Leave.into())
        })
    }

    pub fn wait() -> impl SysCall<Ext> {
        (RuntimeCosts::Wait, move |ctx: &mut CallerWrap<Ext>| {
            ctx.ext_mut().wait()?;
            Err(ActorTerminationReason::Wait(None, MessageWaitedType::Wait).into())
        })
    }

    pub fn wait_for(duration: u32) -> impl SysCall<Ext> {
        (RuntimeCosts::WaitFor, move |ctx: &mut CallerWrap<Ext>| {
            ctx.ext_mut().wait_for(duration)?;
            Err(ActorTerminationReason::Wait(Some(duration), MessageWaitedType::WaitFor).into())
        })
    }

    pub fn wait_up_to(duration: u32) -> impl SysCall<Ext> {
        (RuntimeCosts::WaitUpTo, move |ctx: &mut CallerWrap<Ext>| {
            let waited_type = if ctx.ext_mut().wait_up_to(duration)? {
                MessageWaitedType::WaitUpToFull
            } else {
                MessageWaitedType::WaitUpTo
            };
            Err(ActorTerminationReason::Wait(Some(duration), waited_type).into())
        })
    }

    pub fn wake(message_id_ptr: u32, delay: u32) -> impl SysCall<Ext> {
        (
            RuntimeCosts::Wake,
            FallibleSysCallError::<ErrorBytes>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                let read_message_id = ctx.manager.register_read_decoded(message_id_ptr);
                let message_id = ctx.read_decoded(read_message_id)?;

                ctx.ext_mut().wake(message_id, delay).map_err(Into::into)
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn create_program_inner(
        ctx: &mut CallerWrap<Ext>,
        cid_value_ptr: u32,
        salt_ptr: u32,
        salt_len: u32,
        payload_ptr: u32,
        payload_len: u32,
        gas_limit: Option<u64>,
        delay: u32,
    ) -> Result<(MessageId, ProgramId), RunFallibleError> {
        let read_cid_value = ctx.manager.register_read_as(cid_value_ptr);
        let read_salt = ctx.manager.register_read(salt_ptr, salt_len);
        let read_payload = ctx.manager.register_read(payload_ptr, payload_len);
        let HashWithValue {
            hash: code_id,
            value,
        } = ctx.read_as(read_cid_value)?;
        let salt = Self::read_message_payload(ctx, read_salt)?;
        let payload = Self::read_message_payload(ctx, read_payload)?;

        let message_id = ctx.ext_mut().message_id()?;

        ctx.ext_mut()
            .create_program(
                InitPacket::new_from_program(
                    code_id.into(),
                    salt,
                    payload,
                    message_id,
                    gas_limit,
                    value,
                ),
                delay,
            )
            .map_err(Into::into)
    }

    pub fn create_program(
        cid_value_ptr: u32,
        salt_ptr: u32,
        salt_len: u32,
        payload_ptr: u32,
        payload_len: u32,
        delay: u32,
    ) -> impl SysCall<Ext> {
        (
            RuntimeCosts::CreateProgram(payload_len, salt_len),
            FallibleSysCallError::<ErrorWithTwoHashes>::default(),
            move |ctx: &mut CallerWrap<Ext>| -> Result<_, RunFallibleError> {
                Self::create_program_inner(
                    ctx,
                    cid_value_ptr,
                    salt_ptr,
                    salt_len,
                    payload_ptr,
                    payload_len,
                    None,
                    delay,
                )
            },
        )
    }

    pub fn create_program_wgas(
        cid_value_ptr: u32,
        salt_ptr: u32,
        salt_len: u32,
        payload_ptr: u32,
        payload_len: u32,
        gas_limit: u64,
        delay: u32,
    ) -> impl SysCall<Ext> {
        (
            RuntimeCosts::CreateProgramWGas(payload_len, salt_len),
            FallibleSysCallError::<ErrorWithTwoHashes>::default(),
            move |ctx: &mut CallerWrap<Ext>| {
                Self::create_program_inner(
                    ctx,
                    cid_value_ptr,
                    salt_ptr,
                    salt_len,
                    payload_ptr,
                    payload_len,
                    Some(gas_limit),
                    delay,
                )
            },
        )
    }

    pub fn forbidden(_args: &[Value]) -> impl SysCall<Ext> {
        (RuntimeCosts::Null, |_: &mut CallerWrap<Ext>| {
            Err(ActorTerminationReason::Trap(TrapExplanation::ForbiddenFunction).into())
        })
    }

    pub fn out_of_gas() -> impl SysCall<Ext> {
        |ctx: &mut CallerWrap<Ext>| {
            let ext = ctx.ext_mut();
            let current_counter = ext.current_counter_type();
            log::trace!(target: "syscalls", "[out_of_gas] Current counter in global represents {current_counter:?}");

            if current_counter == CounterType::GasAllowance {
                // We manually decrease it to 0 because global won't be affected
                // since it didn't pass comparison to argument of `gas_charge()`
                ext.decrease_current_counter_to(0);
            }

            let termination_reason: ActorTerminationReason = current_counter.into();

            ctx.set_termination_reason(termination_reason.into());
            Err(HostError)
        }
    }
}
