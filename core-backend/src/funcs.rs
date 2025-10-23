// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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
    BackendExternalities,
    accessors::{
        Read, ReadAs, ReadAsOption, ReadPayloadLimited, SyscallArg, SyscallValue, WriteAs,
        WriteInGrRead,
    },
    error::{
        ActorTerminationReason, BackendAllocSyscallError, BackendSyscallError, RunFallibleError,
        TrapExplanation, UndefinedTerminationReason, UnrecoverableExecutionError,
        UnrecoverableMemoryError,
    },
    memory::{BackendMemory, ExecutorMemory, MemoryAccessRegistry},
    runtime::MemoryCallerContext,
    state::HostState,
};
use alloc::{format, string::String};
use blake2::{Blake2b, Digest, digest::typenum::U32};
use bytemuck::Pod;
use core::marker::PhantomData;
use gear_core::{
    buffer::{Payload, RuntimeBuffer},
    costs::CostToken,
    env::MessageWaitedType,
    gas::CounterType,
    ids::{ActorId, MessageId, ReservationId},
    limited::LimitedVecError,
    message::{HandlePacket, InitPacket, ReplyPacket},
    pages::WasmPage,
};
use gear_core_errors::{MessageError, ReplyCode, SignalCode};
use gear_sandbox::{AsContextExt, ReturnValue, Value};
use gear_sandbox_env::{HostError, WasmReturnValue};
use gear_wasm_instrument::{SyscallName, SystemBreakCode};
use gsys::{
    BlockNumberWithHash, ErrorBytes, ErrorWithGas, ErrorWithHandle, ErrorWithHash,
    ErrorWithReplyCode, ErrorWithSignalCode, ErrorWithTwoHashes, Gas, Hash, HashWithValue,
    TwoHashesWithValue,
};

/// BLAKE2b-256 hasher state.
type Blake2b256 = Blake2b<U32>;

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

pub(crate) trait SyscallContext: Sized + Copy {
    fn from_args(args: &[Value]) -> Result<(Self, Gas, &[Value]), HostError>;
}

pub(crate) trait Syscall<Caller, T = ()> {
    type Context: SyscallContext;

    fn execute(
        self,
        caller: &mut MemoryCallerContext<Caller>,
        ctx: Self::Context,
        syscall_name: SyscallName,
    ) -> Result<(Gas, T), HostError>;
}

/// Trait is implemented for functions.
///
/// # Generics
/// `Args` is to make specialization based on function arguments
/// `Ext` and `Res` are for syscall itself (`Syscall<Ext, Res>`)
pub(crate) trait SyscallBuilder<Caller, Args: ?Sized, Res, Call>
where
    Call: Syscall<Caller, Res>,
{
    fn build(
        self,
        ctx: &mut MemoryCallerContext<Caller>,
        args: &[Value],
    ) -> Result<Call, HostError>;
}

impl<Caller, Res, Call, Builder> SyscallBuilder<Caller, (), Res, Call> for Builder
where
    Builder: FnOnce() -> Call,
    Call: Syscall<Caller, Res>,
{
    fn build(self, _: &mut MemoryCallerContext<Caller>, args: &[Value]) -> Result<Call, HostError> {
        let _: [Value; 0] = args.try_into().map_err(|_| HostError)?;
        Ok((self)())
    }
}

impl<Caller, Res, Call, Builder> SyscallBuilder<Caller, [Value], Res, Call> for Builder
where
    Builder: for<'a> FnOnce(&'a [Value]) -> Call,
    Call: Syscall<Caller, Res>,
{
    fn build(self, _: &mut MemoryCallerContext<Caller>, args: &[Value]) -> Result<Call, HostError> {
        Ok((self)(args))
    }
}

// implement [`SyscallBuilder`] for functions with different amount of arguments
macro_rules! impl_syscall_builder {
    ($($generic:ident),+) => {
        #[allow(non_snake_case)]
        impl<Caller, Ext, Res, Call, Builder, $($generic),+> SyscallBuilder<Caller, ($($generic,)+), Res, Call>
            for Builder
        where
            Builder: FnOnce($($generic),+) -> Call,
            Call: Syscall<Caller, Res>,
            Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
            Ext: BackendExternalities + 'static,
            $( $generic: SyscallArg, )+
        {
            fn build(self, ctx: &mut MemoryCallerContext<Caller>, args: &[Value]) -> Result<Call, HostError> {
                let ARGS_AMOUNT: usize = 0 $( + $generic::REQUIRED_ARGS )+;

                if args.len() != ARGS_AMOUNT {
                    return Err(HostError);
                }

                let mut registry: Option<MemoryAccessRegistry<Caller>> = None;

                let mut index = 0;
                $(
                    let args_count = $generic::REQUIRED_ARGS;
                    let args_slice = &args[index..index + args_count];
                    let $generic = $generic::pre_process(&mut registry, args_slice)?;
                    index += args_count;
                )+

                debug_assert_eq!(index, ARGS_AMOUNT);

                if let Some(registry) = registry {
                    let io = registry.pre_process(&mut ctx.caller_wrap);
                    ctx.memory_wrap.set_io(io);
                }

                $(
                    let $generic = $generic::post_process($generic, ctx);
                )+

                Ok(((self)($($generic),+)))
            }
        }
    };
}

impl_syscall_builder!(A);
impl_syscall_builder!(A, B);
impl_syscall_builder!(A, B, C);
impl_syscall_builder!(A, B, C, D);
impl_syscall_builder!(A, B, C, D, E);
impl_syscall_builder!(A, B, C, D, E, F);
impl_syscall_builder!(A, B, C, D, E, F, G);

/// Fallible syscall context that parses `gas` and `err_ptr` arguments.
#[derive(Copy, Clone)]
struct FallibleSyscallContext {
    res_ptr: u32,
}

impl SyscallContext for FallibleSyscallContext {
    fn from_args(args: &[Value]) -> Result<(Self, Gas, &[Value]), HostError> {
        let (gas, args) = args.split_first().ok_or(HostError)?;
        let gas: Gas = SyscallValue(*gas).try_into()?;
        let (res_ptr, args) = args.split_last().ok_or(HostError)?;
        let res_ptr: u32 = SyscallValue(*res_ptr).try_into()?;
        Ok((FallibleSyscallContext { res_ptr }, gas, args))
    }
}

/// Fallible syscall that calls [`MemoryCallerContext::run_fallible`] underneath.
#[derive(Copy, Clone)]
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

impl<T, E, F, Caller, Ext> Syscall<Caller, ()> for FallibleSyscall<E, F>
where
    F: FnOnce(&mut MemoryCallerContext<Caller>) -> Result<T, RunFallibleError>,
    E: From<Result<T, u32>> + Pod,
    Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
    Ext: BackendExternalities + 'static,
{
    type Context = FallibleSyscallContext;

    fn execute(
        self,
        caller: &mut MemoryCallerContext<Caller>,
        context: Self::Context,
        syscall_name: SyscallName,
    ) -> Result<(Gas, ()), HostError> {
        let Self { token, f, .. } = self;
        let FallibleSyscallContext { res_ptr } = context;
        caller.check_func_forbiddenness(syscall_name)?;
        caller.run_fallible::<T, _, E>(res_ptr, token, f)
    }
}

/// Infallible syscall context that parses `gas` argument.
#[derive(Copy, Clone)]
pub struct InfallibleSyscallContext;

impl SyscallContext for InfallibleSyscallContext {
    fn from_args(args: &[Value]) -> Result<(Self, Gas, &[Value]), HostError> {
        let (gas, args) = args.split_first().ok_or(HostError)?;
        let gas: Gas = SyscallValue(*gas).try_into()?;
        Ok((Self, gas, args))
    }
}

/// Infallible syscall that calls [`MemoryCallerContext::run_any`] underneath
#[derive(Copy, Clone)]
struct InfallibleSyscall<F> {
    token: CostToken,
    f: F,
}

impl<F> InfallibleSyscall<F> {
    fn new(token: CostToken, f: F) -> Self {
        Self { token, f }
    }
}

impl<T, F, Caller, Ext> Syscall<Caller, T> for InfallibleSyscall<F>
where
    F: FnOnce(&mut MemoryCallerContext<Caller>) -> Result<T, UndefinedTerminationReason>,
    Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
    Ext: BackendExternalities + 'static,
{
    type Context = InfallibleSyscallContext;

    fn execute(
        self,
        caller: &mut MemoryCallerContext<Caller>,
        _ctx: Self::Context,
        syscall_name: SyscallName,
    ) -> Result<(Gas, T), HostError> {
        let Self { token, f } = self;
        caller.check_func_forbiddenness(syscall_name)?;
        caller.run_any::<T, _>(token, f)
    }
}

pub(crate) struct FuncsHandler<Caller> {
    _phantom: PhantomData<Caller>,
}

impl<Caller, Ext> FuncsHandler<Caller>
where
    Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
    Ext: BackendExternalities + 'static,
    Ext::UnrecoverableError: BackendSyscallError,
    RunFallibleError: From<Ext::FallibleError>,
    Ext::AllocError: BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
{
    pub fn execute<Builder, Args, Res, Call>(
        caller: &mut Caller,
        args: &[Value],
        builder: Builder,
        syscall_name: SyscallName,
    ) -> Result<WasmReturnValue, HostError>
    where
        Builder: SyscallBuilder<Caller, Args, Res, Call>,
        Args: ?Sized,
        Call: Syscall<Caller, Res>,
        Res: Into<SyscallReturnValue>,
    {
        crate::log::trace_syscall::<Builder>(args);

        let mut memory_caller_context = MemoryCallerContext::new(caller);

        let (ctx, gas, args) = Call::Context::from_args(args)?;

        memory_caller_context
            .caller_wrap
            .state_mut()
            .ext
            .decrease_current_counter_to(gas);

        let syscall = builder.build(&mut memory_caller_context, args)?;

        let (gas, value) = syscall.execute(&mut memory_caller_context, ctx, syscall_name)?;

        let value = value.into();

        Ok(WasmReturnValue {
            gas: gas as i64,
            inner: value.0,
        })
    }

    fn read_payload(payload: ReadPayloadLimited) -> Result<Payload, RunFallibleError> {
        payload
            .into_inner()
            .map_err(|_| MessageError::MaxMessageSizeExceed.into())
            .map_err(RunFallibleError::FallibleExt)?
            .map_err(|e| e.into_run_fallible_error())
    }

    fn send_inner(
        ctx: &mut MemoryCallerContext<Caller>,
        pid_value: ReadAs<HashWithValue>,
        payload: ReadPayloadLimited,
        gas_limit: Option<u64>,
        delay: u32,
    ) -> Result<MessageId, RunFallibleError> {
        let HashWithValue {
            hash: destination,
            value,
        } = pid_value.into_inner()?;

        let payload = Self::read_payload(payload)?;

        ctx.caller_wrap
            .ext_mut()
            .send(
                HandlePacket::maybe_with_gas(destination.into(), payload, gas_limit, value),
                delay,
            )
            .map_err(Into::into)
    }

    pub fn send(
        pid_value: ReadAs<HashWithValue>,
        payload: ReadPayloadLimited,
        delay: u32,
    ) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::Send(payload.size().into()),
            move |ctx: &mut MemoryCallerContext<Caller>| {
                Self::send_inner(ctx, pid_value, payload, None, delay)
            },
        )
    }

    pub fn send_wgas(
        pid_value: ReadAs<HashWithValue>,
        payload: ReadPayloadLimited,
        gas_limit: u64,
        delay: u32,
    ) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::SendWGas(payload.size().into()),
            move |ctx: &mut MemoryCallerContext<Caller>| {
                Self::send_inner(ctx, pid_value, payload, Some(gas_limit), delay)
            },
        )
    }

    fn send_commit_inner(
        ctx: &mut MemoryCallerContext<Caller>,
        handle: u32,
        pid_value: ReadAs<HashWithValue>,
        gas_limit: Option<u64>,
        delay: u32,
    ) -> Result<MessageId, RunFallibleError> {
        let HashWithValue {
            hash: destination,
            value,
        } = pid_value.into_inner()?;

        ctx.caller_wrap
            .ext_mut()
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

    pub fn send_commit(
        handle: u32,
        pid_value: ReadAs<HashWithValue>,
        delay: u32,
    ) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::SendCommit,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                Self::send_commit_inner(ctx, handle, pid_value, None, delay)
            },
        )
    }

    pub fn send_commit_wgas(
        handle: u32,
        pid_value: ReadAs<HashWithValue>,
        gas_limit: u64,
        delay: u32,
    ) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::SendCommitWGas,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                Self::send_commit_inner(ctx, handle, pid_value, Some(gas_limit), delay)
            },
        )
    }

    pub fn send_init() -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHandle>(
            CostToken::SendInit,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                ctx.caller_wrap.ext_mut().send_init().map_err(Into::into)
            },
        )
    }

    pub fn send_push(handle: u32, payload: Read) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorBytes>(
            CostToken::SendPush(payload.size().into()),
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let payload = payload.into_inner()?;

                ctx.caller_wrap
                    .ext_mut()
                    .send_push(handle, &payload)
                    .map_err(Into::into)
            },
        )
    }

    pub fn reservation_send(
        rid_pid_value: ReadAs<TwoHashesWithValue>,
        payload: ReadPayloadLimited,
        delay: u32,
    ) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReservationSend(payload.size().into()),
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let TwoHashesWithValue {
                    hash1: reservation_id,
                    hash2: destination,
                    value,
                } = rid_pid_value.into_inner()?;

                let payload = Self::read_payload(payload)?;

                ctx.caller_wrap
                    .ext_mut()
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
        rid_pid_value: ReadAs<TwoHashesWithValue>,
        delay: u32,
    ) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReservationSendCommit,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let TwoHashesWithValue {
                    hash1: reservation_id,
                    hash2: destination,
                    value,
                } = rid_pid_value.into_inner()?;

                ctx.caller_wrap
                    .ext_mut()
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

    pub fn read(at: u32, buffer: WriteInGrRead) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorBytes>(
            CostToken::Read,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let payload = ctx.caller_wrap.ext_mut().payload_slice(at, buffer.size())?;
                buffer.write(ctx, payload.slice()).map_err(Into::into)
            },
        )
    }

    pub fn size(size_write: WriteAs<u32>) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::Size,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let size = ctx.caller_wrap.ext_mut().size()? as u32;

                size_write.write(ctx, &size).map_err(Into::into)
            },
        )
    }

    pub fn exit(inheritor_id: ReadAs<ActorId>) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::Exit,
            move |_ctx: &mut MemoryCallerContext<Caller>| {
                let inheritor_id = inheritor_id.into_inner()?;
                Err(ActorTerminationReason::Exit(inheritor_id).into())
            },
        )
    }

    pub fn reply_code() -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithReplyCode>(
            CostToken::ReplyCode,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                ctx.caller_wrap
                    .ext_mut()
                    .reply_code()
                    .map(ReplyCode::to_bytes)
                    .map_err(Into::into)
            },
        )
    }

    pub fn signal_code() -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithSignalCode>(
            CostToken::SignalCode,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                ctx.caller_wrap
                    .ext_mut()
                    .signal_code()
                    .map(SignalCode::to_u32)
                    .map_err(Into::into)
            },
        )
    }

    pub fn alloc(pages: u32) -> impl Syscall<Caller, u32> {
        InfallibleSyscall::new(
            CostToken::Alloc,
            move |ctx: &mut MemoryCallerContext<Caller>| {
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
            },
        )
    }

    pub fn free(page_no: u32) -> impl Syscall<Caller, i32> {
        InfallibleSyscall::new(
            CostToken::Free,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let page = WasmPage::try_from(page_no).map_err(|_| {
                    UndefinedTerminationReason::Actor(ActorTerminationReason::Trap(
                        TrapExplanation::Unknown,
                    ))
                })?;

                let res = ctx.caller_wrap.ext_mut().free(page);
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
            },
        )
    }

    pub fn free_range(start: u32, end: u32) -> impl Syscall<Caller, i32> {
        InfallibleSyscall::new(
            CostToken::FreeRange,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let page_err = |_| {
                    UndefinedTerminationReason::Actor(ActorTerminationReason::Trap(
                        TrapExplanation::Unknown,
                    ))
                };

                let start = WasmPage::try_from(start).map_err(page_err)?;
                let end = WasmPage::try_from(end).map_err(page_err)?;

                let result = ctx.caller_wrap.ext_mut().free_range(start, end);

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
            },
        )
    }

    pub fn env_vars(vars_ver: u32, vars_ptr: u32) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::EnvVars,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let vars = ctx.caller_wrap.ext_mut().env_vars(vars_ver)?;
                let vars_bytes = vars.to_bytes();

                let mut registry = MemoryAccessRegistry::default();
                let vars_write = registry.register_write(vars_ptr, vars_bytes.len() as u32);
                let mut io = registry.pre_process(&mut ctx.caller_wrap)?;
                io.write(&mut ctx.caller_wrap, vars_write, vars_bytes)
                    .map_err(Into::into)
            },
        )
    }

    pub fn block_height(height_write: WriteAs<u32>) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::BlockHeight,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let height = ctx.caller_wrap.ext_mut().block_height()?;

                height_write.write(ctx, &height).map_err(Into::into)
            },
        )
    }

    pub fn block_timestamp(timestamp_write: WriteAs<u64>) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::BlockTimestamp,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let timestamp = ctx.caller_wrap.ext_mut().block_timestamp()?;

                timestamp_write.write(ctx, &timestamp).map_err(Into::into)
            },
        )
    }

    pub fn random(
        subject_ptr: ReadAs<Hash>,
        bn_random_ptr: WriteAs<BlockNumberWithHash>,
    ) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::Random,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let raw_subject = subject_ptr.into_inner()?;
                let (random, bn) = ctx.caller_wrap.ext_mut().random()?;
                let subject = [&raw_subject, random].concat();

                let mut blake2_ctx = Blake2b256::new();
                blake2_ctx.update(subject);
                let hash = blake2_ctx.finalize().into();

                bn_random_ptr
                    .write(ctx, &BlockNumberWithHash { bn, hash })
                    .map_err(Into::into)
            },
        )
    }

    fn reply_inner(
        ctx: &mut MemoryCallerContext<Caller>,
        payload: ReadPayloadLimited,
        gas_limit: Option<u64>,
        value: ReadAsOption<u128>,
    ) -> Result<MessageId, RunFallibleError> {
        let value = value.into_inner()?.unwrap_or(0);
        let payload = Self::read_payload(payload)?;

        ctx.caller_wrap
            .ext_mut()
            .reply(ReplyPacket::maybe_with_gas(payload, gas_limit, value))
            .map_err(Into::into)
    }

    pub fn reply(payload: ReadPayloadLimited, value: ReadAsOption<u128>) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::Reply(payload.size().into()),
            move |ctx: &mut MemoryCallerContext<Caller>| {
                Self::reply_inner(ctx, payload, None, value)
            },
        )
    }

    pub fn reply_wgas(
        payload: ReadPayloadLimited,
        gas_limit: u64,
        value: ReadAsOption<u128>,
    ) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReplyWGas(payload.size().into()),
            move |ctx: &mut MemoryCallerContext<Caller>| {
                Self::reply_inner(ctx, payload, Some(gas_limit), value)
            },
        )
    }

    fn reply_commit_inner(
        ctx: &mut MemoryCallerContext<Caller>,
        gas_limit: Option<u64>,
        value: ReadAsOption<u128>,
    ) -> Result<MessageId, RunFallibleError> {
        let value = value.into_inner()?.unwrap_or(0);

        ctx.caller_wrap
            .ext_mut()
            .reply_commit(ReplyPacket::maybe_with_gas(
                Default::default(),
                gas_limit,
                value,
            ))
            .map_err(Into::into)
    }

    pub fn reply_commit(value: ReadAsOption<u128>) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReplyCommit,
            move |ctx: &mut MemoryCallerContext<Caller>| Self::reply_commit_inner(ctx, None, value),
        )
    }

    pub fn reply_commit_wgas(gas_limit: u64, value: ReadAsOption<u128>) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReplyCommitWGas,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                Self::reply_commit_inner(ctx, Some(gas_limit), value)
            },
        )
    }

    pub fn reservation_reply(
        rid_value: ReadAs<HashWithValue>,
        payload: ReadPayloadLimited,
    ) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReservationReply(payload.size().into()),
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let HashWithValue {
                    hash: reservation_id,
                    value,
                } = rid_value.into_inner()?;
                let payload = Self::read_payload(payload)?;

                ctx.caller_wrap
                    .ext_mut()
                    .reservation_reply(reservation_id.into(), ReplyPacket::new(payload, value))
                    .map_err(Into::into)
            },
        )
    }

    pub fn reservation_reply_commit(rid_value: ReadAs<HashWithValue>) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReservationReplyCommit,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let HashWithValue {
                    hash: reservation_id,
                    value,
                } = rid_value.into_inner()?;

                ctx.caller_wrap
                    .ext_mut()
                    .reservation_reply_commit(
                        reservation_id.into(),
                        ReplyPacket::new(Default::default(), value),
                    )
                    .map_err(Into::into)
            },
        )
    }

    pub fn reply_to() -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReplyTo,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                ctx.caller_wrap.ext_mut().reply_to().map_err(Into::into)
            },
        )
    }

    pub fn signal_from() -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::SignalFrom,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                ctx.caller_wrap.ext_mut().signal_from().map_err(Into::into)
            },
        )
    }

    pub fn reply_push(payload: Read) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorBytes>(
            CostToken::ReplyPush(payload.size().into()),
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let payload = payload.into_inner()?;

                ctx.caller_wrap
                    .ext_mut()
                    .reply_push(&payload)
                    .map_err(Into::into)
            },
        )
    }

    fn reply_input_inner(
        ctx: &mut MemoryCallerContext<Caller>,
        offset: u32,
        len: u32,
        gas_limit: Option<u64>,
        value: ReadAsOption<u128>,
    ) -> Result<MessageId, RunFallibleError> {
        let value = value.into_inner()?.unwrap_or(0);

        // Charge for `len` is inside `reply_push_input`
        ctx.caller_wrap.ext_mut().reply_push_input(offset, len)?;

        ctx.caller_wrap
            .ext_mut()
            .reply_commit(ReplyPacket::maybe_with_gas(
                Default::default(),
                gas_limit,
                value,
            ))
            .map_err(Into::into)
    }

    pub fn reply_input(offset: u32, len: u32, value: ReadAsOption<u128>) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReplyInput,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                Self::reply_input_inner(ctx, offset, len, None, value)
            },
        )
    }

    pub fn reply_input_wgas(
        offset: u32,
        len: u32,
        gas_limit: u64,
        value: ReadAsOption<u128>,
    ) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReplyInputWGas,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                Self::reply_input_inner(ctx, offset, len, Some(gas_limit), value)
            },
        )
    }

    pub fn reply_push_input(offset: u32, len: u32) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorBytes>(
            CostToken::ReplyPushInput,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                ctx.caller_wrap
                    .ext_mut()
                    .reply_push_input(offset, len)
                    .map_err(Into::into)
            },
        )
    }

    fn send_input_inner(
        ctx: &mut MemoryCallerContext<Caller>,
        pid_value: ReadAs<HashWithValue>,
        offset: u32,
        len: u32,
        gas_limit: Option<u64>,
        delay: u32,
    ) -> Result<MessageId, RunFallibleError> {
        let HashWithValue {
            hash: destination,
            value,
        } = pid_value.into_inner()?;

        let handle = ctx.caller_wrap.ext_mut().send_init()?;
        // Charge for `len` inside `send_push_input`
        ctx.caller_wrap
            .ext_mut()
            .send_push_input(handle, offset, len)?;

        ctx.caller_wrap
            .ext_mut()
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

    pub fn send_input(
        pid_value: ReadAs<HashWithValue>,
        offset: u32,
        len: u32,
        delay: u32,
    ) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::SendInput,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                Self::send_input_inner(ctx, pid_value, offset, len, None, delay)
            },
        )
    }

    pub fn send_input_wgas(
        pid_value: ReadAs<HashWithValue>,
        offset: u32,
        len: u32,
        gas_limit: u64,
        delay: u32,
    ) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::SendInputWGas,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                Self::send_input_inner(ctx, pid_value, offset, len, Some(gas_limit), delay)
            },
        )
    }

    pub fn send_push_input(handle: u32, offset: u32, len: u32) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorBytes>(
            CostToken::SendPushInput,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                ctx.caller_wrap
                    .ext_mut()
                    .send_push_input(handle, offset, len)
                    .map_err(Into::into)
            },
        )
    }

    pub fn debug(data: Read) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::Debug(data.size().into()),
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let data: RuntimeBuffer = data
                    .into_inner()?
                    .try_into()
                    .map_err(|LimitedVecError| {
                        UnrecoverableMemoryError::RuntimeAllocOutOfBounds.into()
                    })
                    .map_err(TrapExplanation::UnrecoverableExt)?;

                let s = String::from_utf8(data.into_vec())
                    .map_err(|_err| UnrecoverableExecutionError::InvalidDebugString.into())
                    .map_err(TrapExplanation::UnrecoverableExt)?;
                ctx.caller_wrap.ext_mut().debug(&s)?;

                Ok(())
            },
        )
    }

    pub fn panic(data: ReadPayloadLimited) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::Null,
            move |_ctx: &mut MemoryCallerContext<Caller>| {
                let data = Self::read_payload(data)
                    .map_err(|_| UnrecoverableExecutionError::PanicBufferIsTooBig.into())
                    .map_err(TrapExplanation::UnrecoverableExt)?;

                Err(ActorTerminationReason::Trap(TrapExplanation::Panic(data.into())).into())
            },
        )
    }

    pub fn oom_panic() -> impl Syscall<Caller> {
        InfallibleSyscall::new(CostToken::Null, |_ctx: &mut MemoryCallerContext<Caller>| {
            Err(ActorTerminationReason::Trap(TrapExplanation::ProgramAllocOutOfBounds).into())
        })
    }

    pub fn reserve_gas(gas_value: u64, duration: u32) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithHash>(
            CostToken::ReserveGas,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                ctx.caller_wrap
                    .ext_mut()
                    .reserve_gas(gas_value, duration)
                    .map_err(Into::into)
            },
        )
    }

    pub fn reply_deposit(message_id: ReadAs<MessageId>, gas_value: u64) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorBytes>(
            CostToken::ReplyDeposit,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let message_id = message_id.into_inner()?;

                ctx.caller_wrap
                    .ext_mut()
                    .reply_deposit(message_id, gas_value)
                    .map_err(Into::into)
            },
        )
    }

    pub fn unreserve_gas(reservation_id: ReadAs<ReservationId>) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithGas>(
            CostToken::UnreserveGas,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let reservation_id = reservation_id.into_inner()?;

                ctx.caller_wrap
                    .ext_mut()
                    .unreserve_gas(reservation_id)
                    .map_err(Into::into)
            },
        )
    }

    pub fn system_reserve_gas(gas_value: u64) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorBytes>(
            CostToken::SystemReserveGas,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                ctx.caller_wrap
                    .ext_mut()
                    .system_reserve_gas(gas_value)
                    .map_err(Into::into)
            },
        )
    }

    pub fn gas_available(gas: WriteAs<u64>) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::GasAvailable,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let gas_available = ctx.caller_wrap.ext_mut().gas_available()?;

                gas.write(ctx, &gas_available).map_err(Into::into)
            },
        )
    }

    pub fn message_id(message_id_write: WriteAs<MessageId>) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::MsgId,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let message_id = ctx.caller_wrap.ext_mut().message_id()?;

                message_id_write.write(ctx, &message_id).map_err(Into::into)
            },
        )
    }

    pub fn program_id(program_id_write: WriteAs<ActorId>) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::ActorId,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let program_id = ctx.caller_wrap.ext_mut().program_id()?;

                program_id_write.write(ctx, &program_id).map_err(Into::into)
            },
        )
    }

    pub fn source(source_write: WriteAs<ActorId>) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::Source,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let source = ctx.caller_wrap.ext_mut().source()?;

                source_write.write(ctx, &source).map_err(Into::into)
            },
        )
    }

    pub fn value(value_write: WriteAs<u128>) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::Value,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let value = ctx.caller_wrap.ext_mut().value()?;

                value_write.write(ctx, &value).map_err(Into::into)
            },
        )
    }

    pub fn value_available(value_write: WriteAs<u128>) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::ValueAvailable,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let value_available = ctx.caller_wrap.ext_mut().value_available()?;

                value_write.write(ctx, &value_available).map_err(Into::into)
            },
        )
    }

    pub fn leave() -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::Leave,
            move |_ctx: &mut MemoryCallerContext<Caller>| Err(ActorTerminationReason::Leave.into()),
        )
    }

    pub fn wait() -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::Wait,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                ctx.caller_wrap.ext_mut().wait()?;
                Err(ActorTerminationReason::Wait(None, MessageWaitedType::Wait).into())
            },
        )
    }

    pub fn wait_for(duration: u32) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::WaitFor,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                ctx.caller_wrap.ext_mut().wait_for(duration)?;
                Err(ActorTerminationReason::Wait(Some(duration), MessageWaitedType::WaitFor).into())
            },
        )
    }

    pub fn wait_up_to(duration: u32) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::WaitUpTo,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let waited_type = if ctx.caller_wrap.ext_mut().wait_up_to(duration)? {
                    MessageWaitedType::WaitUpToFull
                } else {
                    MessageWaitedType::WaitUpTo
                };
                Err(ActorTerminationReason::Wait(Some(duration), waited_type).into())
            },
        )
    }

    pub fn wake(message_id: ReadAs<MessageId>, delay: u32) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorBytes>(
            CostToken::Wake,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                let message_id = message_id.into_inner()?;

                ctx.caller_wrap
                    .ext_mut()
                    .wake(message_id, delay)
                    .map_err(Into::into)
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn create_program_inner(
        ctx: &mut MemoryCallerContext<Caller>,
        cid_value: ReadAs<HashWithValue>,
        salt: ReadPayloadLimited,
        payload: ReadPayloadLimited,
        gas_limit: Option<u64>,
        delay: u32,
    ) -> Result<(MessageId, ActorId), RunFallibleError> {
        let HashWithValue {
            hash: code_id,
            value,
        } = cid_value.into_inner()?;
        let salt = Self::read_payload(salt)?;
        let payload = Self::read_payload(payload)?;

        let message_id = ctx.caller_wrap.ext_mut().message_id()?;

        ctx.caller_wrap
            .ext_mut()
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
        cid_value: ReadAs<HashWithValue>,
        salt: ReadPayloadLimited,
        payload: ReadPayloadLimited,
        delay: u32,
    ) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithTwoHashes>(
            CostToken::CreateProgram(payload.size().into(), salt.size().into()),
            move |ctx: &mut MemoryCallerContext<Caller>| -> Result<_, RunFallibleError> {
                Self::create_program_inner(ctx, cid_value, salt, payload, None, delay)
            },
        )
    }

    pub fn create_program_wgas(
        cid_value: ReadAs<HashWithValue>,
        salt: ReadPayloadLimited,
        payload: ReadPayloadLimited,
        gas_limit: u64,
        delay: u32,
    ) -> impl Syscall<Caller> {
        FallibleSyscall::new::<ErrorWithTwoHashes>(
            CostToken::CreateProgramWGas(payload.size().into(), salt.size().into()),
            move |ctx: &mut MemoryCallerContext<Caller>| {
                Self::create_program_inner(ctx, cid_value, salt, payload, Some(gas_limit), delay)
            },
        )
    }

    fn out_of_gas(ctx: &mut MemoryCallerContext<Caller>) -> UndefinedTerminationReason {
        let ext = ctx.caller_wrap.ext_mut();
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

    pub fn system_break(code: u32) -> impl Syscall<Caller> {
        InfallibleSyscall::new(
            CostToken::Null,
            move |ctx: &mut MemoryCallerContext<Caller>| {
                // At the instrumentation level, we can only use variants of the `SystemBreakCode`,
                // so we should never reach `unreachable!("{err_msg}")`.
                let termination_reason = SystemBreakCode::try_from(code)
                    .map(|system_break_code| match system_break_code {
                        SystemBreakCode::OutOfGas => Self::out_of_gas(ctx),
                        SystemBreakCode::StackLimitExceeded => Self::stack_limit_exceeded(),
                    })
                    .unwrap_or_else(|e| {
                        let err_msg = format!(
                            "system_break: Invalid system break code. \
                        System break code - {code}. \
                        Got error - {e}"
                        );

                        log::error!("{err_msg}");
                        unreachable!("{err_msg}")
                    });
                Err(termination_reason)
            },
        )
    }
}
