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

//! Memory accessors

use crate::{
    BackendExternalities,
    memory::{
        BackendMemory, ExecutorMemory, MemoryAccessError, MemoryAccessRegistry, WasmMemoryRead,
        WasmMemoryReadAs, WasmMemoryWrite, WasmMemoryWriteAs,
    },
    runtime::MemoryCallerContext,
    state::HostState,
};
use alloc::vec::Vec;
use bytemuck::Pod;
use gear_core::{buffer::Payload, limited::LimitedVecError};
use gear_sandbox::{AsContextExt, Value};
use gear_sandbox_env::HostError;

const PTR_SPECIAL: u32 = u32::MAX;

/// Actually just wrapper around [`Value`] to implement conversions.
#[derive(Clone, Copy)]
pub(crate) struct SyscallValue(pub Value);

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

impl From<Value> for SyscallValue {
    fn from(value: Value) -> Self {
        SyscallValue(value)
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

pub(crate) trait SyscallArg: Sized {
    type Output;

    const REQUIRED_ARGS: usize;

    fn pre_process<Caller, Ext>(
        registry: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self::Output, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static;

    fn post_process<Caller, Ext>(
        output: Self::Output,
        ctx: &mut MemoryCallerContext<Caller>,
    ) -> Self
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static;
}

pub(crate) struct Read {
    result: Result<Vec<u8>, MemoryAccessError>,
    size: u32,
}

pub(crate) struct ReadPayloadLimited<const N: usize = { Payload::MAX_LEN }> {
    result: Result<Result<Payload, MemoryAccessError>, LimitedVecError>,
    size: u32,
}

pub(crate) struct ReadAs<T> {
    result: Result<T, MemoryAccessError>,
}

pub(crate) struct ReadAsOption<T> {
    result: Result<Option<T>, MemoryAccessError>,
}

pub(crate) struct WriteInGrRead {
    write: WasmMemoryWrite,
}

pub(crate) struct WriteAs<T> {
    write: WasmMemoryWriteAs<T>,
}

impl SyscallArg for u32 {
    type Output = u32;

    const REQUIRED_ARGS: usize = 1;

    fn pre_process<Caller, Ext>(
        _: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self::Output, HostError> {
        debug_assert_eq!(args.len(), Self::REQUIRED_ARGS);

        SyscallValue(args[0]).try_into()
    }

    fn post_process<Caller, Ext>(
        output: Self::Output,
        _ctx: &mut MemoryCallerContext<Caller>,
    ) -> Self {
        output
    }
}

impl SyscallArg for u64 {
    type Output = u64;
    const REQUIRED_ARGS: usize = 1;

    fn pre_process<Caller, Ext>(
        _: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self::Output, HostError> {
        debug_assert_eq!(args.len(), Self::REQUIRED_ARGS);

        SyscallValue(args[0]).try_into()
    }

    fn post_process<Caller, Ext>(
        output: Self::Output,
        _ctx: &mut MemoryCallerContext<Caller>,
    ) -> Self {
        output
    }
}

impl SyscallArg for Read {
    type Output = WasmMemoryRead;
    const REQUIRED_ARGS: usize = 2;

    fn pre_process<Caller, Ext>(
        registry: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self::Output, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        debug_assert_eq!(args.len(), Self::REQUIRED_ARGS);

        let ptr = SyscallValue(args[0]).try_into()?;
        let size = SyscallValue(args[1]).try_into()?;

        Ok(registry.get_or_insert_default().register_read(ptr, size))
    }

    fn post_process<Caller, Ext>(
        output: Self::Output,
        ctx: &mut MemoryCallerContext<Caller>,
    ) -> Self
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        Self {
            size: output.size,
            result: ctx
                .memory_wrap
                .io_mut_ref()
                .and_then(|io| io.read(&mut ctx.caller_wrap, output)),
        }
    }
}

impl<const N: usize> SyscallArg for ReadPayloadLimited<N> {
    type Output = Result<WasmMemoryRead, LimitedVecError>;
    const REQUIRED_ARGS: usize = 2;

    fn pre_process<Caller, Ext>(
        registry: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self::Output, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        debug_assert_eq!(args.len(), Self::REQUIRED_ARGS);

        let ptr = SyscallValue(args[0]).try_into()?;
        let size = SyscallValue(args[1]).try_into()?;

        if size as usize > N {
            Ok(Err(LimitedVecError))
        } else {
            Ok(Ok(registry
                .get_or_insert_default()
                .register_read(ptr, size)))
        }
    }

    fn post_process<Caller, Ext>(
        output: Self::Output,
        ctx: &mut MemoryCallerContext<Caller>,
    ) -> Self
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        match output {
            Ok(output) => {
                let size = output.size;

                let bytes = ctx
                    .memory_wrap
                    .io_mut_ref()
                    .and_then(|io| io.read(&mut ctx.caller_wrap, output));

                let payload = bytes.map(|bytes| {
                    bytes.try_into().unwrap_or_else(|_| {
                        unreachable!("Length is checked inside ReadPayloadLimited::pre_process")
                    })
                });

                Self {
                    size,
                    result: Ok(payload),
                }
            }
            Err(err) => Self {
                size: 0,
                result: Err(err),
            },
        }
    }
}

impl<T: Pod> SyscallArg for ReadAs<T> {
    type Output = WasmMemoryReadAs<T>;
    const REQUIRED_ARGS: usize = 1;

    fn pre_process<Caller, Ext>(
        registry: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self::Output, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        debug_assert_eq!(args.len(), Self::REQUIRED_ARGS);

        let ptr = SyscallValue(args[0]).try_into()?;

        Ok(registry.get_or_insert_default().register_read_as(ptr))
    }

    fn post_process<Caller, Ext>(
        output: Self::Output,
        ctx: &mut MemoryCallerContext<Caller>,
    ) -> Self
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        Self {
            result: ctx
                .memory_wrap
                .io_mut_ref()
                .and_then(|io| io.read_as(&mut ctx.caller_wrap, output)),
        }
    }
}

impl<T: Pod> SyscallArg for ReadAsOption<T> {
    type Output = Option<WasmMemoryReadAs<T>>;
    const REQUIRED_ARGS: usize = 1;

    fn pre_process<Caller, Ext>(
        registry: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self::Output, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        debug_assert_eq!(args.len(), Self::REQUIRED_ARGS);

        let ptr = SyscallValue(args[0]).try_into()?;

        if ptr != PTR_SPECIAL {
            Ok(Some(registry.get_or_insert_default().register_read_as(ptr)))
        } else {
            Ok(None)
        }
    }

    fn post_process<Caller, Ext>(
        output: Self::Output,
        ctx: &mut MemoryCallerContext<Caller>,
    ) -> Self
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        Self {
            result: output
                .map(|read| {
                    ctx.memory_wrap
                        .io_mut_ref()
                        .and_then(|io| io.read_as(&mut ctx.caller_wrap, read))
                })
                .transpose(),
        }
    }
}

impl SyscallArg for WriteInGrRead {
    type Output = WasmMemoryWrite;
    const REQUIRED_ARGS: usize = 2;

    fn pre_process<Caller, Ext>(
        registry: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self::Output, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        debug_assert_eq!(args.len(), Self::REQUIRED_ARGS);

        let ptr = SyscallValue(args[1]).try_into()?;
        let size = SyscallValue(args[0]).try_into()?;

        Ok(registry.get_or_insert_default().register_write(ptr, size))
    }

    fn post_process<Caller, Ext>(
        output: Self::Output,
        _ctx: &mut MemoryCallerContext<Caller>,
    ) -> Self {
        Self { write: output }
    }
}

impl<T> SyscallArg for WriteAs<T> {
    type Output = WasmMemoryWriteAs<T>;
    const REQUIRED_ARGS: usize = 1;

    fn pre_process<Caller, Ext>(
        registry: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self::Output, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        debug_assert_eq!(args.len(), Self::REQUIRED_ARGS);

        let ptr = SyscallValue(args[0]).try_into()?;

        Ok(registry.get_or_insert_default().register_write_as(ptr))
    }

    fn post_process<Caller, Ext>(
        output: Self::Output,
        _ctx: &mut MemoryCallerContext<Caller>,
    ) -> Self {
        Self { write: output }
    }
}

impl Read {
    pub fn into_inner(self) -> Result<Vec<u8>, MemoryAccessError> {
        self.result
    }

    pub fn size(&self) -> u32 {
        self.size
    }
}

impl<const N: usize> ReadPayloadLimited<N> {
    pub fn into_inner(self) -> Result<Result<Payload, MemoryAccessError>, LimitedVecError> {
        self.result
    }

    pub fn size(&self) -> u32 {
        self.size
    }
}

impl<T> ReadAs<T> {
    pub fn into_inner(self) -> Result<T, MemoryAccessError> {
        self.result
    }
}

impl<T> ReadAsOption<T> {
    pub fn into_inner(self) -> Result<Option<T>, MemoryAccessError> {
        self.result
    }
}

impl WriteInGrRead {
    pub fn write<Caller, Ext>(
        self,
        ctx: &mut MemoryCallerContext<Caller>,
        buff: &[u8],
    ) -> Result<(), MemoryAccessError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        ctx.memory_wrap
            .io_mut_ref()
            .and_then(|io| io.write(&mut ctx.caller_wrap, self.write, buff))
    }

    pub fn size(&self) -> u32 {
        self.write.size
    }
}

impl<T: Pod> WriteAs<T> {
    pub fn write<Caller, Ext>(
        self,
        ctx: &mut MemoryCallerContext<Caller>,
        obj: &T,
    ) -> Result<(), MemoryAccessError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        ctx.memory_wrap
            .io_mut_ref()
            .and_then(|io| io.write_as(&mut ctx.caller_wrap, self.write, obj))
    }
}
