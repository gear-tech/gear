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
    memory::{
        BackendMemory, ExecutorMemory, MemoryAccessError, MemoryAccessIo, MemoryAccessRegistry,
        WasmMemoryRead, WasmMemoryReadAs, WasmMemoryReadDecoded, WasmMemoryWrite,
        WasmMemoryWriteAs,
    },
    runtime::CallerWrap,
    state::HostState,
    BackendExternalities,
};
use alloc::vec::Vec;
use codec::{Decode, MaxEncodedLen};
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
    const REQUIRED_ARGS: usize;
    const REQUIRES_MEMORY_MANAGER: bool;

    fn new<Caller, Ext>(
        registry: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static;
}

pub(crate) struct Read {
    read: WasmMemoryRead,
}

pub(crate) struct ReadAs<T> {
    read: WasmMemoryReadAs<T>,
}

pub(crate) struct ReadDecoded<T: Decode + MaxEncodedLen> {
    read: WasmMemoryReadDecoded<T>,
}

pub(crate) struct ReadDecodedSpecial<T: Decode + MaxEncodedLen + Default> {
    read: Option<WasmMemoryReadDecoded<T>>,
}

pub(crate) struct Write {
    write: WasmMemoryWrite,
}

pub(crate) struct WriteAs<T> {
    write: WasmMemoryWriteAs<T>,
}

impl SyscallArg for u32 {
    const REQUIRED_ARGS: usize = 1;
    const REQUIRES_MEMORY_MANAGER: bool = false;

    fn new<Caller, Ext>(
        _: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        if args.len() != Self::REQUIRED_ARGS {
            return Err(HostError);
        }

        SyscallValue(args[0]).try_into()
    }
}

impl SyscallArg for u64 {
    const REQUIRED_ARGS: usize = 1;
    const REQUIRES_MEMORY_MANAGER: bool = false;

    fn new<Caller, Ext>(
        _: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        if args.len() != Self::REQUIRED_ARGS {
            return Err(HostError);
        }

        SyscallValue(args[0]).try_into()
    }
}

impl SyscallArg for Read {
    const REQUIRED_ARGS: usize = 2;
    const REQUIRES_MEMORY_MANAGER: bool = true;

    fn new<Caller, Ext>(
        registry: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        if args.len() != Self::REQUIRED_ARGS {
            return Err(HostError);
        }

        let ptr = SyscallValue(args[0]).try_into()?;
        let size = SyscallValue(args[1]).try_into()?;

        let read = registry.as_mut().unwrap().register_read(ptr, size);

        Ok(Self { read })
    }
}

impl<T> SyscallArg for ReadAs<T> {
    const REQUIRED_ARGS: usize = 1;
    const REQUIRES_MEMORY_MANAGER: bool = true;

    fn new<Caller, Ext>(
        registry: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        if args.len() != Self::REQUIRED_ARGS {
            return Err(HostError);
        }

        let ptr = SyscallValue(args[0]).try_into()?;

        let read = registry.as_mut().unwrap().register_read_as(ptr);
        Ok(Self { read })
    }
}

impl<T: Decode + MaxEncodedLen> SyscallArg for ReadDecoded<T> {
    const REQUIRED_ARGS: usize = 1;
    const REQUIRES_MEMORY_MANAGER: bool = true;

    fn new<Caller, Ext>(
        registry: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        if args.len() != Self::REQUIRED_ARGS {
            return Err(HostError);
        }

        let ptr = SyscallValue(args[0]).try_into()?;

        let read = registry.as_mut().unwrap().register_read_decoded(ptr);
        Ok(Self { read })
    }
}

impl<T: Decode + MaxEncodedLen + Default> SyscallArg for ReadDecodedSpecial<T> {
    const REQUIRED_ARGS: usize = 1;
    const REQUIRES_MEMORY_MANAGER: bool = true;
    fn new<Caller, Ext>(
        registry: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        if args.len() != Self::REQUIRED_ARGS {
            return Err(HostError);
        }

        let ptr: u32 = SyscallValue(args[0]).try_into()?;

        if ptr != PTR_SPECIAL {
            let read = registry.as_mut().unwrap().register_read_decoded(ptr);
            Ok(Self { read: Some(read) })
        } else {
            Ok(Self { read: None })
        }
    }
}

impl SyscallArg for Write {
    const REQUIRED_ARGS: usize = 2;
    const REQUIRES_MEMORY_MANAGER: bool = true;

    fn new<Caller, Ext>(
        registry: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        if args.len() != Self::REQUIRED_ARGS {
            return Err(HostError);
        }

        let ptr = SyscallValue(args[1]).try_into()?;
        let size = SyscallValue(args[0]).try_into()?;

        let write = registry.as_mut().unwrap().register_write(ptr, size);

        Ok(Self { write })
    }
}

impl<T> SyscallArg for WriteAs<T> {
    const REQUIRED_ARGS: usize = 1;
    const REQUIRES_MEMORY_MANAGER: bool = true;

    fn new<Caller, Ext>(
        registry: &mut Option<MemoryAccessRegistry<Caller>>,
        args: &[Value],
    ) -> Result<Self, HostError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        if args.len() != Self::REQUIRED_ARGS {
            return Err(HostError);
        }

        let ptr = SyscallValue(args[0]).try_into()?;

        let write = registry.as_mut().unwrap().register_write_as(ptr);
        Ok(Self { write })
    }
}

impl Read {
    pub fn read<Caller, Ext>(
        self,
        ctx: &mut CallerWrap<Caller>,
        io: &MemoryAccessIo<Caller, BackendMemory<ExecutorMemory>>,
    ) -> Result<Vec<u8>, MemoryAccessError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        io.read(ctx, self.read)
    }

    pub fn size(&self) -> u32 {
        self.read.size
    }
}

impl<T> ReadAs<T> {
    pub fn read<Caller, Ext>(
        self,
        ctx: &mut CallerWrap<Caller>,
        io: &MemoryAccessIo<Caller, BackendMemory<ExecutorMemory>>,
    ) -> Result<T, MemoryAccessError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        io.read_as(ctx, self.read)
    }
}

impl<T: Decode + MaxEncodedLen> ReadDecoded<T> {
    pub fn read<Caller, Ext>(
        self,
        ctx: &mut CallerWrap<Caller>,
        io: &MemoryAccessIo<Caller, BackendMemory<ExecutorMemory>>,
    ) -> Result<T, MemoryAccessError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        io.read_decoded(ctx, self.read)
    }
}

impl<T: Decode + MaxEncodedLen + Default> ReadDecodedSpecial<T> {
    pub fn read<Caller, Ext>(
        self,
        ctx: &mut CallerWrap<Caller>,
        io: &MemoryAccessIo<Caller, BackendMemory<ExecutorMemory>>,
    ) -> Result<T, MemoryAccessError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        match self.read {
            Some(read) => io.read_decoded(ctx, read),
            None => Ok(Default::default()),
        }
    }
}

impl Write {
    pub fn write<Caller, Ext>(
        self,
        ctx: &mut CallerWrap<Caller>,
        io: &mut MemoryAccessIo<Caller, BackendMemory<ExecutorMemory>>,
        buff: &[u8],
    ) -> Result<(), MemoryAccessError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        io.write(ctx, self.write, buff)
    }

    pub fn size(&self) -> u32 {
        self.write.size
    }
}

impl<T> WriteAs<T> {
    pub fn write<Caller, Ext>(
        self,
        ctx: &mut CallerWrap<Caller>,
        io: &mut MemoryAccessIo<Caller, BackendMemory<ExecutorMemory>>,
        obj: T,
    ) -> Result<(), MemoryAccessError>
    where
        Caller: AsContextExt<State = HostState<Ext, BackendMemory<ExecutorMemory>>>,
        Ext: BackendExternalities + 'static,
    {
        io.write_as(ctx, self.write, obj)
    }
}
