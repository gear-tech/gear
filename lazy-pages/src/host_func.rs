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
    common::{Error, GasLeftCharger, LazyPagesExecutionContext, WeightNo},
    pages::{GearPageNumber, PageDynSize},
    process::{self, AccessHandler},
    LAZY_PAGES_CONTEXT,
};
use gear_backend_common::{lazy_pages::Status, memory::ProcessAccessError};
use gear_core::{gas::GasLeft, memory::MemoryInterval};
use std::collections::BTreeSet;

pub(crate) struct HostFuncAccessHandler<'a> {
    pub is_write: bool,
    pub gas_left: &'a mut GasLeft,
    pub gas_left_charger: GasLeftCharger,
}

impl<'a> AccessHandler for HostFuncAccessHandler<'a> {
    type Pages = BTreeSet<GearPageNumber>;
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
        page: GearPageNumber,
        is_accessed: bool,
    ) -> Result<Status, Error> {
        self.gas_left_charger.charge_for_page_access(
            self.gas_left,
            page,
            self.is_write,
            is_accessed,
        )
    }

    fn charge_for_page_data_loading(&mut self) -> Result<Status, Error> {
        Ok(self
            .gas_left_charger
            .charge_for_page_data_load(self.gas_left))
    }

    fn last_page(pages: &Self::Pages) -> Option<GearPageNumber> {
        pages.last().copied()
    }

    fn process_pages(
        pages: Self::Pages,
        mut process_one: impl FnMut(GearPageNumber) -> Result<(), Error>,
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
    ctx: &mut LazyPagesExecutionContext,
    accesses: &[MemoryInterval],
    pages: &mut BTreeSet<GearPageNumber>,
) -> Result<(), Error> {
    let page_size = GearPageNumber::size(ctx);

    accesses
        .iter()
        .try_for_each(|access| -> Result<(), Error> {
            // TODO: here we suppose zero byte access like one byte access, because
            // backend memory impl can access memory even in case access has size 0.
            // We can optimize this if will ignore zero bytes access in core-backend (issue #2095).
            let last_byte = access
                .offset
                .checked_add(access.size.saturating_sub(1))
                .ok_or_else(|| Error::OutOfWasmMemoryAccess)?;

            let start = (access.offset / page_size) * page_size;
            let end = (last_byte / page_size) * page_size;
            let mut offset = start;
            while offset <= end {
                pages.insert(GearPageNumber::from_offset(ctx, offset));
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
    gas_left: &mut GasLeft,
) -> Result<(), ProcessAccessError> {
    log::trace!("host func mem accesses: {reads:?} {writes:?}");
    LAZY_PAGES_CONTEXT
        .with(|ctx| {
            let mut ctx = ctx.borrow_mut();
            let ctx = ctx.execution_context_mut()?;

            let mut read_pages = BTreeSet::new();
            accesses_pages(ctx, reads, &mut read_pages)?;

            let gas_left_charger = {
                GasLeftCharger {
                    read_cost: ctx.weight(WeightNo::HostFuncRead),
                    write_cost: ctx.weight(WeightNo::HostFuncWrite),
                    write_after_read_cost: ctx.weight(WeightNo::HostFuncWriteAfterRead),
                    load_data_cost: ctx.weight(WeightNo::LoadPageDataFromStorage),
                }
            };

            let status = process::process_lazy_pages(
                ctx,
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

            let mut write_pages = BTreeSet::new();
            accesses_pages(ctx, writes, &mut write_pages)?;

            process::process_lazy_pages(
                ctx,
                HostFuncAccessHandler {
                    is_write: true,
                    gas_left,
                    gas_left_charger,
                },
                write_pages,
            )
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
            Status::GasAllowanceExceeded => Err(ProcessAccessError::GasAllowanceExceeded),
        })?
}
