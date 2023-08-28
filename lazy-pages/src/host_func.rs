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
use gear_backend_common::{lazy_pages::Status, memory::ProcessAccessError};
use gear_core::{
    self,
    memory::MemoryInterval,
    pages::{GearPage, PageDynSize},
};
use std::collections::BTreeSet;

pub(crate) struct HostFuncAccessHandler<'a> {
    pub is_write: bool,
    pub gas_counter: &'a mut u64,
    pub gas_charger: &'a mut GasCharger,
}

impl<'a> AccessHandler for HostFuncAccessHandler<'a> {
    type Pages = BTreeSet<GearPage>;
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
        pages: Self::Pages,
        mut process_one: impl FnMut(GearPage) -> Result<(), Error>,
    ) -> Result<(), Error> {
        pages.iter().try_for_each(|page| -> Result<(), Error> {
            process_one(*page)?;
            Ok(())
        })
    }

    fn into_output(self, ctx: &mut LazyPagesExecutionContext) -> Result<Self::Output, Error> {
        Ok(ctx.status)
    }
}

fn process_memory_intervals(
    ctx: &mut LazyPagesExecutionContext,
    intervals: impl Iterator<Item = MemoryInterval>,
    gas_counter: &mut u64,
    gas_charger: &mut GasCharger,
    is_write: bool,
) -> Result<Status, Error> {
    let page_size = GearPage::size(ctx);

    let mut pages = BTreeSet::new();
    let mut last_offset = None;

    for interval in intervals {
        if last_offset == Some(interval.offset) {
            // Skip duplicates
            continue;
        }
        last_offset = Some(interval.offset);

        // Process memory interval
        let last_byte = interval
            .offset
            .saturating_add(interval.size)
            .saturating_sub(1);
        let start = (interval.offset / page_size) * page_size;
        let end = (last_byte / page_size) * page_size;

        let mut offset = start;
        while offset <= end {
            pages.insert(GearPage::from_offset(ctx, offset));
            match offset.checked_add(page_size) {
                Some(next_offset) => offset = next_offset,
                None => break,
            }
        }
    }

    process::process_lazy_pages(
        ctx,
        HostFuncAccessHandler {
            is_write,
            gas_counter,
            gas_charger,
        },
        pages,
    )
}

pub fn pre_process_memory_accesses(
    reads: impl Iterator<Item = MemoryInterval> + std::fmt::Debug,
    writes: impl Iterator<Item = MemoryInterval> + std::fmt::Debug,
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
