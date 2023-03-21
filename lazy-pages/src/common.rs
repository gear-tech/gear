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

//! Lazy-pages structures for common usage.

use std::{collections::BTreeSet, mem::size_of, num::NonZeroU32};

use gear_backend_common::lazy_pages::{GlobalsAccessError, Status};
use gear_core::gas::GasLeft;

use crate::{
    globals::{GlobalNo, GlobalsContext},
    mprotect::MprotectError,
    pages::{GearPageNumber, PageDynSize, PageSizeNo, SizeManager, WasmPageNumber},
};

// TODO: investigate error allocations #2441
#[derive(Debug, derive_more::Display, derive_more::From)]
pub(crate) enum Error {
    #[display(fmt = "Accessed memory interval is out of wasm memory")]
    OutOfWasmMemoryAccess,
    #[display(fmt = "Signals cannot come from WASM program virtual stack memory")]
    SignalFromStackMemory,
    #[display(fmt = "Signals cannot come from write accessed page")]
    SignalFromWriteAccessedPage,
    #[display(fmt = "Read access signal cannot come from already accessed page")]
    ReadAccessSignalFromAccessedPage,
    #[display(fmt = "WASM memory begin address is not set")]
    WasmMemAddrIsNotSet,
    #[display(fmt = "Page data in storage must contain {expected} bytes, actually has {actual}")]
    InvalidPageDataSize { expected: u32, actual: u32 },
    #[display(fmt = "Any page cannot be write accessed twice: {_0:?}")]
    DoubleWriteAccess(GearPageNumber),
    #[display(fmt = "Any page cannot be read charged twice: {_0:?}")]
    DoubleReadCharge(GearPageNumber),
    #[display(fmt = "Memory protection error: {_0}")]
    #[from]
    MemoryProtection(MprotectError),
    #[display(fmt = "Given instance host pointer is invalid")]
    HostInstancePointerIsInvalid,
    #[display(fmt = "Given pointer to globals access provider dyn object is invalid")]
    DynGlobalsAccessPointerIsInvalid,
    #[display(fmt = "Something goes wrong when trying to access globals: {_0:?}")]
    #[from]
    AccessGlobal(GlobalsAccessError),
    #[display(fmt = "It's unknown wether memory access is read or write")]
    ReadOrWriteIsUnknown,
    #[display(fmt = "Cannot receive signal from wasm memory, when status is gas limit exceed")]
    SignalWhenStatusGasExceeded,
    #[from]
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
    pub fn execution_context_mut(
        &mut self,
    ) -> Result<&mut LazyPagesExecutionContext, ContextError> {
        self.execution_context
            .as_mut()
            .ok_or(ContextError::ExecutionContextIsNotSet)
    }
    pub fn set_runtime_context(&mut self, ctx: LazyPagesRuntimeContext) {
        self.runtime_context = Some(ctx);
    }
    pub fn set_execution_context(&mut self, ctx: LazyPagesExecutionContext) {
        self.execution_context = Some(ctx);
    }
}

pub(crate) type Weights = [u64; WeightNo::Amount as usize];
pub(crate) type PageSizes = [NonZeroU32; PageSizeNo::Amount as usize];
pub(crate) type GlobalNames = [String; GlobalNo::Amount as usize];

#[derive(Debug)]
pub(crate) struct LazyPagesRuntimeContext {
    pub page_sizes: PageSizes,
    pub global_names: GlobalNames,
    pub pages_storage_prefix: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct LazyPagesExecutionContext {
    pub page_sizes: PageSizes,
    /// Lazy-pages accesses weights.
    pub weights: Weights,
    /// Pointer to the begin of wasm memory buffer
    pub wasm_mem_addr: Option<usize>,
    /// Wasm memory buffer size, to identify whether signal is from wasm memory buffer.
    pub wasm_mem_size: WasmPageNumber,
    /// Current program prefix in storage
    pub program_storage_prefix: PagePrefix,
    /// Pages which has been accessed by program during current execution
    pub accessed_pages: BTreeSet<GearPageNumber>,
    /// Pages which has been write accessed by program during current execution
    pub write_accessed_pages: BTreeSet<GearPageNumber>,
    /// End of stack wasm address. Default is `0`, which means,
    /// that wasm data has no stack region. It's not necessary to specify
    /// this value, `lazy-pages` uses it to identify memory, for which we
    /// can skip processing and this memory won't be protected. So, pages
    /// which lies before this value will never get into `write_accessed_pages`,
    /// which means that they will never be updated in storage.
    pub stack_end: WasmPageNumber,
    /// Context to access globals and works with them: charge gas, set status global.
    pub globals_context: Option<GlobalsContext>,
    /// Lazy-pages status: indicates in which mod lazy-pages works actually.
    pub status: Status,
}

#[derive(Clone, Copy, Debug)]
pub enum LazyPagesVersion {
    Version1,
}

impl SizeManager for LazyPagesExecutionContext {
    fn size_non_zero<P: PageDynSize>(&self) -> NonZeroU32 {
        self.page_sizes[P::SIZE_NO]
    }
}

impl SizeManager for LazyPagesRuntimeContext {
    fn size_non_zero<P: PageDynSize>(&self) -> NonZeroU32 {
        self.page_sizes[P::SIZE_NO]
    }
}

impl LazyPagesExecutionContext {
    pub fn is_accessed(&self, page: GearPageNumber) -> bool {
        self.accessed_pages.contains(&page)
    }

    pub fn is_write_accessed(&self, page: GearPageNumber) -> bool {
        self.write_accessed_pages.contains(&page)
    }

    pub fn set_accessed(&mut self, page: GearPageNumber) {
        self.accessed_pages.insert(page);
    }

    pub fn set_write_accessed(&mut self, page: GearPageNumber) -> Result<(), Error> {
        self.set_accessed(page);
        match self.write_accessed_pages.insert(page) {
            true => Ok(()),
            false => Err(Error::DoubleWriteAccess(page)),
        }
    }

    pub fn key_for_page(&mut self, page: GearPageNumber) -> &[u8] {
        self.program_storage_prefix.calc_key_for_page(page)
    }

    pub fn page_has_data_in_storage(&mut self, page: GearPageNumber) -> bool {
        sp_io::storage::exists(self.key_for_page(page))
    }

    pub fn load_page_data_from_storage(
        &mut self,
        page: GearPageNumber,
        buffer: &mut [u8],
    ) -> Result<bool, Error> {
        if let Some(size) = sp_io::storage::read(self.key_for_page(page), buffer, 0) {
            if size != GearPageNumber::size(self) {
                return Err(Error::InvalidPageDataSize {
                    expected: GearPageNumber::size(self),
                    actual: size,
                });
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn weight(&self, no: WeightNo) -> u64 {
        self.weights[no as usize]
    }
}

/// Struct for fast calculation of page key in storage.
/// Key consists of two parts:
/// 1) current program prefix in storage
/// 2) page number in little endian bytes order
/// First part is always the same, so we can copy it to buffer
/// once and then use it for all pages.
#[derive(Debug)]
pub(crate) struct PagePrefix {
    buffer: Vec<u8>,
}

impl PagePrefix {
    /// New page prefix from program prefix
    pub fn new_from_program_prefix(mut program_prefix: Vec<u8>) -> Self {
        program_prefix.extend_from_slice(&u32::MAX.to_le_bytes());
        Self {
            buffer: program_prefix,
        }
    }

    /// Returns key in storage for `page`.
    fn calc_key_for_page(&mut self, page: GearPageNumber) -> &[u8] {
        let len = self.buffer.len();
        let page_no: u32 = page.into();
        self.buffer[len - size_of::<u32>()..len].copy_from_slice(page_no.to_le_bytes().as_slice());
        &self.buffer
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GasLeftCharger {
    pub read_cost: u64,
    pub write_cost: u64,
    pub write_after_read_cost: u64,
    pub load_data_cost: u64,
}

impl GasLeftCharger {
    fn sub_gas(gas_left: &mut GasLeft, amount: u64) -> Status {
        let new_gas = gas_left.gas.checked_sub(amount);
        let new_allowance = gas_left.allowance.checked_sub(amount);
        *gas_left = (
            new_gas.unwrap_or_default(),
            new_allowance.unwrap_or_default(),
        )
            .into();
        match (new_gas, new_allowance) {
            (None, _) => Status::GasLimitExceeded,
            (Some(_), None) => Status::GasAllowanceExceeded,
            (Some(_), Some(_)) => Status::Normal,
        }
    }

    pub fn charge_for_page_access(
        &self,
        gas_left: &mut GasLeft,
        page: GearPageNumber,
        is_write: bool,
        is_accessed: bool,
    ) -> Result<Status, Error> {
        let amount = match (is_write, is_accessed) {
            (true, true) => self.write_after_read_cost,
            (true, false) => self.write_cost,
            (false, false) => self.read_cost,
            (false, true) => return Err(Error::DoubleReadCharge(page)),
        };
        Ok(Self::sub_gas(gas_left, amount))
    }

    pub fn charge_for_page_data_load(&mut self, gas_left: &mut GasLeft) -> Status {
        Self::sub_gas(gas_left, self.load_data_cost)
    }
}

pub(crate) enum WeightNo {
    SignalRead = 0,
    SignalWrite = 1,
    SignalWriteAfterRead = 2,
    HostFuncRead = 3,
    HostFuncWrite = 4,
    HostFuncWriteAfterRead = 5,
    LoadPageDataFromStorage = 6,
    Amount = 7,
}
