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

use crate::configs::{AllocationsConfig, BlockInfo};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use core::fmt;
use gear_core::{
    costs::HostFnWeights,
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    ids::{CodeId, MessageId, ProgramId},
    memory::{AllocationsContext, PageBuf, PageNumber},
    message::MessageContext,
};
use gear_core_errors::{ExtError, MemoryError, TerminationReason};

/// Trait to which ext must have to work in processor wasm executor.
/// Currently used only for lazy-pages support.
pub trait ProcessorExt {
    /// An error issues in processor
    type Error: fmt::Display;

    /// Create new
    #[allow(clippy::too_many_arguments)]
    fn new(
        gas_counter: GasCounter,
        gas_allowance_counter: GasAllowanceCounter,
        value_counter: ValueCounter,
        allocations_context: AllocationsContext,
        message_context: MessageContext,
        block_info: BlockInfo,
        config: AllocationsConfig,
        existential_deposit: u128,
        exit_argument: Option<ProgramId>,
        origin: ProgramId,
        program_id: ProgramId,
        program_candidates_data: BTreeMap<CodeId, Vec<(ProgramId, MessageId)>>,
        host_fn_weights: HostFnWeights,
    ) -> Self;

    /// Returns whether this extension works with lazy pages
    fn is_lazy_pages_enabled() -> bool;

    /// If extention support lazy pages, then checks that
    /// environment for lazy pages is initialized.
    fn check_lazy_pages_consistent_state() -> bool;

    /// Protect and save storage keys for pages which has no data
    fn lazy_pages_protect_and_init_info(
        mem: &dyn Memory,
        lazy_pages: &BTreeSet<PageNumber>,
        prog_id: ProgramId,
    ) -> Result<(), Self::Error>;

    /// Lazy pages contract post execution actions
    fn lazy_pages_post_execution_actions(
        mem: &dyn Memory,
        memory_pages: &mut BTreeMap<PageNumber, PageBuf>,
    ) -> Result<(), Self::Error>;
}
