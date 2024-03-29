// This file is part of Gear.

// Copyright (C) 2023-2024 Gear Technologies Inc.
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
    costs::CostToken,
    env::{DropPayloadLockBound, Externalities},
    gas::CounterType,
    ids::{MessageId, ProgramId},
    message::{
        HandlePacket, InitPacket, MessageWaitedType, Payload, PayloadSizeError, ReplyPacket,
    },
    pages::WasmPage,
};
use gear_core_errors::{MessageError, ReplyCode, SignalCode};
use gear_sandbox::{default_executor::Caller, ReturnValue, Value};
use gear_sandbox_env::{HostError, WasmReturnValue};
use gear_wasm_instrument::SystemBreakCode;
use gsys::{
    BlockNumberWithHash, ErrorBytes, ErrorWithGas, ErrorWithHandle, ErrorWithHash,
    ErrorWithReplyCode, ErrorWithSignalCode, ErrorWithTwoHashes, Gas, Hash, HashWithValue,
    TwoHashesWithValue,
};

const PTR_SPECIAL: u32 = u32::MAX;

/// Actually just wrapper around [`Value`] to implement conversions.
#[derive(Clone, Copy)]
struct SyscallValue(Value);

impl From<i32> for SyscallValue {
    fn from(value: i32) -> Self {
        SyscallValue(Value::I32(value))
    }
}

impl From<u32> for SyscallValue {
    fn from(value: u32) -> Self {
        SyscallValue(Value::I32(value as i32))
    }
}

impl TryFrom<SyscallValue> for u32 {
    type Error = HostError;

    fn try_from(val: SyscallValue) -> Result<u32, HostError> {
        if let Value::I32(val) = val.0 {
            Ok(val as u32)
        } else {
            Err(HostError)
        }
    }
}

impl TryFrom<SyscallValue> for u64 {
    type Error = HostError;

    fn try_from(val: SyscallValue) -> Result<u64, HostError> {
        if let Value::I64(val) = val.0 {
            Ok(val as u64)
        } else {
            Err(HostError)
        }
    }
}

/// Actually just wrapper around [`ReturnValue`] to implement conversions.
pub struct SyscallReturnValue(ReturnValue);

impl From<SyscallReturnValue> for ReturnValue {
    fn from(value: SyscallReturnValue) -> Self {
        value.0
    }
}

impl From<()> for SyscallReturnValue {
    fn from((): ()) -> Self {
        Self(ReturnValue::Unit)
    }
}

impl From<i32> for SyscallReturnValue {
    fn from(value: i32) -> Self {
        Self(ReturnValue::Value(Value::I32(value)))
    }
}

impl From<u32> for SyscallReturnValue {
    fn from(value: u32) -> Self {
        Self(ReturnValue::Value(Value::I32(value as i32)))
    }
}

pub(crate) trait SyscallContext: Sized {
    fn from_args(args: &[Value]) -> Result<(Self, &[Value]), HostError>;
}

impl SyscallContext for () {
    fn from_args(args: &[Value]) -> Result<(Self, &[Value]), HostError> {
        Ok(((), args))
    }
}

pub(crate) trait Syscall<Ext, T = ()> {
    type Context: SyscallContext;

    fn execute(
        self,
        caller: &mut CallerWrap<Ext>,
        ctx: Self::Context,
    ) -> Result<(Gas, T), HostError>;
}

/// Trait is implemented for functions.
///
/// # Generics
/// `Args` is to make specialization based on function arguments
/// `Ext` and `Res` are for syscall itself (`Syscall<Ext, Res>`)
pub(crate) trait SyscallBuilder<Ext, Args: ?Sized, Res, Syscall> {
    fn build(self, args: &[Value]) -> Result<Syscall, HostError>;
}

impl<Ext, Res, Call, Builder> SyscallBuilder<Ext, (), Res, Call> for Builder
where
    Builder: FnOnce() -> Call,
    Call: Syscall<Ext, Res>,
{
    fn build(self, args: &[Value]) -> Result<Call, HostError> {
        let _: [Value; 0] = args.try_into().map_err(|_| HostError)?;
        Ok((self)())
    }
}

impl<Ext, Res, Call, Builder> SyscallBuilder<Ext, [Value], Res, Call> for Builder
where
    Builder: for<'a> FnOnce(&'a [Value]) -> Call,
    Call: Syscall<Ext, Res>,
{
    fn build(self, args: &[Value]) -> Result<Call, HostError> {
        Ok((self)(args))
    }
}

// implement [`SyscallBuilder`] for functions with different amount of arguments
macro_rules! impl_syscall_builder {
    ($($generic:ident),+) => {
        #[allow(non_snake_case)]
        impl<Ext, Res, Call, Builder, $($generic),+> SyscallBuilder<Ext, ($($generic,)+), Res, Call>
            for Builder
        where
            Builder: FnOnce($($generic),+) -> Call,
            Call: Syscall<Ext, Res>,
            $( $generic: TryFrom<SyscallValue, Error = HostError>,)+
        {
            fn build(self, args: &[Value]) -> Result<Call, HostError> {
                const ARGS_AMOUNT: usize = impl_syscall_builder!(@count $($generic),+);

                let [$($generic),+]: [Value; ARGS_AMOUNT] = args.try_into().map_err(|_| HostError)?;
                $(
                    let $generic = SyscallValue($generic).try_into()?;
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

/// "raw" syscall without any argument parsing or without calling [`CallerWrap`] helper methods
struct RawSyscall<F>(F);

impl<F> RawSyscall<F> {
    fn new(f: F) -> Self {
        Self(f)
    }
}

impl<T, F, Ext> Syscall<Ext, T> for RawSyscall<F>
where
    F: FnOnce(&mut CallerWrap<Ext>) -> Result<(Gas, T), HostError>,
    Ext: BackendExternalities + 'static,
{
    type Context = ();

    fn execute(
        self,
        caller: &mut CallerWrap<Ext>,
        (): Self::Context,
    ) -> Result<(Gas, T), HostError> {
        (self.0)(caller)
    }
}

/// Fallible syscall context that parses `gas` and `err_ptr` arguments.
struct FallibleSyscallContext {
    gas: Gas,
    res_ptr: u32,
}

impl SyscallContext for FallibleSyscallContext {
    fn from_args(args: &[Value]) -> Result<(Self, &[Value]), HostError> {
        let (gas, args) = args.split_first().ok_or(HostError)?;
        let gas: Gas = SyscallValue(*gas).try_into()?;
        let (res_ptr, args) = args.split_last().ok_or(HostError)?;
        let res_ptr: u32 = SyscallValue(*res_ptr).try_into()?;
        Ok((FallibleSyscallContext { gas, res_ptr }, args))
    }
}

/// Fallible syscall that calls [`CallerWrap::run_fallible`] underneath.
struct FallibleSyscall<E, F> {
    token: CostToken,
    error: PhantomData<E>,
    f: F,
}

impl<F> FallibleSyscall<(), F> {
    fn new<E>(token: CostToken, f: F) -> FallibleSyscall<E, F> {
        FallibleSyscall {
            token,
            error: PhantomData,
            f,
        }
    }
}

impl<T, E, F, Ext> Syscall<Ext, ()> for FallibleSyscall<E, F>
where
    F: FnOnce(&mut CallerWrap<Ext>) -> Result<T, RunFallibleError>,
    E: From<Result<T, u32>>,
    Ext: BackendExternalities + 'static,
{
    type Context = FallibleSyscallContext;

    fn execute(
        self,
        caller: &mut CallerWrap<Ext>,
        context: Self::Context,
    ) -> Result<(Gas, ()), HostError> {
        let Self { token, f, .. } = self;
        let FallibleSyscallContext { gas, res_ptr } = context;
        caller.run_fallible::<T, _, E>(gas, res_ptr, token, f)
    }
}

/// Infallible syscall context that parses `gas` argument.
pub struct InfallibleSyscallContext {
    gas: Gas,
}

impl SyscallContext for InfallibleSyscallContext {
    fn from_args(args: &[Value]) -> Result<(Self, &[Value]), HostError> {
        let (gas, args) = args.split_first().ok_or(HostError)?;
        let gas: Gas = SyscallValue(*gas).try_into()?;
        Ok((Self { gas }, args))
    }
}

/// Infallible syscall that calls [`CallerWrap::run_any`] underneath
struct InfallibleSyscall<F> {
    token: CostToken,
    f: F,
}

impl<F> InfallibleSyscall<F> {
    fn new(token: CostToken, f: F) -> Self {
        Self { token, f }
    }
}

impl<T, F, Ext> Syscall<Ext, T> for InfallibleSyscall<F>
where
    F: Fn(&mut CallerWrap<Ext>) -> Result<T, UndefinedTerminationReason>,
    Ext: BackendExternalities + 'static,
{
    type Context = InfallibleSyscallContext;

    fn execute(
        self,
        caller: &mut CallerWrap<Ext>,
        ctx: Self::Context,
    ) -> Result<(Gas, T), HostError> {
        let Self { token, f } = self;
        let InfallibleSyscallContext { gas } = ctx;
        caller.run_any::<T, _>(gas, token, f)
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
    pub fn execute<Builder, Args, Res, Call>(
        caller: &mut Caller<HostState<Ext, ExecutorMemory>>,
        args: &[Value],
        builder: Builder,
    ) -> Result<WasmReturnValue, HostError>
    where
        Builder: SyscallBuilder<Ext, Args, Res, Call>,
        Args: ?Sized,
        Call: Syscall<Ext, Res>,
        Res: Into<SyscallReturnValue>,
    {
        crate::log::trace_syscall::<Builder>(args);

        let mut caller = CallerWrap::prepare(caller);

        let (ctx, args) = Call::Context::from_args(args)?;
        let syscall = builder.build(args)?;
        let (gas, value) = syscall.execute(&mut caller, ctx)?;
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

    pub fn send(pid_value_ptr: u32, payload_ptr: u32, len: u32, delay: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::Send(len.into()),
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
    ) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::SendWGas(len.into()),
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

    pub fn send_commit(handle: u32, pid_value_ptr: u32, delay: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::SendCommit,
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
    ) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::SendCommitWGas,
            move |ctx: &mut CallerWrap<Ext>| {
                Self::send_commit_inner(ctx, handle, pid_value_ptr, Some(gas_limit), delay)
            },
        )
    }

    pub fn send_init() -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHandle>(
            CostToken::SendInit,
            move |ctx: &mut CallerWrap<Ext>| ctx.ext_mut().send_init().map_err(Into::into),
        )
    }

    pub fn send_push(handle: u32, payload_ptr: u32, len: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorBytes>(
            CostToken::SendPush(len.into()),
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
    ) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReservationSend(len.into()),
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
    ) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReservationSendCommit,
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

    pub fn read(at: u32, len: u32, buffer_ptr: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorBytes>(CostToken::Read, move |ctx: &mut CallerWrap<Ext>| {
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
        })
    }

    pub fn size(size_ptr: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::Size, move |ctx: &mut CallerWrap<Ext>| {
            let size = ctx.ext_mut().size()? as u32;

            let write_size = ctx.manager.register_write_as(size_ptr);
            ctx.write_as(write_size, size.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn exit(inheritor_id_ptr: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::Exit, move |ctx: &mut CallerWrap<Ext>| {
            let read_inheritor_id = ctx.manager.register_read_decoded(inheritor_id_ptr);
            let inheritor_id = ctx.read_decoded(read_inheritor_id)?;
            Err(ActorTerminationReason::Exit(inheritor_id).into())
        })
    }

    pub fn reply_code() -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithReplyCode>(
            CostToken::ReplyCode,
            move |ctx: &mut CallerWrap<Ext>| {
                ctx.ext_mut()
                    .reply_code()
                    .map(ReplyCode::to_bytes)
                    .map_err(Into::into)
            },
        )
    }

    pub fn signal_code() -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithSignalCode>(
            CostToken::SignalCode,
            move |ctx: &mut CallerWrap<Ext>| {
                ctx.ext_mut()
                    .signal_code()
                    .map(SignalCode::to_u32)
                    .map_err(Into::into)
            },
        )
    }

    pub fn alloc(pages: u32) -> impl Syscall<Ext, u32> {
        InfallibleSyscall::new(CostToken::Alloc, move |ctx: &mut CallerWrap<Ext>| {
            let res = ctx.alloc(pages);
            let res = ctx.process_alloc_func_result(res)?;

            let page = match res {
                Ok(page) => {
                    log::trace!("Alloc {pages:?} pages at {page:?}");
                    page.into()
                }
                Err(err) => {
                    log::trace!("Alloc failed: {err}");
                    u32::MAX
                }
            };
            Ok(page)
        })
    }

    pub fn free(page_no: u32) -> impl Syscall<Ext, i32> {
        InfallibleSyscall::new(CostToken::Free, move |ctx: &mut CallerWrap<Ext>| {
            let page = WasmPage::try_from(page_no).map_err(|_| {
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

    pub fn free_range(start: u32, end: u32) -> impl Syscall<Ext, i32> {
        InfallibleSyscall::new(CostToken::FreeRange, move |ctx: &mut CallerWrap<Ext>| {
            let page_err = |_| {
                UndefinedTerminationReason::Actor(ActorTerminationReason::Trap(
                    TrapExplanation::Unknown,
                ))
            };

            let start = WasmPage::try_from(start).map_err(page_err)?;
            let end = WasmPage::try_from(end).map_err(page_err)?;

            let result = ctx.ext_mut().free_range(start, end);

            match ctx.process_alloc_func_result(result)? {
                Ok(()) => {
                    log::trace!("Free range {start:?}:{end:?} success");
                    Ok(0)
                }
                Err(e) => {
                    log::trace!("Free range {start:?}:{end:?} failed: {e}");
                    Ok(1)
                }
            }
        })
    }

    pub fn env_vars(vars_ver: u32, vars_ptr: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::EnvVars, move |ctx: &mut CallerWrap<Ext>| {
            let vars = ctx.ext_mut().env_vars(vars_ver)?;
            let vars_bytes = vars.to_bytes();
            let vars_write = ctx
                .manager
                .register_write(vars_ptr, vars_bytes.len() as u32);
            ctx.write(vars_write, vars_bytes).map_err(Into::into)
        })
    }

    pub fn block_height(height_ptr: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::BlockHeight, move |ctx: &mut CallerWrap<Ext>| {
            let height = ctx.ext_mut().block_height()?;

            let write_height = ctx.manager.register_write_as(height_ptr);
            ctx.write_as(write_height, height.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn block_timestamp(timestamp_ptr: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(
            CostToken::BlockTimestamp,
            move |ctx: &mut CallerWrap<Ext>| {
                let timestamp = ctx.ext_mut().block_timestamp()?;

                let write_timestamp = ctx.manager.register_write_as(timestamp_ptr);
                ctx.write_as(write_timestamp, timestamp.to_le_bytes())
                    .map_err(Into::into)
            },
        )
    }

    pub fn random(subject_ptr: u32, bn_random_ptr: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::Random, move |ctx: &mut CallerWrap<Ext>| {
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

    pub fn reply(payload_ptr: u32, len: u32, value_ptr: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::Reply(len.into()),
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
    ) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReplyWGas(len.into()),
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

    pub fn reply_commit(value_ptr: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReplyCommit,
            move |ctx: &mut CallerWrap<Ext>| Self::reply_commit_inner(ctx, None, value_ptr),
        )
    }

    pub fn reply_commit_wgas(gas_limit: u64, value_ptr: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReplyCommitWGas,
            move |ctx: &mut CallerWrap<Ext>| {
                Self::reply_commit_inner(ctx, Some(gas_limit), value_ptr)
            },
        )
    }

    pub fn reservation_reply(rid_value_ptr: u32, payload_ptr: u32, len: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReservationReply(len.into()),
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

    pub fn reservation_reply_commit(rid_value_ptr: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReservationReplyCommit,
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

    pub fn reply_to() -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReplyTo,
            move |ctx: &mut CallerWrap<Ext>| ctx.ext_mut().reply_to().map_err(Into::into),
        )
    }

    pub fn signal_from() -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::SignalFrom,
            move |ctx: &mut CallerWrap<Ext>| ctx.ext_mut().signal_from().map_err(Into::into),
        )
    }

    pub fn reply_push(payload_ptr: u32, len: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorBytes>(
            CostToken::ReplyPush(len.into()),
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

    pub fn reply_input(offset: u32, len: u32, value_ptr: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReplyInput,
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
    ) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReplyInputWGas,
            move |ctx: &mut CallerWrap<Ext>| {
                Self::reply_input_inner(ctx, offset, len, Some(gas_limit), value_ptr)
            },
        )
    }

    pub fn reply_push_input(offset: u32, len: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorBytes>(
            CostToken::ReplyPushInput,
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

    pub fn send_input(pid_value_ptr: u32, offset: u32, len: u32, delay: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::SendInput,
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
    ) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::SendInputWGas,
            move |ctx: &mut CallerWrap<Ext>| {
                Self::send_input_inner(ctx, pid_value_ptr, offset, len, Some(gas_limit), delay)
            },
        )
    }

    pub fn send_push_input(handle: u32, offset: u32, len: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorBytes>(
            CostToken::SendPushInput,
            move |ctx: &mut CallerWrap<Ext>| {
                ctx.ext_mut()
                    .send_push_input(handle, offset, len)
                    .map_err(Into::into)
            },
        )
    }

    pub fn debug(data_ptr: u32, data_len: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(
            CostToken::Debug(data_len.into()),
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

    pub fn panic(data_ptr: u32, data_len: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::Null, move |ctx: &mut CallerWrap<Ext>| {
            let read_data = ctx.manager.register_read(data_ptr, data_len);
            let data = ctx.read(read_data).unwrap_or_default();

            let s = String::from_utf8_lossy(&data).to_string();

            Err(ActorTerminationReason::Trap(TrapExplanation::Panic(s.into())).into())
        })
    }

    pub fn oom_panic() -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::Null, |_ctx: &mut CallerWrap<Ext>| {
            Err(ActorTerminationReason::Trap(TrapExplanation::ProgramAllocOutOfBounds).into())
        })
    }

    pub fn reserve_gas(gas_value: u64, duration: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReserveGas,
            move |ctx: &mut CallerWrap<Ext>| {
                ctx.ext_mut()
                    .reserve_gas(gas_value, duration)
                    .map_err(Into::into)
            },
        )
    }

    pub fn reply_deposit(message_id_ptr: u32, gas_value: u64) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorBytes>(
            CostToken::ReplyDeposit,
            move |ctx: &mut CallerWrap<Ext>| {
                let read_message_id = ctx.manager.register_read_decoded(message_id_ptr);
                let message_id = ctx.read_decoded(read_message_id)?;

                ctx.ext_mut()
                    .reply_deposit(message_id, gas_value)
                    .map_err(Into::into)
            },
        )
    }

    pub fn unreserve_gas(reservation_id_ptr: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithGas>(
            CostToken::UnreserveGas,
            move |ctx: &mut CallerWrap<Ext>| {
                let read_reservation_id = ctx.manager.register_read_decoded(reservation_id_ptr);
                let reservation_id = ctx.read_decoded(read_reservation_id)?;

                ctx.ext_mut()
                    .unreserve_gas(reservation_id)
                    .map_err(Into::into)
            },
        )
    }

    pub fn system_reserve_gas(gas_value: u64) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorBytes>(
            CostToken::SystemReserveGas,
            move |ctx: &mut CallerWrap<Ext>| {
                ctx.ext_mut()
                    .system_reserve_gas(gas_value)
                    .map_err(Into::into)
            },
        )
    }

    pub fn gas_available(gas_ptr: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::GasAvailable, move |ctx: &mut CallerWrap<Ext>| {
            let gas_available = ctx.ext_mut().gas_available()?;

            let write_gas = ctx.manager.register_write_as(gas_ptr);
            ctx.write_as(write_gas, gas_available.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn message_id(message_id_ptr: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::MsgId, move |ctx: &mut CallerWrap<Ext>| {
            let message_id = ctx.ext_mut().message_id()?;

            let write_message_id = ctx.manager.register_write_as(message_id_ptr);
            ctx.write_as(write_message_id, message_id.into_bytes())
                .map_err(Into::into)
        })
    }

    pub fn program_id(program_id_ptr: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::ProgramId, move |ctx: &mut CallerWrap<Ext>| {
            let program_id = ctx.ext_mut().program_id()?;

            let write_program_id = ctx.manager.register_write_as(program_id_ptr);
            ctx.write_as(write_program_id, program_id.into_bytes())
                .map_err(Into::into)
        })
    }

    pub fn source(source_ptr: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::Source, move |ctx: &mut CallerWrap<Ext>| {
            let source = ctx.ext_mut().source()?;

            let write_source = ctx.manager.register_write_as(source_ptr);
            ctx.write_as(write_source, source.into_bytes())
                .map_err(Into::into)
        })
    }

    pub fn value(value_ptr: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::Value, move |ctx: &mut CallerWrap<Ext>| {
            let value = ctx.ext_mut().value()?;

            let write_value = ctx.manager.register_write_as(value_ptr);
            ctx.write_as(write_value, value.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn value_available(value_ptr: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(
            CostToken::ValueAvailable,
            move |ctx: &mut CallerWrap<Ext>| {
                let value_available = ctx.ext_mut().value_available()?;

                let write_value = ctx.manager.register_write_as(value_ptr);
                ctx.write_as(write_value, value_available.to_le_bytes())
                    .map_err(Into::into)
            },
        )
    }

    pub fn leave() -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::Leave, move |_ctx: &mut CallerWrap<Ext>| {
            Err(ActorTerminationReason::Leave.into())
        })
    }

    pub fn wait() -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::Wait, move |ctx: &mut CallerWrap<Ext>| {
            ctx.ext_mut().wait()?;
            Err(ActorTerminationReason::Wait(None, MessageWaitedType::Wait).into())
        })
    }

    pub fn wait_for(duration: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::WaitFor, move |ctx: &mut CallerWrap<Ext>| {
            ctx.ext_mut().wait_for(duration)?;
            Err(ActorTerminationReason::Wait(Some(duration), MessageWaitedType::WaitFor).into())
        })
    }

    pub fn wait_up_to(duration: u32) -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::WaitUpTo, move |ctx: &mut CallerWrap<Ext>| {
            let waited_type = if ctx.ext_mut().wait_up_to(duration)? {
                MessageWaitedType::WaitUpToFull
            } else {
                MessageWaitedType::WaitUpTo
            };
            Err(ActorTerminationReason::Wait(Some(duration), waited_type).into())
        })
    }

    pub fn wake(message_id_ptr: u32, delay: u32) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorBytes>(CostToken::Wake, move |ctx: &mut CallerWrap<Ext>| {
            let read_message_id = ctx.manager.register_read_decoded(message_id_ptr);
            let message_id = ctx.read_decoded(read_message_id)?;

            ctx.ext_mut().wake(message_id, delay).map_err(Into::into)
        })
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
    ) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithTwoHashes>(
            CostToken::CreateProgram(payload_len.into(), salt_len.into()),
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
    ) -> impl Syscall<Ext> {
        FallibleSyscall::new::<ErrorWithTwoHashes>(
            CostToken::CreateProgramWGas(payload_len.into(), salt_len.into()),
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

    pub fn forbidden(_args: &[Value]) -> impl Syscall<Ext> {
        InfallibleSyscall::new(CostToken::Null, |_: &mut CallerWrap<Ext>| {
            Err(ActorTerminationReason::Trap(TrapExplanation::ForbiddenFunction).into())
        })
    }

    fn out_of_gas(ctx: &mut CallerWrap<Ext>) -> UndefinedTerminationReason {
        let ext = ctx.ext_mut();
        let current_counter = ext.current_counter_type();
        log::trace!(target: "syscalls", "system_break(OutOfGas): Current counter in global represents {current_counter:?}");

        if current_counter == CounterType::GasAllowance {
            // We manually decrease it to 0 because global won't be affected
            // since it didn't pass comparison to argument of `gas_charge()`
            ext.decrease_current_counter_to(0);
        }

        ActorTerminationReason::from(current_counter).into()
    }

    fn stack_limit_exceeded() -> UndefinedTerminationReason {
        TrapExplanation::StackLimitExceeded.into()
    }

    pub fn system_break(_gas: Gas, code: u32) -> impl Syscall<Ext> {
        RawSyscall::new(move |ctx: &mut CallerWrap<Ext>| {
            // At the instrumentation level, we can only use variants of the `SystemBreakCode`,
            // so we should never reach `unreachable!("{e}")`.
            let termination_reason = SystemBreakCode::try_from(code)
                .map(|system_break_code| match system_break_code {
                    SystemBreakCode::OutOfGas => Self::out_of_gas(ctx),
                    SystemBreakCode::StackLimitExceeded => Self::stack_limit_exceeded(),
                })
                .unwrap_or_else(|e| unreachable!("{e}"));
            ctx.set_termination_reason(termination_reason);
            Err(HostError)
        })
    }
}
