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

//! Host function call `pre_process_memory_accesses` support in lazy-pages.

use crate::{
    common::{Error, GasCharger, LazyPagesExecutionContext, WeightNo},
    process::{self, AccessHandler},
    LAZY_PAGES_CONTEXT,
};
use gear_core::{
    self,
    memory::MemoryInterval,
    pages::{GearPage, PageDynSize, PageNumber},
};
use gear_lazy_pages_common::{ProcessAccessError, Status};
use std::collections::BTreeSet;
use std::ops::RangeInclusive;

pub struct MemoryIntervalPageIterator<'a> {
    intervals: &'a mut dyn Iterator<Item = MemoryInterval>,
    page_size: u32,
    current_pages: Option<RangeInclusive<u32>>,
}

impl<'a> MemoryIntervalPageIterator<'a> {
    fn new(intervals: &'a mut impl Iterator<Item = MemoryInterval>, page_size: u32) -> Self {
        let current_pages = Self::calculate_current_pages(intervals.next(), page_size);

        Self {
            intervals,
            page_size,
            current_pages,
        }
    }

    fn calculate_current_pages(
        interval: Option<MemoryInterval>,
        page_size: u32,
    ) -> Option<RangeInclusive<u32>> {
        interval.map(|interval| {
            let last_byte = interval
                .offset
                .saturating_add(interval.size)
                .saturating_sub(1);
            let start_page = interval.offset / page_size;
            let end_page = last_byte / page_size;

            start_page..=end_page
        })
    }
}

impl<'a> Iterator for MemoryIntervalPageIterator<'a> {
    type Item = GearPage;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(pages) = self.current_pages.as_mut() {
            if let Some(page_number) = pages.next() {
                return Some(unsafe { GearPage::from_raw(page_number) });
            } else {
                self.current_pages =
                    Self::calculate_current_pages(self.intervals.next(), self.page_size);
            }
        }
        None
    }
}

pub(crate) struct HostFuncAccessHandler<'a> {
    pub is_write: bool,
    pub gas_counter: &'a mut u64,
    pub gas_charger: &'a mut GasCharger,
}

impl<'a> AccessHandler for HostFuncAccessHandler<'a> {
    type Pages = MemoryIntervalPageIterator<'a>;
    type Output = Status;

    fn is_write(&self) -> bool {
        self.is_write
    }

    fn check_status_is_gas_exceeded() -> Result<(), Error> {
        // Currently, we charge gas for sys-call after memory processing, so this can appear.
        // In this case we do nothing, because all memory is already unprotected, and no need
        // to take in account pages data from storage, because gas is exceeded.
        Ok(())
    }

    fn check_stack_memory_access() -> Result<(), Error> {
        Ok(())
    }

    fn check_write_accessed_memory_access() -> Result<(), Error> {
        Ok(())
    }

    fn check_read_from_accessed_memory() -> Result<(), Error> {
        Ok(())
    }

    fn charge_for_page_access(
        &mut self,
        page: GearPage,
        is_accessed: bool,
    ) -> Result<Status, Error> {
        self.gas_charger
            .charge_for_page_access(self.gas_counter, page, self.is_write, is_accessed)
    }

    fn charge_for_page_data_loading(&mut self) -> Result<Status, Error> {
        Ok(self.gas_charger.charge_for_page_data_load(self.gas_counter))
    }

    fn process_pages(
        mut pages: Self::Pages,
        mut process_one: impl FnMut(GearPage) -> Result<(), Error>,
    ) -> Result<(), Error> {
        pages.try_for_each(|page| -> Result<(), Error> {
            log::trace!("process_pages: {page:?}");
            process_one(page)?;
            Ok(())
        })
    }

    fn into_output(self, ctx: &mut LazyPagesExecutionContext) -> Result<Self::Output, Error> {
        Ok(ctx.status)
    }
}

fn process_memory_intervals(
    ctx: &mut LazyPagesExecutionContext,
    intervals: &mut impl Iterator<Item = MemoryInterval>,
    gas_counter: &mut u64,
    gas_charger: &mut GasCharger,
    is_write: bool,
) -> Result<Status, Error> {
    let page_size = GearPage::size(ctx);

    let page_iter = MemoryIntervalPageIterator::new(intervals, page_size);

    process::process_lazy_pages(
        ctx,
        HostFuncAccessHandler {
            is_write,
            gas_counter,
            gas_charger,
        },
        page_iter,
        page_size,
    )
}

pub fn pre_process_memory_accesses(
    reads: &mut (impl Iterator<Item = MemoryInterval> + std::fmt::Debug),
    writes: &mut (impl Iterator<Item = MemoryInterval> + std::fmt::Debug),
    gas_counter: &mut u64,
) -> Result<(), ProcessAccessError> {
    log::trace!("host func mem accesses: {reads:?} {writes:?}");
    LAZY_PAGES_CONTEXT
        .with(|ctx| {
            let mut ctx = ctx.borrow_mut();
            let ctx = ctx.execution_context_mut()?;

            let mut gas_charger = GasCharger {
                read_cost: ctx.weight(WeightNo::HostFuncRead),
                write_cost: ctx.weight(WeightNo::HostFuncWrite),
                write_after_read_cost: ctx.weight(WeightNo::HostFuncWriteAfterRead),
                load_data_cost: ctx.weight(WeightNo::LoadPageDataFromStorage),
            };

            // Process reads
            let mut status =
                process_memory_intervals(ctx, reads, gas_counter, &mut gas_charger, false)?;

            // Does not process write accesses if gas exceeded.
            if !matches!(status, Status::Normal) {
                return Ok(status);
            }

            // Process writes
            status = process_memory_intervals(ctx, writes, gas_counter, &mut gas_charger, true)?;

            Ok(status)
        })
        .map_err(|err| match err {
            Error::WasmMemAddrIsNotSet | Error::OutOfWasmMemoryAccess => {
                ProcessAccessError::OutOfBounds
            }
            err => unreachable!("Lazy-pages unexpected error: {}", err),
        })
        .map(|status| match status {
            Status::Normal => Ok(()),
            Status::GasLimitExceeded => Err(ProcessAccessError::GasLimitExceeded),
        })?
}
