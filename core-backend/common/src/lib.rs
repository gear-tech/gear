// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Crate provides support for wasm runtime.

#![no_std]

extern crate alloc;

pub mod funcs;

use alloc::{
    borrow::Cow,
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use codec::Encode;
use core::fmt;
use gear_core::costs::RuntimeCosts;
use gear_core::memory::Memory;
use gear_core::message::{ExitCode, HandlePacket, InitPacket, ReplyPacket};
use gear_core::{
    env::Ext,
    gas::GasAmount,
    ids::{CodeId, MessageId, ProgramId},
    memory::{PageBuf, PageNumber, WasmPageNumber},
    message::{ContextStore, Dispatch},
};
use gear_core_errors::{CoreError, ExtError};

pub type HostPointer = u64;

#[derive(Debug, Clone)]
pub enum TerminationReason {
    Exit(ProgramId),
    Leave,
    Success,
    Trap {
        explanation: Option<ExtError>,
        description: Option<Cow<'static, str>>,
    },
    Wait,
    GasAllowanceExceeded,
}

pub struct ExtInfo {
    pub gas_amount: GasAmount,
    pub allocations: BTreeSet<WasmPageNumber>,
    pub pages_data: BTreeMap<PageNumber, Vec<u8>>,
    pub generated_dispatches: Vec<Dispatch>,
    pub awakening: Vec<MessageId>,
    pub program_candidates_data: BTreeMap<CodeId, Vec<(ProgramId, MessageId)>>,
    pub context_store: ContextStore,
    pub trap_explanation: Option<ExtError>,
    pub exit_argument: Option<ProgramId>,
}

pub trait IntoExtInfo {
    fn into_ext_info<F: FnMut(usize, &mut [u8]) -> Result<(), T>, T>(
        self,
        get_page_data: F,
    ) -> Result<ExtInfo, (T, GasAmount)>;
    fn into_gas_amount(self) -> GasAmount;
}

pub struct BackendReport {
    pub termination: TerminationReason,
    pub info: ExtInfo,
}

#[derive(Debug)]
pub struct BackendError<T> {
    pub gas_amount: GasAmount,
    pub reason: T,
    pub description: Option<Cow<'static, str>>,
}

impl<T> fmt::Display for BackendError<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(description) = &self.description {
            write!(f, "{}: {}", self.reason, description)
        } else {
            write!(f, "{}", self.reason)
        }
    }
}

pub trait Environment<E: Ext + IntoExtInfo + 'static>: Sized {
    /// An error issues in environment
    type Error: fmt::Display;

    /// Creates new external environment to execute wasm binary:
    /// 1) instatiates wasm binary.
    /// 2) creates wasm memory with filled data (execption if lazy pages enabled).
    /// 3) instatiate external funcs for wasm module.
    fn new(
        ext: E,
        binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        mem_size: WasmPageNumber,
    ) -> Result<Self, BackendError<Self::Error>>;

    /// Returns addr to the stack end if it can be identified
    fn get_stack_mem_end(&mut self) -> Option<WasmPageNumber>;

    /// Returns host address of wasm memory buffer. Needed for lazy-pages
    fn get_wasm_memory_begin_addr(&self) -> HostPointer;

    /// Run instance setup starting at `entry_point` - wasm export function name.
    /// Also runs `post_execution_handler` after running instance at provided entry point.
    fn execute<F, T>(
        self,
        entry_point: &str,
        post_execution_handler: F,
    ) -> Result<BackendReport, BackendError<Self::Error>>
    where
        F: FnOnce(HostPointer) -> Result<(), T>,
        T: fmt::Display;

    /// Consumes environment and returns gas state.
    fn into_gas_amount(self) -> GasAmount;
}

pub struct ExtErrorProcessor<'a, T, E: Ext> {
    ext: &'a mut ErrorSavingExt<E>,
    success: Option<T>,
}

impl<'a, T, E: Ext> ExtErrorProcessor<'a, T, E> {
    pub fn new(ext: &'a mut ErrorSavingExt<E>) -> Self {
        Self { ext, success: None }
    }

    pub fn with<U, F>(mut self, f: F) -> Result<Self, U>
    where
        F: FnOnce(&mut ErrorSavingExt<E>) -> Result<T, U>,
        U: CoreError,
    {
        match f(self.ext) {
            Ok(t) => {
                self.success = Some(t);
            }
            Err(err) => {
                err.into_ext_error()?;
            }
        };
        Ok(self)
    }

    pub fn on_success<U, F>(mut self, f: F) -> Result<Self, U>
    where
        F: FnOnce(T) -> Result<(), U>,
    {
        self.success.take().map(f).transpose()?;
        Ok(self)
    }

    pub fn error_len(self) -> u32 {
        self.ext
            .err
            .as_ref()
            .and_then(|err| err.as_ext_error())
            .map(|err| err.encoded_size() as u32)
            .unwrap_or(0)
    }
}

// TODO: use for issue #911
pub struct ErrorSavingExt<T: Ext> {
    pub inner: T,
    /// Ext error, if available.
    pub err: Option<ExtError>,
}

impl<E> ErrorSavingExt<E>
where
    E: Ext,
{
    pub fn new(inner: E) -> Self {
        Self { inner, err: None }
    }

    /// Return result and store error info in field
    fn return_and_store_err<T>(&mut self, result: Result<T, E::Error>) -> Result<T, E::Error> {
        result.map_err(|err| {
            self.err = err.as_ext_error().cloned();
            err
        })
    }
}

impl<T> Ext for ErrorSavingExt<T>
where
    T: Ext,
{
    type Error = T::Error;

    fn alloc(
        &mut self,
        pages: WasmPageNumber,
        mem: &mut dyn Memory,
    ) -> Result<WasmPageNumber, Self::Error> {
        let res = self.inner.alloc(pages, mem);
        self.return_and_store_err(res)
    }

    fn block_height(&mut self) -> Result<u32, Self::Error> {
        let res = self.inner.block_height();
        self.return_and_store_err(res)
    }

    fn block_timestamp(&mut self) -> Result<u64, Self::Error> {
        let res = self.inner.block_timestamp();
        self.return_and_store_err(res)
    }

    fn origin(&mut self) -> Result<ProgramId, Self::Error> {
        let res = self.inner.origin();
        self.return_and_store_err(res)
    }

    fn send_init(&mut self) -> Result<usize, Self::Error> {
        let res = self.inner.send_init();
        self.return_and_store_err(res)
    }

    fn send_push(&mut self, handle: usize, buffer: &[u8]) -> Result<(), Self::Error> {
        let res = self.inner.send_push(handle, buffer);
        self.return_and_store_err(res)
    }

    fn send_commit(&mut self, handle: usize, msg: HandlePacket) -> Result<MessageId, Self::Error> {
        let res = self.inner.send_commit(handle, msg);
        self.return_and_store_err(res)
    }

    fn send(&mut self, msg: HandlePacket) -> Result<MessageId, Self::Error> {
        let res = self.inner.send(msg);
        self.return_and_store_err(res)
    }

    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Self::Error> {
        let res = self.inner.reply_push(buffer);
        self.return_and_store_err(res)
    }

    fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::Error> {
        let res = self.inner.reply_commit(msg);
        self.return_and_store_err(res)
    }

    fn reply(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::Error> {
        let res = self.inner.reply(msg);
        self.return_and_store_err(res)
    }

    fn reply_to(&mut self) -> Result<Option<(MessageId, ExitCode)>, Self::Error> {
        let res = self.inner.reply_to();
        self.return_and_store_err(res)
    }

    fn source(&mut self) -> Result<ProgramId, Self::Error> {
        let res = self.inner.source();
        self.return_and_store_err(res)
    }

    fn exit(&mut self, value_destination: ProgramId) -> Result<(), Self::Error> {
        let res = self.inner.exit(value_destination);
        self.return_and_store_err(res)
    }

    fn message_id(&mut self) -> Result<MessageId, Self::Error> {
        let res = self.inner.message_id();
        self.return_and_store_err(res)
    }

    fn program_id(&mut self) -> Result<ProgramId, Self::Error> {
        let res = self.inner.program_id();
        self.return_and_store_err(res)
    }

    fn free(&mut self, page: WasmPageNumber) -> Result<(), Self::Error> {
        let res = self.inner.free(page);
        self.return_and_store_err(res)
    }

    fn debug(&mut self, data: &str) -> Result<(), Self::Error> {
        let res = self.inner.debug(data);
        self.return_and_store_err(res)
    }

    fn leave(&mut self) -> Result<(), Self::Error> {
        let res = self.inner.leave();
        self.return_and_store_err(res)
    }

    fn msg(&mut self) -> &[u8] {
        self.inner.msg()
    }

    fn gas(&mut self, amount: u32) -> Result<(), Self::Error> {
        let res = self.inner.gas(amount);
        self.return_and_store_err(res)
    }

    fn charge_gas(&mut self, amount: u32) -> Result<(), Self::Error> {
        let res = self.inner.charge_gas(amount);
        self.return_and_store_err(res)
    }

    fn charge_gas_runtime(&mut self, costs: RuntimeCosts) -> Result<(), Self::Error> {
        let res = self.inner.charge_gas_runtime(costs);
        self.return_and_store_err(res)
    }

    fn refund_gas(&mut self, amount: u32) -> Result<(), Self::Error> {
        let res = self.inner.refund_gas(amount);
        self.return_and_store_err(res)
    }

    fn gas_available(&mut self) -> Result<u64, Self::Error> {
        let res = self.inner.gas_available();
        self.return_and_store_err(res)
    }

    fn value(&mut self) -> Result<u128, Self::Error> {
        let res = self.inner.value();
        self.return_and_store_err(res)
    }

    fn value_available(&mut self) -> Result<u128, Self::Error> {
        let res = self.inner.value_available();
        self.return_and_store_err(res)
    }

    fn wait(&mut self) -> Result<(), Self::Error> {
        let res = self.inner.wait();
        self.return_and_store_err(res)
    }

    fn wake(&mut self, waker_id: MessageId) -> Result<(), Self::Error> {
        let res = self.inner.wake(waker_id);
        self.return_and_store_err(res)
    }

    fn create_program(&mut self, packet: InitPacket) -> Result<ProgramId, Self::Error> {
        let res = self.inner.create_program(packet);
        self.return_and_store_err(res)
    }
}
