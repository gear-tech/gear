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

use std::{
    cell::RefMut,
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU32,
};

use gear_backend_common::lazy_pages::{
    GlobalsAccessError, GlobalsConfig, LazyPagesWeights, Status,
};
use gear_core::{
    costs::CostPerPage,
    gas::GasLeft,
    memory::{
        GearPage, GranularityPage, PageU32Size, PagesIterInclusive, WasmPage, GEAR_PAGE_SIZE,
    },
};

use crate::mprotect::MprotectError;

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
    #[display(fmt = "Accessed memory interval is out of wasm memory")]
    OutOfWasmMemoryAccess,
    #[display(fmt = "Signals cannot come from WASM program virtual stack memory")]
    SignalFromStackMemory,
    #[display(fmt = "Signals cannot come from released page")]
    SignalFromReleasedPage,
    #[display(fmt = "Read access signal cannot come from already accessed page")]
    ReadAccessSignalFromAccessedPage,
    #[display(fmt = "WASM memory begin address is not set")]
    WasmMemAddrIsNotSet,
    #[display(fmt = "WASM memory size is not set")]
    WasmMemSizeIsNotSet,
    #[display(fmt = "Program pages prefix in storage is not set")]
    ProgramPrefixIsNotSet,
    #[display(fmt = "Page data in storage must contain {expected} bytes, actually has {actual}")]
    InvalidPageDataSize { expected: u32, actual: u32 },
    #[display(fmt = "Any page cannot be released twice: {_0:?}")]
    DoubleRelease(LazyPage),
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
    #[display(fmt = "Status must be set before program execution")]
    StatusIsNone,
    #[display(fmt = "It's unknown wether memory access is read or write")]
    ReadOrWriteIsUnknown,
    #[display(fmt = "Cannot receive signal from wasm memory, when status is gas limit exceed")]
    SignalWhenStatusGasExceeded,
}

#[derive(Clone, Copy)]
pub enum LazyPagesVersion {
    Version1,
}

#[derive(Default, Debug)]
pub(crate) struct LazyPagesExecutionContext {
    /// Pointer to the begin of wasm memory buffer
    pub wasm_mem_addr: Option<usize>,
    /// Wasm memory buffer size, to identify whether signal is from wasm memory buffer.
    pub wasm_mem_size: Option<WasmPage>,
    /// Current program prefix in storage
    program_storage_prefix: Option<PagePrefix>,
    /// Wasm addresses of lazy-pages, that have been read or write accessed at least once.
    /// Lazy page here is page, which has `size = max(native_page_size, gear_page_size)`.
    pub accessed_pages: BTreeSet<LazyPage>,
    /// Granularity pages, for which we have already charge gas for read.
    pub read_charged: BTreeSet<GranularityPage>,
    /// Granularity pages, for which we have already charge gas for write.
    pub write_charged: BTreeSet<GranularityPage>,
    /// Granularity pages, for which we have already charge gas for read after write.
    // pub write_after_read_charged: BTreeSet<GranularityPage>,
    /// Loading page data from storage cost.
    pub load_data_charged: BTreeSet<GranularityPage>,
    /// End of stack wasm address. Default is `0`, which means,
    /// that wasm data has no stack region. It's not necessary to specify
    /// this value, `lazy-pages` uses it to identify memory, for which we
    /// can skip processing and this memory won't be protected. So, pages
    /// which lies before this value will never get into `released_pages`,
    /// which means that they will never be updated in storage.
    pub stack_end_wasm_page: WasmPage,
    /// Gear pages, which has been write accessed.
    pub released_pages: BTreeSet<LazyPage>,
    /// Context to access globals and works with them: charge gas, set status global.
    pub globals_config: Option<GlobalsConfig>,
    /// Lazy-pages status: indicates in which mod lazy-pages works actually.
    pub status: Option<Status>,
    /// Lazy-pages accesses weights.
    pub lazy_pages_weights: LazyPagesWeights,
    /// Cache information about whether page has data in storage
    pub page_has_data_in_storage: BTreeMap<GranularityPage, bool>,
}

impl LazyPagesExecutionContext {
    pub fn is_read_charged(&self, page: GranularityPage) -> bool {
        self.read_charged.contains(&page)
    }

    pub fn is_write_charged(&self, page: GranularityPage) -> bool {
        self.write_charged.contains(&page)
    }

    pub fn set_read_charged(&mut self, page: GranularityPage) -> bool {
        if self.stack_end_wasm_page > page.to_page() {
            // is stack page
            return false;
        }
        match self.is_write_charged(page) {
            true => false,
            false => self.read_charged.insert(page),
        }
    }

    pub fn set_write_charged(&mut self, page: GranularityPage) -> bool {
        if self.stack_end_wasm_page > page.to_page() {
            // is stack page
            return false;
        }
        self.write_charged.insert(page)
    }

    pub fn set_load_data_charged(&mut self, page: GranularityPage) -> bool {
        self.load_data_charged.insert(page)
    }

    pub fn add_to_released(&mut self, page: LazyPage) -> Result<(), Error> {
        match self.released_pages.insert(page) {
            true => Ok(()),
            false => Err(Error::DoubleRelease(page)),
        }
    }

    pub fn set_program_prefix(&mut self, prefix: Vec<u8>) {
        self.program_storage_prefix = Some(PagePrefix::new_from_program_prefix(prefix));
    }

    pub fn get_key_for_page(&mut self, page: GearPage) -> Result<&[u8], Error> {
        self.program_storage_prefix
            .as_mut()
            .map(|prefix| prefix.calc_key_for_page(page))
            .ok_or(Error::ProgramPrefixIsNotSet)
    }

    pub fn page_has_data_in_storage(&mut self, page: GearPage) -> Result<bool, Error> {
        if let Some(&res) = self.page_has_data_in_storage.get(&page.to_page()) {
            return Ok(res);
        }
        let page_key_exists = sp_io::storage::exists(self.get_key_for_page(page)?);
        self.page_has_data_in_storage
            .insert(page.to_page(), page_key_exists);
        Ok(page_key_exists)
    }

    pub fn load_page_data_from_storage(
        &mut self,
        page: GearPage,
        buffer: &mut [u8],
    ) -> Result<(), Error> {
        if let Some(size) = sp_io::storage::read(self.get_key_for_page(page)?, buffer, 0) {
            self.page_has_data_in_storage.insert(page.to_page(), true);
            if size != GearPage::size() {
                return Err(Error::InvalidPageDataSize {
                    expected: GearPage::size(),
                    actual: size,
                });
            }
        } else {
            self.page_has_data_in_storage.insert(page.to_page(), false);
        }
        Ok(())
    }

    pub fn handle_psg_case_one_page(
        &mut self,
        page: LazyPage,
    ) -> Result<PagesIterInclusive<LazyPage>, Error> {
        if self.page_has_data_in_storage(page.to_page())? {
            // if at least one gear page has data in storage, then all pages from corresponding
            // `GranularityPage` have data in storage, therefor all gear pages from `page`
            // have data in storage. So, this is not psg case and we can handle only one `LazyPage`.
            Ok(page.iter_once())
        } else {
            // no pages in storage - this is psg case.
            Ok(page.to_page::<GranularityPage>().to_pages_iter())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub struct LazyPage(u32);

impl PageU32Size for LazyPage {
    fn size_non_zero() -> NonZeroU32 {
        static_assertions::const_assert_ne!(GEAR_PAGE_SIZE, 0);
        unsafe { NonZeroU32::new_unchecked(region::page::size().max(GEAR_PAGE_SIZE) as u32) }
    }

    fn raw(&self) -> u32 {
        self.0
    }

    unsafe fn new_unchecked(num: u32) -> Self {
        Self(num)
    }
}

/// Struct for fast calculation of page key in storage.
/// Key consists of two parts:
/// 1) current program prefix in storage
/// 2) page number in little endian bytes order
/// First part is always the same, so we can copy it to buffer
/// once and then use it for all pages.
#[derive(Debug)]
struct PagePrefix {
    buffer: Vec<u8>,
}

impl PagePrefix {
    /// New page prefix from program prefix
    fn new_from_program_prefix(mut program_prefix: Vec<u8>) -> Self {
        program_prefix.extend_from_slice(&u32::MAX.to_le_bytes());
        Self {
            buffer: program_prefix,
        }
    }

    /// Returns key in storage for `page`.
    fn calc_key_for_page(&mut self, page: GearPage) -> &[u8] {
        let len = self.buffer.len();
        self.buffer[len - std::mem::size_of::<u32>()..len]
            .copy_from_slice(page.raw().to_le_bytes().as_slice());
        &self.buffer
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GasLeftCharger {
    pub read_cost: CostPerPage<GranularityPage>,
    pub write_cost: CostPerPage<GranularityPage>,
    pub write_after_read_cost: CostPerPage<GranularityPage>,
    pub load_data_cost: CostPerPage<GranularityPage>,
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

    pub fn charge_for_pages(
        &self,
        gas_left: &mut GasLeft,
        ctx: &mut RefMut<LazyPagesExecutionContext>,
        pages: PagesIterInclusive<LazyPage>,
        is_write: bool,
    ) -> Result<Status, Error> {
        let for_write = |ctx: &mut RefMut<LazyPagesExecutionContext>, page| {
            if ctx.set_write_charged(page) {
                if ctx.is_read_charged(page) {
                    self.write_after_read_cost.one()
                } else {
                    self.write_cost.one()
                }
            } else {
                0
            }
        };

        let for_read = |ctx: &mut RefMut<LazyPagesExecutionContext>, page| {
            if ctx.set_read_charged(page) {
                self.read_cost.one()
            } else {
                0
            }
        };

        let mut amount = 0u64;
        for page in pages.convert() {
            let amount_for_page = if is_write {
                for_write(ctx, page)
            } else {
                for_read(ctx, page)
            };
            amount = amount.saturating_add(amount_for_page);
        }

        Ok(Self::sub_gas(gas_left, amount))
    }

    pub fn charge_for_page_data_load(
        &mut self,
        gas_left: &mut GasLeft,
        ctx: &mut RefMut<LazyPagesExecutionContext>,
        page: GranularityPage,
    ) -> Result<Status, Error> {
        if ctx.set_load_data_charged(page) {
            Ok(Self::sub_gas(gas_left, self.load_data_cost.one()))
        } else {
            Ok(Status::Normal)
        }
    }
}
