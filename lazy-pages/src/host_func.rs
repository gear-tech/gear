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

//! Host function call `pre_process_memory_accesses` support in lazy-pages.

use crate::{
    LAZY_PAGES_CONTEXT,
    common::{CostNo, Error, GasCharger, LazyPagesExecutionContext, LazyPagesRuntimeContext},
    pages::GearPage,
    process::{self, AccessHandler},
};
use gear_core::{self, memory::MemoryInterval};
use gear_lazy_pages_common::{ProcessAccessError, Status};
use std::collections::BTreeSet;

pub(crate) struct HostFuncAccessHandler<'a> {
    pub is_write: bool,
    pub gas_counter: &'a mut u64,
    pub gas_charger: GasCharger,
}

impl AccessHandler for HostFuncAccessHandler<'_> {
    type Pages = BTreeSet<GearPage>;
    type Output = Status;

    fn is_write(&self) -> bool {
        self.is_write
    }

    fn check_status_is_gas_exceeded() -> Result<(), Error> {
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

    fn last_page(pages: &Self::Pages) -> Option<GearPage> {
        pages.last().copied()
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

fn accesses_pages(
    ctx: &LazyPagesRuntimeContext,
    accesses: &[MemoryInterval],
    pages: &mut BTreeSet<GearPage>,
) -> Result<(), Error> {
    let page_size = GearPage::size(ctx);

    accesses
        .iter()
        .try_for_each(|access| -> Result<(), Error> {
            // Here we suppose zero byte access like one byte access, because
            // backend memory impl can access memory even in case access has size 0.
            let last_byte = access
                .offset
                .checked_add(access.size.saturating_sub(1))
                .ok_or(Error::OutOfWasmMemoryAccess)?;

            let start = (access.offset / page_size) * page_size;
            let end = (last_byte / page_size) * page_size;
            let mut offset = start;
            while offset <= end {
                pages.insert(GearPage::from_offset(ctx, offset));
                offset = match offset.checked_add(page_size) {
                    Some(next_offset) => next_offset,
                    None => break,
                }
            }
            Ok(())
        })?;
    Ok(())
}

pub fn pre_process_memory_accesses(
    reads: &[MemoryInterval],
    writes: &[MemoryInterval],
    gas_counter: &mut u64,
) -> Result<(), ProcessAccessError> {
    log::trace!("host func mem accesses: {reads:?} {writes:?}");
    LAZY_PAGES_CONTEXT
        .with(|ctx| {
            let mut ctx = ctx.borrow_mut();
            let (rt_ctx, exec_ctx) = ctx.contexts_mut()?;

            let gas_charger = {
                GasCharger {
                    read_cost: exec_ctx.cost(CostNo::HostFuncRead),
                    write_cost: exec_ctx.cost(CostNo::HostFuncWrite),
                    write_after_read_cost: exec_ctx.cost(CostNo::HostFuncWriteAfterRead),
                    load_data_cost: exec_ctx.cost(CostNo::LoadPageDataFromStorage),
                }
            };
            let mut status = Status::Normal;

            if !reads.is_empty() {
                let mut read_pages = BTreeSet::new();
                accesses_pages(rt_ctx, reads, &mut read_pages)?;

                status = process::process_lazy_pages(
                    rt_ctx,
                    exec_ctx,
                    HostFuncAccessHandler {
                        is_write: false,
                        gas_counter,
                        gas_charger: gas_charger.clone(),
                    },
                    read_pages,
                )?;
            }

            // Does not process write accesses if gas exceeded.
            if !matches!(status, Status::Normal) {
                return Ok(status);
            }

            if !writes.is_empty() {
                let mut write_pages = BTreeSet::new();
                accesses_pages(rt_ctx, writes, &mut write_pages)?;

                status = process::process_lazy_pages(
                    rt_ctx,
                    exec_ctx,
                    HostFuncAccessHandler {
                        is_write: true,
                        gas_counter,
                        gas_charger,
                    },
                    write_pages,
                )?;
            }

            Ok(status)
        })
        .map_err(|err| match err {
            Error::WasmMemAddrIsNotSet | Error::OutOfWasmMemoryAccess => {
                ProcessAccessError::OutOfBounds
            }
            err => {
                let err_msg = format!(
                    "pre_process_memory_accesses: unexpected error. \
                    Reads - {reads:?}, writes - {writes:?}, gas counter - {gas_counter}. Got error - {err}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            }
        })
        .map(|status| match status {
            Status::Normal => Ok(()),
            Status::GasLimitExceeded => Err(ProcessAccessError::GasLimitExceeded),
        })?
}
