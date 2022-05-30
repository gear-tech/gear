// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use ::common::Origin;
use alloc::collections::BTreeSet;
use common::{lazy_pages, save_page_lazy_info};
use core::fmt;
use core_processor::{
    configs::{AllocationsConfig, BlockInfo},
    ProcessorExt,
};
use gear_backend_common::{ExtInfo, IntoExtInfo};
use gear_core::{
    costs::{HostFnWeights, RuntimeCosts},
    env::Ext as EnvExt,
    gas::{ChargeResult, GasAllowanceCounter, GasAmount, GasCounter, ValueCounter},
    ids::{CodeId, MessageId, ProgramId},
    memory::{AllocationsContext, Memory, PageBuf, PageNumber, WasmPageNumber},
    message::{HandlePacket, InitPacket, MessageContext, ReplyPacket},
};
use gear_core_errors::{CoreError, ExtError, MemoryError, TerminationReason};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Core(ExtError),
    LazyPages(lazy_pages::Error),
}

impl CoreError for Error {
    fn from_termination_reason(reason: TerminationReason) -> Self {
        Self::Core(ExtError::from_termination_reason(reason))
    }

    fn as_termination_reason(&self) -> Option<TerminationReason> {
        match self {
            Self::Core(err) => err.as_termination_reason(),
            _ => None,
        }
    }
}

impl From<ExtError> for Error {
    fn from(err: ExtError) -> Self {
        Self::Core(err)
    }
}

impl From<lazy_pages::Error> for Error {
    fn from(err: lazy_pages::Error) -> Self {
        Self::LazyPages(err)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Core(err) => fmt::Display::fmt(err, f),
            Error::LazyPages(err) => fmt::Display::fmt(err, f),
        }
    }
}

/// Structure providing externalities for running host functions.
pub struct Ext {
    /// Gas counter.
    pub gas_counter: GasCounter,
    /// Gas allowance counter.
    pub gas_allowance_counter: GasAllowanceCounter,
    /// Value counter.
    pub value_counter: ValueCounter,
    /// Allocations context.
    pub allocations_context: AllocationsContext,
    /// Message context.
    pub message_context: MessageContext,
    /// Block info.
    pub block_info: BlockInfo,
    /// Allocations config.
    pub config: AllocationsConfig,
    /// Account existential deposit
    pub existential_deposit: u128,
    /// Any guest code panic explanation, if available.
    pub error_explanation: Option<ExtError>,
    /// Contains argument to the `exit` if it was called.
    pub exit_argument: Option<ProgramId>,
    /// Communication origin
    pub origin: ProgramId,
    /// Current program id
    pub program_id: ProgramId,
    /// Map of code hashes to program ids of future programs, which are planned to be
    /// initialized with the corresponding code (with the same code hash).
    pub program_candidates_data: BTreeMap<CodeId, Vec<(ProgramId, MessageId)>>,
    /// Weights of host functions.
    pub host_fn_weights: HostFnWeights,
    pub lazy_pages_enabled: bool,
    // Pages which has been alloced during current execution
    pub fresh_allocations: BTreeSet<WasmPageNumber>,
}

/// Empty implementation for non-substrate (and non-lazy-pages) using
impl ProcessorExt for Ext {
    type Error = Error;

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
    ) -> Self {
        Self {
            gas_counter,
            gas_allowance_counter,
            value_counter,
            allocations_context,
            message_context,
            block_info,
            config,
            existential_deposit,
            error_explanation: None,
            exit_argument,
            origin,
            program_id,
            program_candidates_data,
            host_fn_weights,
            lazy_pages_enabled: cfg!(feature = "lazy-pages")
                && lazy_pages::try_to_enable_lazy_pages(),
            fresh_allocations: Default::default(),
        }
    }

    fn is_lazy_pages_enabled() -> bool {
        cfg!(feature = "lazy-pages") && lazy_pages::try_to_enable_lazy_pages()
    }

    fn check_lazy_pages_consistent_state() -> bool {
        cfg!(feature = "lazy-pages") && lazy_pages::is_lazy_pages_enabled()
    }

    fn lazy_pages_protect_and_init_info(
        mem: &dyn Memory,
        memory_pages: &BTreeSet<PageNumber>,
        prog_id: ProgramId,
    ) -> Result<(), Self::Error> {
        if cfg!(feature = "lazy-pages") {
            lazy_pages::protect_pages_and_init_info(memory_pages, prog_id, wasm_mem_begin_addr)
                .map_err(Error::LazyPages)
        } else {
            unreachable!()
        }
    }

    fn lazy_pages_post_execution_actions(
        mem: &dyn Memory,
        memory_pages: &mut BTreeMap<PageNumber, PageBuf>,
    ) -> Result<(), Self::Error> {
        if cfg!(feature = "lazy-pages") {
            lazy_pages::post_execution_actions(memory_pages, wasm_mem_begin_addr)
                .map_err(Error::LazyPages)
        } else {
            unreachable!()
        }
    }
}

impl IntoExtInfo for Ext {
    fn into_ext_info<F: FnMut(usize, &mut [u8]) -> Result<(), T>, T>(
        self,
        mut get_page_data: F,
    ) -> Result<ExtInfo, (T, GasAmount)> {
        let allocations = self.allocations_context.allocations().clone();
        let pages_data = if self.lazy_pages_enabled {
            // accessed pages are all pages except current lazy pages
            let mut accessed_pages: BTreeSet<PageNumber> = allocations
                .iter()
                .flat_map(|p| p.to_gear_pages_iter())
                .collect();
            let lazy_pages_numbers = lazy_pages::get_lazy_pages_numbers();
            lazy_pages_numbers.into_iter().for_each(|p| {
                accessed_pages.remove(&p);
            });

            log::trace!("accessed pages numbers = {:?}", accessed_pages);

            let mut accessed_pages_data = BTreeMap::new();
            for page in accessed_pages {
                let mut buf = PageBuf::new_zeroed();
                if let Err(err) = get_page_data(page.offset(), buf.as_mut_slice()) {
                    return Err((err, self.gas_counter.into()));
                }
                accessed_pages_data.insert(page, buf);
            }
            accessed_pages_data
        } else {
            let mut pages_data = BTreeMap::new();
            for page in allocations.iter().flat_map(|p| p.to_gear_pages_iter()) {
                let mut buf = PageBuf::new_zeroed();
                if let Err(err) = get_page_data(page.offset(), buf.as_mut_slice()) {
                    return Err((err, self.gas_counter.into()));
                }
                pages_data.insert(page, buf);
            }
            pages_data
        };

        let (outcome, context_store) = self.message_context.drain();
        let (generated_dispatches, awakening) = outcome.drain();

        Ok(ExtInfo {
            gas_amount: self.gas_counter.into(),
            allocations,
            pages_data,
            generated_dispatches,
            awakening,
            context_store,
            trap_explanation: self.error_explanation,
            exit_argument: self.exit_argument,
            program_candidates_data: self.program_candidates_data,
        })
    }

    fn into_gas_amount(self) -> GasAmount {
        self.gas_counter.into()
    }
}

impl EnvExt for Ext {
    type Error = Error;

    fn alloc(
        &mut self,
        pages_num: WasmPageNumber,
        mem: &mut dyn Memory,
    ) -> Result<WasmPageNumber, Self::Error> {
        // Greedily charge gas for allocations
        self.charge_gas(pages_num.0.saturating_mul(self.config.alloc_cost as u32))?;
        // Greedily charge gas for grow
        self.charge_gas(pages_num.0.saturating_mul(self.config.mem_grow_cost as u32))?;

        let old_mem_size = mem.size();



        // Returns back greedily used gas for grow
        let new_mem_size = mem.size();
        let grow_pages_num = new_mem_size - old_mem_size;
        let mut gas_to_return_back = self
            .config
            .mem_grow_cost
            .saturating_mul((pages_num - grow_pages_num).0 as u64);

        // Returns back greedily used gas for allocations
        let first_page = page_number;
        let last_page = first_page + pages_num - 1.into();
        let mut new_allocated_pages_num = WasmPageNumber(0);
        for page in first_page.0..=last_page.0 {
            if !self.allocations_context.is_init_page(page.into()) {
                new_allocated_pages_num = new_allocated_pages_num + 1.into();
            }
        }
        gas_to_return_back = gas_to_return_back.saturating_add(
            self.config
                .alloc_cost
                .saturating_mul((pages_num - new_allocated_pages_num).0 as u64),
        );

        self.refund_gas(gas_to_return_back as u32)?;

        Ok(page_number)
    }

    fn block_height(&mut self) -> Result<u32, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::BlockHeight)?;
        Ok(self.block_info.height)
    }

    fn block_timestamp(&mut self) -> Result<u64, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::BlockTimestamp)?;
        Ok(self.block_info.timestamp)
    }

    fn origin(&mut self) -> Result<gear_core::ids::ProgramId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Origin)?;
        Ok(self.origin)
    }

    fn send_commit(&mut self, handle: usize, msg: HandlePacket) -> Result<MessageId, Self::Error> {
        if 0 < msg.value() && msg.value() < self.existential_deposit {
            return self.return_and_store_err(Err(ExtError::InsufficientMessageValue));
        };

        self.charge_gas_runtime(RuntimeCosts::SendCommit(msg.payload().len() as u32))?;

        if self.gas_counter.reduce(msg.gas_limit().unwrap_or(0)) != ChargeResult::Enough {
            return self.return_and_store_err(Err(ExtError::GasLimitExceeded));
        };

        if self.value_counter.reduce(msg.value()) != ChargeResult::Enough {
            return self.return_and_store_err(Err(ExtError::NotEnoughValue));
        };

        let result = self
            .message_context()
            .send_commit(handle as u32, msg)
            .map_err(ExtError::Message);

        self.return_and_store_err(result)
    }

    fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Reply(msg.payload().len() as u32))?;
        if 0 < msg.value() && msg.value() < self.existential_deposit {
            return self.return_and_store_err(Err(ExtError::InsufficientMessageValue));
        };

        if self.value_counter.reduce(msg.value()) != ChargeResult::Enough {
            return self.return_and_store_err(Err(ExtError::NotEnoughValue));
        };

        let result = self
            .message_context()
            .reply_commit(msg)
            .map_err(ExtError::Message);

        self.return_and_store_err(result)
    }

    fn exit(&mut self, value_destination: ProgramId) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Exit)?;
        if self.exit_argument.is_some() {
            Err(Error::Core(ExtError::ExitTwice))
        } else {
            self.exit_argument = Some(value_destination);
            Ok(())
        }
    }

    fn program_id(&mut self) -> Result<gear_core::ids::ProgramId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::ProgramId)?;
        Ok(self.program_id)
    }

    fn free(&mut self, page: WasmPageNumber) -> Result<(), Self::Error> {
        let result = self
            .allocations_context()
            .free(page)
            .map_err(ExtError::Free);

        // Returns back gas for allocated page if it's new
        if !self.allocations_context().is_init_page(page) {
            self.refund_gas(self.config.alloc_cost as u32)?;
        }

        self.return_and_store_err(result)
    }

    fn debug(&mut self, data: &str) -> Result<(), Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Debug)?;

        if data.starts_with("panic occurred") {
            self.error_explanation = Some(ExtError::PanicOccurred);
        }
        log::debug!(target: "gwasm", "DEBUG: {}", data);

        Ok(())
    }

    fn create_program(&mut self, packet: InitPacket) -> Result<ProgramId, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::CreateProgram)?;

        // Sending value should apply the range {0} ∪ [existential_deposit; +inf)
        if 0 < packet.value() && packet.value() < self.existential_deposit {
            return self.return_and_store_err(Err(ExtError::InsufficientMessageValue));
        };
        // Charge for using expiring resources. Charge for calling sys-call was done earlier.
        if self.gas_counter.reduce(packet.gas_limit().unwrap_or(0)) != ChargeResult::Enough {
            return self.return_and_store_err(Err(ExtError::GasLimitExceeded));
        };
        if self.value_counter.reduce(packet.value()) != ChargeResult::Enough {
            return self.return_and_store_err(Err(ExtError::NotEnoughValue));
        };

        let code_hash = packet.code_id();

        // Send a message for program creation
        let result = self
            .message_context()
            .init_program(packet)
            .map(|(new_prog_id, init_msg_id)| {
                // Save a program candidate for this run
                let entry = self.program_candidates_data.entry(code_hash).or_default();
                entry.push((new_prog_id, init_msg_id));

                new_prog_id
            })
            .map_err(ExtError::InitMessageNotDuplicated);

        self.return_and_store_err(result)
    }

    fn gas_counter(&mut self) -> &mut GasCounter {
        &mut self.gas_counter
    }

    fn gas_allowance_counter(&mut self) -> &mut GasAllowanceCounter {
        &mut self.gas_allowance_counter
    }

    fn value_counter(&mut self) -> &mut ValueCounter {
        &mut self.value_counter
    }

    fn message_context(&mut self) -> &mut MessageContext {
        &mut self.message_context
    }

    fn allocations_context(&mut self) -> &mut AllocationsContext {
        &mut self.allocations_context
    }

    fn host_fn_weights(&self) -> &HostFnWeights {
        &self.host_fn_weights
    }

    fn return_and_store_err<T>(&mut self, result: Result<T, ExtError>) -> Result<T, Error> {
        result.map_err(|err| {
            self.error_explanation = Some(err);
            Error::Core(err)
        })
    }
}
