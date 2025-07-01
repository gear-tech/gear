// Copyright (C) 2023-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Precharge module.

use crate::{
    common::{
        ActorExecutionErrorReplyReason, ExecutableActorData, JournalNote, ReservationsAndMemorySize,
    },
    configs::BlockConfig,
    context::SystemReservationContext,
    processing::{process_allowance_exceed, process_execution_error},
};
use alloc::vec::Vec;
use core::marker::PhantomData;
use gear_core::{
    code::{CodeMetadata, InstantiatedSectionSizes, SectionName},
    costs::{BytesAmount, ProcessCosts},
    gas::{ChargeResult, GasAllowanceCounter, GasCounter},
    message::IncomingDispatch,
    primitives::ActorId,
};

/// Operation related to gas charging.
#[derive(Debug, PartialEq, Eq, derive_more::Display)]
pub enum PreChargeGasOperation {
    /// Handle memory static pages.
    #[display("handle memory static pages")]
    StaticPages,
    /// Handle program data.
    #[display("handle program data")]
    ProgramData,
    /// Obtain code metadata.
    #[display("obtain code metadata")]
    CodeMetadata,
    /// Obtain original code
    #[display("obtain original code")]
    OriginalCode,
    /// Obtain instrumented code
    #[display("obtain instrumented code")]
    InstrumentedCode,
    /// Instantiate the type section of the Wasm module.
    #[display("instantiate {_0} of Wasm module")]
    ModuleInstantiation(SectionName),
    /// Instrument Wasm module.
    #[display("instrument Wasm module")]
    ModuleInstrumentation,
    /// Obtain program allocations.
    #[display("obtain program allocations")]
    Allocations,
}

/// Defines result variants of the precharge functions.
pub type PrechargeResult<T> = Result<T, Vec<JournalNote>>;

/// ZST for the context that charged nothing.
pub struct ForNothing;

/// ZST for the context that charged for program data.
pub struct ForProgram;

/// ZST for the context that charged for code metadata.
pub struct ForCodeMetadata;

/// ZST for the context that charged for original code.
pub struct ForOriginalCode;

/// ZST for the context that charged for instrumented code.
pub struct ForInstrumentedCode;

/// ZST for the context that charged for allocations.
pub struct ForAllocations;

/// ZST for the context that charged for module instantiation.
pub struct ForModuleInstantiation;

/// Context charged gas for the program execution.
pub struct ContextCharged<For = ForNothing> {
    destination_id: ActorId,
    dispatch: IncomingDispatch,
    gas_counter: GasCounter,
    gas_allowance_counter: GasAllowanceCounter,
    actor_data: Option<ExecutableActorData>,
    reservations_and_memory_size: Option<ReservationsAndMemorySize>,

    _phantom: PhantomData<For>,
}

impl ContextCharged {
    /// Creates a new empty instance of the context charged for the program execution.
    pub fn new(
        destination_id: ActorId,
        dispatch: IncomingDispatch,
        gas_allowance: u64,
    ) -> ContextCharged<ForNothing> {
        let gas_counter = GasCounter::new(dispatch.gas_limit());
        let gas_allowance_counter = GasAllowanceCounter::new(gas_allowance);

        Self {
            destination_id,
            dispatch,
            gas_counter,
            gas_allowance_counter,
            actor_data: None,
            reservations_and_memory_size: None,
            _phantom: PhantomData,
        }
    }
}

impl<T> ContextCharged<T> {
    /// Splits the context into parts.
    pub fn into_parts(self) -> (ActorId, IncomingDispatch, GasCounter, GasAllowanceCounter) {
        (
            self.destination_id,
            self.dispatch,
            self.gas_counter,
            self.gas_allowance_counter,
        )
    }

    /// Gas already burned
    pub fn gas_burned(&self) -> u64 {
        self.gas_counter.burned()
    }

    /// Gas left
    pub fn gas_left(&self) -> u64 {
        self.gas_counter.left()
    }

    /// Charges gas for the operation.
    fn charge_gas<For>(
        mut self,
        operation: PreChargeGasOperation,
        amount: u64,
    ) -> PrechargeResult<ContextCharged<For>> {
        if self.gas_allowance_counter.charge_if_enough(amount) != ChargeResult::Enough {
            let gas_burned = self.gas_counter.burned();

            return Err(process_allowance_exceed(
                self.dispatch,
                self.destination_id,
                gas_burned,
            ));
        }

        if self.gas_counter.charge_if_enough(amount) != ChargeResult::Enough {
            let gas_burned = self.gas_counter.burned();
            let system_reservation_ctx = SystemReservationContext::from_dispatch(&self.dispatch);

            return Err(process_execution_error(
                self.dispatch,
                self.destination_id,
                gas_burned,
                system_reservation_ctx,
                ActorExecutionErrorReplyReason::PreChargeGasLimitExceeded(operation),
            ));
        }

        Ok(ContextCharged {
            destination_id: self.destination_id,
            dispatch: self.dispatch,
            gas_counter: self.gas_counter,
            gas_allowance_counter: self.gas_allowance_counter,
            actor_data: self.actor_data,
            reservations_and_memory_size: self.reservations_and_memory_size,
            _phantom: PhantomData,
        })
    }
}

impl ContextCharged<ForNothing> {
    /// Charges gas for getting the program data.
    pub fn charge_for_program(
        self,
        block_config: &BlockConfig,
    ) -> PrechargeResult<ContextCharged<ForProgram>> {
        self.charge_gas(
            PreChargeGasOperation::ProgramData,
            block_config.costs.read.cost_for_one(),
        )
    }
}

impl ContextCharged<ForProgram> {
    /// Charges gas for getting the code metadata.
    pub fn charge_for_code_metadata(
        self,
        block_config: &BlockConfig,
    ) -> PrechargeResult<ContextCharged<ForCodeMetadata>> {
        self.charge_gas(
            PreChargeGasOperation::CodeMetadata,
            block_config.costs.read.cost_for_one(),
        )
    }
}

impl ContextCharged<ForCodeMetadata> {
    /// Charges gas for getting the original code.
    pub fn charge_for_original_code(
        self,
        block_config: &BlockConfig,
        code_len_bytes: u32,
    ) -> PrechargeResult<ContextCharged<ForOriginalCode>> {
        self.charge_gas(
            PreChargeGasOperation::OriginalCode,
            block_config
                .costs
                .read
                .cost_for_with_bytes(block_config.costs.read_per_byte, code_len_bytes.into()),
        )
    }

    /// Charges gas for getting the instrumented code.
    pub fn charge_for_instrumented_code(
        self,
        block_config: &BlockConfig,
        code_len_bytes: u32,
    ) -> PrechargeResult<ContextCharged<ForInstrumentedCode>> {
        self.charge_gas(
            PreChargeGasOperation::InstrumentedCode,
            block_config
                .costs
                .read
                .cost_for_with_bytes(block_config.costs.read_per_byte, code_len_bytes.into()),
        )
    }
}

impl ContextCharged<ForOriginalCode> {
    /// Charges gas for code instrumentation.
    pub fn charge_for_instrumentation(
        self,
        block_config: &BlockConfig,
        original_code_len_bytes: u32,
    ) -> PrechargeResult<ContextCharged<ForInstrumentedCode>> {
        self.charge_gas(
            PreChargeGasOperation::ModuleInstrumentation,
            block_config.costs.instrumentation.cost_for_with_bytes(
                block_config.costs.instrumentation_per_byte,
                original_code_len_bytes.into(),
            ),
        )
    }
}

impl ContextCharged<ForInstrumentedCode> {
    /// Charges gas for allocations.
    pub fn charge_for_allocations(
        self,
        block_config: &BlockConfig,
        allocations_tree_len: u32,
    ) -> PrechargeResult<ContextCharged<ForAllocations>> {
        if allocations_tree_len == 0 {
            return Ok(ContextCharged {
                destination_id: self.destination_id,
                dispatch: self.dispatch,
                gas_counter: self.gas_counter,
                gas_allowance_counter: self.gas_allowance_counter,
                actor_data: self.actor_data,
                reservations_and_memory_size: self.reservations_and_memory_size,
                _phantom: PhantomData,
            });
        }

        let amount = block_config
            .costs
            .load_allocations_per_interval
            .cost_for(allocations_tree_len)
            .saturating_add(block_config.costs.read.cost_for_one());

        self.charge_gas(PreChargeGasOperation::Allocations, amount)
    }
}

impl ContextCharged<ForAllocations> {
    /// Charges gas for module instantiation.
    pub fn charge_for_module_instantiation(
        mut self,
        block_config: &BlockConfig,
        actor_data: ExecutableActorData,
        section_sizes: &InstantiatedSectionSizes,
        code_metadata: &CodeMetadata,
    ) -> PrechargeResult<ContextCharged<ForModuleInstantiation>> {
        // Calculates size of wasm memory buffer which must be created in execution environment
        let memory_size = if let Some(page) = actor_data.allocations.end() {
            page.inc()
        } else {
            code_metadata.static_pages()
        };

        let reservations_and_memory_size = ReservationsAndMemorySize {
            max_reservations: block_config.max_reservations,
            memory_size,
        };

        self.actor_data = Some(actor_data);
        self.reservations_and_memory_size = Some(reservations_and_memory_size);

        self = self.charge_gas_for_section_instantiation(
            &block_config.costs,
            SectionName::Function,
            section_sizes.code_section().into(),
        )?;

        self = self.charge_gas_for_section_instantiation(
            &block_config.costs,
            SectionName::Data,
            section_sizes.data_section().into(),
        )?;

        self = self.charge_gas_for_section_instantiation(
            &block_config.costs,
            SectionName::Global,
            section_sizes.global_section().into(),
        )?;

        self = self.charge_gas_for_section_instantiation(
            &block_config.costs,
            SectionName::Table,
            section_sizes.table_section().into(),
        )?;

        self = self.charge_gas_for_section_instantiation(
            &block_config.costs,
            SectionName::Element,
            section_sizes.element_section().into(),
        )?;

        self = self.charge_gas_for_section_instantiation(
            &block_config.costs,
            SectionName::Type,
            section_sizes.type_section().into(),
        )?;

        Ok(ContextCharged {
            destination_id: self.destination_id,
            dispatch: self.dispatch,
            gas_counter: self.gas_counter,
            gas_allowance_counter: self.gas_allowance_counter,
            actor_data: self.actor_data,
            reservations_and_memory_size: self.reservations_and_memory_size,
            _phantom: PhantomData,
        })
    }

    /// Helper function to charge gas for section instantiation.
    fn charge_gas_for_section_instantiation(
        self,
        costs: &ProcessCosts,
        section_name: SectionName,
        section_len: BytesAmount,
    ) -> PrechargeResult<ContextCharged<ForAllocations>> {
        let instantiation_costs = &costs.instantiation_costs;

        let cost_per_byte = match section_name {
            SectionName::Function => &instantiation_costs.code_section_per_byte,
            SectionName::Data => &instantiation_costs.data_section_per_byte,
            SectionName::Global => &instantiation_costs.global_section_per_byte,
            SectionName::Table => &instantiation_costs.table_section_per_byte,
            SectionName::Element => &instantiation_costs.element_section_per_byte,
            SectionName::Type => &instantiation_costs.type_section_per_byte,
            _ => {
                unimplemented!("Wrong {section_name:?} for section instantiation")
            }
        };

        self.charge_gas(
            PreChargeGasOperation::ModuleInstantiation(section_name),
            cost_per_byte.cost_for(section_len),
        )
    }
}

impl ContextCharged<ForModuleInstantiation> {
    /// Converts the context into the final parts.
    pub fn into_final_parts(
        self,
    ) -> (
        ActorId,
        IncomingDispatch,
        GasCounter,
        GasAllowanceCounter,
        ExecutableActorData,
        ReservationsAndMemorySize,
    ) {
        (
            self.destination_id,
            self.dispatch,
            self.gas_counter,
            self.gas_allowance_counter,
            self.actor_data.unwrap(),
            self.reservations_and_memory_size.unwrap(),
        )
    }
}
