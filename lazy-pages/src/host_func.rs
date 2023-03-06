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

use std::{cell::RefMut, collections::BTreeSet};

use gear_backend_common::{lazy_pages::Status, memory::ProcessAccessError};
use gear_core::{
    gas::GasLeft,
    memory::{GranularityPage, MemoryInterval, PageU32Size, PagesIterInclusive},
};

use crate::{
    common::{Error, GasLeftCharger, LazyPage, LazyPagesExecutionContext},
    process::{self, AccessHandler},
    utils::{self, handle_psg_case_one_page},
    LAZY_PAGES_PROGRAM_CONTEXT,
};

pub(crate) struct HostFuncAccessHandler<'a> {
    pub is_write: bool,
    pub gas_left: &'a mut GasLeft,
    pub gas_left_charger: GasLeftCharger,
}

impl<'a> AccessHandler for HostFuncAccessHandler<'a> {
    type Pages = BTreeSet<LazyPage>;
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

    fn check_released_memory_access() -> Result<(), Error> {
        Ok(())
    }

    fn check_read_from_accessed_memory() -> Result<(), Error> {
        Ok(())
    }

    fn charge_for_pages(
        &mut self,
        ctx: &mut RefMut<LazyPagesExecutionContext>,
        pages: PagesIterInclusive<LazyPage>,
    ) -> Result<Status, Error> {
        self.gas_left_charger
            .charge_for_pages(self.gas_left, ctx, pages, self.is_write)
    }

    fn charge_for_data_loading(
        &mut self,
        ctx: &mut RefMut<LazyPagesExecutionContext>,
        page: GranularityPage,
    ) -> Result<Status, Error> {
        self.gas_left_charger.charge_for_page_data_load(self.gas_left, ctx, page)
    }

    fn last_page(pages: &Self::Pages) -> Option<LazyPage> {
        pages.last().copied()
    }

    fn apply_for_pages(
        pages: Self::Pages,
        f: impl FnMut(PagesIterInclusive<LazyPage>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        utils::with_inclusive_ranges(&pages, f)
    }

    fn into_output(
        self,
        ctx: &mut RefMut<LazyPagesExecutionContext>,
    ) -> Result<Self::Output, Error> {
        ctx.status.ok_or(Error::StatusIsNone)
    }
}

fn get_access_pages(accesses: &[MemoryInterval]) -> Result<BTreeSet<LazyPage>, Error> {
    let mut set = BTreeSet::new();
    for access in accesses {
        let start = LazyPage::from_offset(access.offset);
        // TODO: here we suppose zero byte access like one byte access, because
        // backend memory impl can access memory even in case access has size 0.
        // We can optimize this if will ignore zero bytes access in core-backend (issue #2095).
        let byte_after_last = access
            .offset
            .checked_add(access.size.saturating_sub(1))
            .ok_or(Error::OutOfWasmMemoryAccess)?;
        let end = LazyPage::from_offset(byte_after_last);
        let iter = start
            .iter_end_inclusive(end)
            .unwrap_or_else(|err| unreachable!("`start` page is bigger than `end` page: {}", err));
        set.extend(iter);
    }
    Ok(set)
}

fn handle_psg_case(
    ctx: &mut RefMut<LazyPagesExecutionContext>,
    pages: BTreeSet<LazyPage>,
) -> Result<BTreeSet<LazyPage>, Error> {
    let mut res = pages.clone();
    let mut granularity_page: Option<GranularityPage> = None;

    for page in pages {
        if let Some(granularity_page) = granularity_page {
            if granularity_page == page.to_page() {
                continue;
            }
        }
        let psg_pages = handle_psg_case_one_page(ctx, page)?;
        res.extend(psg_pages);
        granularity_page = Some(page.to_page());
    }

    Ok(res)
}

pub fn pre_process_memory_accesses(
    reads: &[MemoryInterval],
    writes: &[MemoryInterval],
    gas_left: &mut GasLeft,
) -> Result<(), ProcessAccessError> {
    log::trace!("host func mem accesses: {reads:?} {writes:?}");
    LAZY_PAGES_PROGRAM_CONTEXT
        .with(|ctx| unsafe {
            let read_pages = get_access_pages(reads)?;
            let write_pages = handle_psg_case(&mut ctx.borrow_mut(), get_access_pages(writes)?)?;

            let gas_left_charger = {
                let ctx = ctx.borrow();
                GasLeftCharger {
                    read_cost: ctx.lazy_pages_weights.host_func_read,
                    write_cost: ctx.lazy_pages_weights.host_func_write,
                    write_after_read_cost: ctx.lazy_pages_weights.host_func_write_after_read,
                    load_data_cost: ctx.lazy_pages_weights.load_page_storage_data,
                }
            };

            let status = process::process_lazy_pages(
                ctx.borrow_mut(),
                HostFuncAccessHandler {
                    is_write: false,
                    gas_left,
                    gas_left_charger: gas_left_charger.clone(),
                },
                read_pages,
            )?;

            // Does not process write accesses if gas exceeded.
            if !matches!(status, Status::Normal) {
                return Ok(status);
            }

            process::process_lazy_pages(
                ctx.borrow_mut(),
                HostFuncAccessHandler {
                    is_write: true,
                    gas_left,
                    gas_left_charger,
                },
                write_pages,
            )
        })
        .map_err(|err| match err {
            Error::OutOfWasmMemoryAccess | Error::WasmMemSizeIsNotSet => {
                ProcessAccessError::OutOfBounds
            }
            err => unreachable!("Lazy-pages unexpected error: {}", err),
        })
        .map(|status| match status {
            Status::Normal => Ok(()),
            Status::GasLimitExceeded => Err(ProcessAccessError::GasLimitExceeded),
            Status::GasAllowanceExceeded => Err(ProcessAccessError::GasAllowanceExceeded),
        })?
}
