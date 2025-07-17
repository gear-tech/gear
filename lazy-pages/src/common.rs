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

//! Lazy-pages structures for common usage.

use crate::{
    globals::GlobalsContext,
    mprotect::MprotectError,
    pages::{GearPage, SIZES_AMOUNT, SizeManager, SizeNumber, WasmPage, WasmPagesAmount},
};
use gear_core::str::LimitedStr;
use gear_lazy_pages_common::{GlobalsAccessError, Status};
use numerated::tree::IntervalsTree;
use std::{fmt, num::NonZero};

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
    #[display("Accessed memory interval is out of wasm memory")]
    OutOfWasmMemoryAccess,
    #[display("Signals cannot come from WASM program virtual stack memory")]
    SignalFromStackMemory,
    #[display("Signals cannot come from write accessed page")]
    SignalFromWriteAccessedPage,
    #[display("Read access signal cannot come from already accessed page")]
    ReadAccessSignalFromAccessedPage,
    #[display("WASM memory begin address is not set")]
    WasmMemAddrIsNotSet,
    #[display("Page data in storage must contain {expected} bytes, actually has {actual}")]
    InvalidPageDataSize {
        expected: u32,
        actual: u32,
    },
    #[from(skip)]
    #[display("Any page cannot be write accessed twice: {_0:?}")]
    DoubleWriteAccess(GearPage),
    #[from(skip)]
    #[display("Any page cannot be read charged twice: {_0:?}")]
    DoubleReadCharge(GearPage),
    #[display("Memory protection error: {_0}")]
    MemoryProtection(MprotectError),
    #[display("Given instance host pointer is invalid")]
    HostInstancePointerIsInvalid,
    #[display("Given pointer to globals access provider dyn object is invalid")]
    DynGlobalsAccessPointerIsInvalid,
    #[display("Something goes wrong when trying to access globals: {_0:?}")]
    AccessGlobal(GlobalsAccessError),
    #[display("It's unknown whether memory access is read or write")]
    ReadOrWriteIsUnknown,
    #[display("Cannot receive signal from wasm memory, when status is gas limit exceed")]
    SignalWhenStatusGasExceeded,
    GlobalContext(ContextError),
}

#[derive(Debug, derive_more::Display)]
pub enum ContextError {
    RuntimeContextIsNotSet,
    ExecutionContextIsNotSet,
}

#[derive(Debug, Default)]
pub(crate) struct LazyPagesContext {
    runtime_context: Option<LazyPagesRuntimeContext>,
    execution_context: Option<LazyPagesExecutionContext>,
}

impl LazyPagesContext {
    pub fn contexts(
        &self,
    ) -> Result<(&LazyPagesRuntimeContext, &LazyPagesExecutionContext), ContextError> {
        Ok((self.runtime_context()?, self.execution_context()?))
    }

    pub fn contexts_mut(
        &mut self,
    ) -> Result<(&mut LazyPagesRuntimeContext, &mut LazyPagesExecutionContext), ContextError> {
        let rt_ctx = self
            .runtime_context
            .as_mut()
            .ok_or(ContextError::RuntimeContextIsNotSet)?;
        let exec_ctx = self
            .execution_context
            .as_mut()
            .ok_or(ContextError::ExecutionContextIsNotSet)?;
        Ok((rt_ctx, exec_ctx))
    }

    pub fn runtime_context(&self) -> Result<&LazyPagesRuntimeContext, ContextError> {
        self.runtime_context
            .as_ref()
            .ok_or(ContextError::RuntimeContextIsNotSet)
    }

    pub fn runtime_context_mut(&mut self) -> Result<&mut LazyPagesRuntimeContext, ContextError> {
        self.runtime_context
            .as_mut()
            .ok_or(ContextError::RuntimeContextIsNotSet)
    }

    pub fn execution_context(&self) -> Result<&LazyPagesExecutionContext, ContextError> {
        self.execution_context
            .as_ref()
            .ok_or(ContextError::ExecutionContextIsNotSet)
    }

    pub fn set_runtime_context(&mut self, ctx: LazyPagesRuntimeContext) {
        self.runtime_context = Some(ctx);
    }

    pub fn set_execution_context(&mut self, ctx: LazyPagesExecutionContext) {
        self.execution_context = Some(ctx);
    }
}

pub(crate) type Costs = [u64; CostNo::Amount as usize];
pub(crate) type GlobalNames = Vec<LimitedStr<'static>>;
pub(crate) type PageSizes = [NonZero<u32>; SIZES_AMOUNT];

#[derive(Debug)]
pub(crate) struct LazyPagesRuntimeContext {
    pub page_sizes: PageSizes,
    pub global_names: GlobalNames,
    pub pages_storage_prefix: Vec<u8>,
    pub program_storage: Box<dyn LazyPagesStorage>,
}

impl LazyPagesRuntimeContext {
    pub fn page_has_data_in_storage(&self, prefix: &mut PagePrefix, page: GearPage) -> bool {
        let key = prefix.key_for_page(page);
        self.program_storage.page_exists(key)
    }

    pub fn load_page_data_from_storage(
        &mut self,
        prefix: &mut PagePrefix,
        page: GearPage,
        buffer: &mut [u8],
    ) -> Result<bool, Error> {
        let key = prefix.key_for_page(page);
        if let Some(size) = self.program_storage.load_page(key, buffer) {
            if size != GearPage::size(self) {
                return Err(Error::InvalidPageDataSize {
                    expected: GearPage::size(self),
                    actual: size,
                });
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

pub trait LazyPagesStorage: fmt::Debug {
    fn page_exists(&self, key: &[u8]) -> bool;

    fn load_page(&mut self, key: &[u8], buffer: &mut [u8]) -> Option<u32>;
}

impl LazyPagesStorage for () {
    fn page_exists(&self, _key: &[u8]) -> bool {
        unreachable!()
    }

    fn load_page(&mut self, _key: &[u8], _buffer: &mut [u8]) -> Option<u32> {
        unreachable!()
    }
}

#[derive(Debug)]
pub(crate) struct LazyPagesExecutionContext {
    /// Lazy-pages accesses costs.
    pub costs: Costs,
    /// Pointer to the begin of wasm memory buffer
    pub wasm_mem_addr: Option<usize>,
    /// Wasm memory buffer size, to identify whether signal is from wasm memory buffer.
    pub wasm_mem_size: WasmPagesAmount,
    /// Current program prefix in storage
    pub program_storage_prefix: PagePrefix,
    /// Pages which has been accessed by program during current execution
    pub accessed_pages: IntervalsTree<GearPage>,
    /// Pages which has been write accessed by program during current execution
    pub write_accessed_pages: IntervalsTree<GearPage>,
    /// End of stack page (not inclusive). Default is `0`, which means,
    /// that wasm data has no stack region. It's not necessary to specify
    /// this value, `lazy-pages` uses it to identify memory, for which we
    /// can skip processing and this memory won't be protected. So, pages
    /// which lies before this value will never get into `write_accessed_pages`,
    /// which means that they will never be uploaded to storage.
    pub stack_end: WasmPage,
    /// Context to access globals and works with them: charge gas, set status global.
    pub globals_context: Option<GlobalsContext>,
    /// Lazy-pages status: indicates in which mod lazy-pages works actually.
    pub status: Status,
}

/// Lazy-pages version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LazyPagesVersion {
    Version1,
}

impl SizeManager for LazyPagesRuntimeContext {
    fn size_non_zero<S: SizeNumber>(&self) -> NonZero<u32> {
        self.page_sizes[S::SIZE_NO]
    }
}

impl LazyPagesExecutionContext {
    pub fn is_accessed(&self, page: GearPage) -> bool {
        self.accessed_pages.contains(page)
    }

    pub fn is_write_accessed(&self, page: GearPage) -> bool {
        self.write_accessed_pages.contains(page)
    }

    pub fn set_accessed(&mut self, page: GearPage) {
        self.accessed_pages.insert(page);
    }

    pub fn set_write_accessed(&mut self, page: GearPage) -> Result<(), Error> {
        self.set_accessed(page);
        match self.write_accessed_pages.insert(page) {
            true => Ok(()),
            false => Err(Error::DoubleWriteAccess(page)),
        }
    }

    pub fn cost(&self, no: CostNo) -> u64 {
        self.costs[no as usize]
    }
}

/// Struct for fast calculation of page key in storage.
/// Key consists of two parts:
/// 1) current program prefix in storage
/// 2) page number in little endian bytes order
///
/// First part is always the same, so we can copy it to buffer
///    once and then use it for all pages.
#[derive(Debug)]
pub(crate) struct PagePrefix {
    buffer: Vec<u8>,
}

impl PagePrefix {
    /// New page prefix from program prefix
    pub(crate) fn new_from_program_prefix(mut storage_prefix: Vec<u8>) -> Self {
        storage_prefix.extend_from_slice(&u32::MAX.to_le_bytes());
        Self {
            buffer: storage_prefix,
        }
    }

    /// Returns key in storage for `page`.
    fn key_for_page(&mut self, page: GearPage) -> &[u8] {
        let len = self.buffer.len();
        self.buffer[len - size_of::<u32>()..len]
            .copy_from_slice(page.raw().to_le_bytes().as_slice());
        &self.buffer
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GasCharger {
    pub read_cost: u64,
    pub write_cost: u64,
    pub write_after_read_cost: u64,
    pub load_data_cost: u64,
}

impl GasCharger {
    fn sub_gas(gas_counter: &mut u64, amount: u64) -> Status {
        let new_gas = gas_counter.checked_sub(amount);
        *gas_counter = new_gas.unwrap_or_default();
        match new_gas {
            None => Status::GasLimitExceeded,
            Some(_) => Status::Normal,
        }
    }

    pub fn charge_for_page_access(
        &self,
        gas_counter: &mut u64,
        page: GearPage,
        is_write: bool,
        is_accessed: bool,
    ) -> Result<Status, Error> {
        let amount = match (is_write, is_accessed) {
            (true, true) => self.write_after_read_cost,
            (true, false) => self.write_cost,
            (false, false) => self.read_cost,
            (false, true) => return Err(Error::DoubleReadCharge(page)),
        };
        Ok(Self::sub_gas(gas_counter, amount))
    }

    pub fn charge_for_page_data_load(&mut self, gas_counter: &mut u64) -> Status {
        Self::sub_gas(gas_counter, self.load_data_cost)
    }
}

pub(crate) enum CostNo {
    SignalRead = 0,
    SignalWrite = 1,
    SignalWriteAfterRead = 2,
    HostFuncRead = 3,
    HostFuncWrite = 4,
    HostFuncWriteAfterRead = 5,
    LoadPageDataFromStorage = 6,
    Amount = 7,
}
