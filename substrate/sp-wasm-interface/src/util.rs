// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::*;
use wasmtime::{AsContext, AsContextMut};
pub use sp_wasm_interface_common::util::checked_range;

pub fn write_memory_from(
    mut ctx: impl AsContextMut<Data = StoreData>,
    address: Pointer<u8>,
    data: &[u8],
) -> Result<()> {
    let memory = ctx.as_context().data().memory();
    let memory = memory.data_mut(&mut ctx);

    let range = checked_range(address.into(), data.len(), memory.len())
        .ok_or_else(|| String::from("memory write is out of bounds"))?;
    memory[range].copy_from_slice(data);
    Ok(())
}

pub fn read_memory_into(
    ctx: impl AsContext<Data = StoreData>,
    address: Pointer<u8>,
    dest: &mut [u8],
) -> Result<()> {
    let memory = ctx.as_context().data().memory().data(&ctx);

    let range = checked_range(address.into(), dest.len(), memory.len())
        .ok_or_else(|| String::from("memory read is out of bounds"))?;
    dest.copy_from_slice(&memory[range]);
    Ok(())
}

pub fn read_memory(
    ctx: impl AsContext<Data = StoreData>,
    address: Pointer<u8>,
    size: WordSize,
) -> Result<Vec<u8>> {
    let mut vec = vec![0; size as usize];
    read_memory_into(ctx, address, &mut vec)?;
    Ok(vec)
}

#[track_caller]
fn host_state_mut<'a>(caller: &'a mut Caller<'_, StoreData>) -> &'a mut HostState {
    caller
        .data_mut()
        .host_state_mut()
        .expect("host state is not empty when calling a function in wasm; qed")
}

pub fn allocate_memory(caller: &mut Caller<'_, StoreData>, size: WordSize) -> Result<Pointer<u8>> {
    let mut allocator = host_state_mut(caller)
        .allocator
        .take()
        .expect("allocator is not empty when calling a function in wasm; qed");

    let memory = caller.data().memory();
    // We can not return on error early, as we need to store back allocator.
    let res = allocator
        .allocate(&mut MemoryWrapper::from((&memory, &mut caller.as_context_mut())), size)
        .map_err(|e| e.to_string());

    host_state_mut(caller).allocator = Some(allocator);

    res
}

pub fn deallocate_memory(caller: &mut Caller<'_, StoreData>, ptr: Pointer<u8>) -> Result<()> {
    let mut allocator = host_state_mut(caller)
        .allocator
        .take()
        .expect("allocator is not empty when calling a function in wasm; qed");

    let memory = caller.data().memory();

    // We can not return on error early, as we need to store back allocator.
    let res = allocator
        .deallocate(&mut MemoryWrapper::from((&memory, &mut caller.as_context_mut())), ptr)
        .map_err(|e| e.to_string());

    host_state_mut(caller).allocator = Some(allocator);

    res
}
