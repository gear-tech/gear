// Copyright (C) 2023-2024 Gear Technologies Inc.
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

use crate::{
    common::{
        ActorExecutionErrorReplyReason, DispatchResult, ExecutableActorData, JournalNote,
        PrechargedDispatch,
    },
    configs::{BlockConfig, ProcessCosts},
    context::{
        ContextChargedForCodeLength, ContextChargedForMemory, ContextData, SystemReservationContext,
    },
    processing::{process_allowance_exceed, process_execution_error, process_success},
    ContextChargedForCode, ContextChargedForInstrumentation,
};
use alloc::vec::Vec;
use gear_core::{
    costs::BytesAmount,
    gas::{ChargeResult, GasAllowanceCounter, GasCounter},
    ids::ProgramId,
    message::{IncomingDispatch, MessageWaitedType},
    pages::{numerated::tree::IntervalsTree, WasmPage, WasmPagesAmount},
};

/// Operation related to gas charging.
#[derive(Debug, PartialEq, Eq, derive_more::Display)]
pub enum PreChargeGasOperation {
    /// Handle memory static pages.
    #[display(fmt = "handle memory static pages")]
    StaticPages,
    /// Handle program data.
    #[display(fmt = "handle program data")]
    ProgramData,
    /// Obtain code length.
    #[display(fmt = "obtain program code length")]
    ProgramCodeLen,
    /// Handle program code.
    #[display(fmt = "handle program code")]
    ProgramCode,
    /// Instantiate Wasm module.
    #[display(fmt = "instantiate Wasm module")]
    ModuleInstantiation,
    /// Instrument Wasm module.
    #[display(fmt = "instrument Wasm module")]
    ModuleInstrumentation,
}

#[derive(Debug, Eq, PartialEq)]
enum PrechargeError {
    BlockGasExceeded,
    GasExceeded(PreChargeGasOperation),
}

struct GasPrecharger<'a> {
    counter: &'a mut GasCounter,
    allowance_counter: &'a mut GasAllowanceCounter,
    costs: &'a ProcessCosts,
}

impl<'a> GasPrecharger<'a> {
    pub fn new(
        counter: &'a mut GasCounter,
        allowance_counter: &'a mut GasAllowanceCounter,
        costs: &'a ProcessCosts,
    ) -> Self {
        Self {
            counter,
            allowance_counter,
            costs,
        }
    }

    fn charge_gas(
        &mut self,
        operation: PreChargeGasOperation,
        amount: u64,
    ) -> Result<(), PrechargeError> {
        if self.allowance_counter.charge_if_enough(amount) != ChargeResult::Enough {
            return Err(PrechargeError::BlockGasExceeded);
        }
        if self.counter.charge_if_enough(amount) != ChargeResult::Enough {
            return Err(PrechargeError::GasExceeded(operation));
        }

        Ok(())
    }

    pub fn charge_gas_for_program_data(&mut self) -> Result<(), PrechargeError> {
        self.charge_gas(
            PreChargeGasOperation::ProgramData,
            self.costs.read.cost_for_one(),
        )
    }

    pub fn charge_gas_for_program_code_len(&mut self) -> Result<(), PrechargeError> {
        self.charge_gas(
            PreChargeGasOperation::ProgramCodeLen,
            self.costs.read.cost_for_one(),
        )
    }

    pub fn charge_gas_for_program_code(
        &mut self,
        code_len: BytesAmount,
    ) -> Result<(), PrechargeError> {
        self.charge_gas(
            PreChargeGasOperation::ProgramCode,
            self.costs
                .read
                .cost_for_with_bytes(self.costs.read_per_byte, code_len),
        )
    }

    pub fn charge_gas_for_instantiation(
        &mut self,
        code_len: BytesAmount,
    ) -> Result<(), PrechargeError> {
        self.charge_gas(
            PreChargeGasOperation::ModuleInstantiation,
            self.costs.module_instantiation_per_byte.cost_for(code_len),
        )
    }

    pub fn charge_gas_for_instrumentation(
        &mut self,
        original_code_len_bytes: BytesAmount,
    ) -> Result<(), PrechargeError> {
        self.charge_gas(
            PreChargeGasOperation::ModuleInstrumentation,
            self.costs
                .instrumentation
                .cost_for_with_bytes(self.costs.instrumentation_per_byte, original_code_len_bytes),
        )
    }

    /// Charge gas for pages and checks that there is enough gas for that.
    /// Returns size of wasm memory buffer which must be created in execution environment.
    pub fn charge_gas_for_pages(
        &mut self,
        allocations: &IntervalsTree<WasmPage>,
        static_pages: WasmPagesAmount,
    ) -> Result<WasmPagesAmount, PrechargeError> {
        // Charging gas for static pages.
        let amount = self.costs.static_page.cost_for(static_pages);
        self.charge_gas(PreChargeGasOperation::StaticPages, amount)?;

        if let Some(page) = allocations.end() {
            Ok(page.inc())
        } else {
            Ok(static_pages)
        }
    }
}

/// Possible variants of the `DispatchResult` if the latter contains value.
#[allow(missing_docs)]
#[derive(Debug)]
pub enum SuccessfulDispatchResultKind {
    Exit(ProgramId),
    Wait(Option<u32>, MessageWaitedType),
    Success,
}

/// Defines result variants of the precharge functions.
pub type PrechargeResult<T> = Result<T, Vec<JournalNote>>;

/// Charge a message for program data beforehand.
pub fn precharge_for_program(
    block_config: &BlockConfig,
    gas_allowance: u64,
    dispatch: IncomingDispatch,
    destination_id: ProgramId,
) -> PrechargeResult<PrechargedDispatch> {
    let mut gas_counter = GasCounter::new(dispatch.gas_limit());
    let mut gas_allowance_counter = GasAllowanceCounter::new(gas_allowance);
    let mut charger = GasPrecharger::new(
        &mut gas_counter,
        &mut gas_allowance_counter,
        &block_config.costs,
    );

    match charger.charge_gas_for_program_data() {
        Ok(()) => Ok(PrechargedDispatch::from_parts(
            dispatch,
            gas_counter,
            gas_allowance_counter,
        )),
        Err(PrechargeError::BlockGasExceeded) => {
            let gas_burned = gas_counter.burned();
            Err(process_allowance_exceed(
                dispatch,
                destination_id,
                gas_burned,
            ))
        }
        Err(PrechargeError::GasExceeded(op)) => {
            let gas_burned = gas_counter.burned();
            let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);
            Err(process_execution_error(
                dispatch,
                destination_id,
                gas_burned,
                system_reservation_ctx,
                ActorExecutionErrorReplyReason::PreChargeGasLimitExceeded(op),
            ))
        }
    }
}

/// Charge a message for fetching the actual length of the binary code
/// from a storage. The updated value of binary code length
/// should be kept in standalone storage. The caller has to call this
/// function to charge gas-counters accordingly before fetching the value.
///
/// The function also performs several additional checks:
/// - if an actor is executable
/// - if a required dispatch method is exported.
pub fn precharge_for_code_length(
    block_config: &BlockConfig,
    dispatch: PrechargedDispatch,
    destination_id: ProgramId,
    actor_data: ExecutableActorData,
) -> PrechargeResult<ContextChargedForCodeLength> {
    let (dispatch, mut gas_counter, mut gas_allowance_counter) = dispatch.into_parts();

    if !actor_data.code_exports.contains(&dispatch.kind()) {
        return Err(process_success(
            SuccessfulDispatchResultKind::Success,
            DispatchResult::success(dispatch, destination_id, gas_counter.to_amount()),
        ));
    }

    let mut charger = GasPrecharger::new(
        &mut gas_counter,
        &mut gas_allowance_counter,
        &block_config.costs,
    );
    match charger.charge_gas_for_program_code_len() {
        Ok(()) => Ok(ContextChargedForCodeLength {
            data: ContextData {
                gas_counter,
                gas_allowance_counter,
                dispatch,
                destination_id,
                actor_data,
            },
        }),
        Err(PrechargeError::BlockGasExceeded) => Err(process_allowance_exceed(
            dispatch,
            destination_id,
            gas_counter.burned(),
        )),
        Err(PrechargeError::GasExceeded(op)) => {
            let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);
            Err(process_execution_error(
                dispatch,
                destination_id,
                gas_counter.burned(),
                system_reservation_ctx,
                ActorExecutionErrorReplyReason::PreChargeGasLimitExceeded(op),
            ))
        }
    }
}

/// Charge a message for the program binary code beforehand.
pub fn precharge_for_code(
    block_config: &BlockConfig,
    mut context: ContextChargedForCodeLength,
    code_len_bytes: u32,
) -> PrechargeResult<ContextChargedForCode> {
    let mut charger = GasPrecharger::new(
        &mut context.data.gas_counter,
        &mut context.data.gas_allowance_counter,
        &block_config.costs,
    );

    match charger.charge_gas_for_program_code(code_len_bytes.into()) {
        Ok(()) => Ok((context, code_len_bytes).into()),
        Err(PrechargeError::BlockGasExceeded) => Err(process_allowance_exceed(
            context.data.dispatch,
            context.data.destination_id,
            context.data.gas_counter.burned(),
        )),
        Err(PrechargeError::GasExceeded(op)) => {
            let system_reservation_ctx =
                SystemReservationContext::from_dispatch(&context.data.dispatch);
            Err(process_execution_error(
                context.data.dispatch,
                context.data.destination_id,
                context.data.gas_counter.burned(),
                system_reservation_ctx,
                ActorExecutionErrorReplyReason::PreChargeGasLimitExceeded(op),
            ))
        }
    }
}

/// Charge a message for instrumentation of the binary code beforehand.
pub fn precharge_for_instrumentation(
    block_config: &BlockConfig,
    mut context: ContextChargedForCode,
    original_code_len_bytes: u32,
) -> PrechargeResult<ContextChargedForInstrumentation> {
    let mut charger = GasPrecharger::new(
        &mut context.data.gas_counter,
        &mut context.data.gas_allowance_counter,
        &block_config.costs,
    );

    match charger.charge_gas_for_instrumentation(original_code_len_bytes.into()) {
        Ok(()) => Ok(context.into()),
        Err(PrechargeError::BlockGasExceeded) => Err(process_allowance_exceed(
            context.data.dispatch,
            context.data.destination_id,
            context.data.gas_counter.burned(),
        )),
        Err(PrechargeError::GasExceeded(op)) => {
            let system_reservation_ctx =
                SystemReservationContext::from_dispatch(&context.data.dispatch);
            Err(process_execution_error(
                context.data.dispatch,
                context.data.destination_id,
                context.data.gas_counter.burned(),
                system_reservation_ctx,
                ActorExecutionErrorReplyReason::PreChargeGasLimitExceeded(op),
            ))
        }
    }
}

/// Charge a message for program memory and module instantiation beforehand.
pub fn precharge_for_memory(
    block_config: &BlockConfig,
    mut context: ContextChargedForInstrumentation,
) -> PrechargeResult<ContextChargedForMemory> {
    let ContextChargedForInstrumentation {
        data:
            ContextData {
                gas_counter,
                gas_allowance_counter,
                actor_data,
                ..
            },
        code_len_bytes,
    } = &mut context;

    let mut f = || {
        let mut charger =
            GasPrecharger::new(gas_counter, gas_allowance_counter, &block_config.costs);

        let memory_size =
            charger.charge_gas_for_pages(&actor_data.allocations, actor_data.static_pages)?;

        charger.charge_gas_for_instantiation((*code_len_bytes).into())?;

        Ok(memory_size)
    };

    match f() {
        Ok(memory_size) => {
            log::trace!("Charged for module instantiation and memory pages. Size: {memory_size:?}");
            Ok(ContextChargedForMemory {
                data: context.data,
                max_reservations: block_config.max_reservations,
                memory_size,
            })
        }
        Err(err) => {
            log::trace!("Failed to charge for module instantiation or memory pages: {err:?}");
            match err {
                PrechargeError::BlockGasExceeded => Err(process_allowance_exceed(
                    context.data.dispatch,
                    context.data.destination_id,
                    context.data.gas_counter.burned(),
                )),
                PrechargeError::GasExceeded(op) => {
                    let system_reservation_ctx =
                        SystemReservationContext::from_dispatch(&context.data.dispatch);
                    Err(process_execution_error(
                        context.data.dispatch,
                        context.data.destination_id,
                        context.data.gas_counter.burned(),
                        system_reservation_ctx,
                        ActorExecutionErrorReplyReason::PreChargeGasLimitExceeded(op),
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepare_gas_counters() -> (GasCounter, GasAllowanceCounter) {
        (
            GasCounter::new(1_000_000),
            GasAllowanceCounter::new(4_000_000),
        )
    }

    #[test]
    fn gas_for_static_pages() {
        let (mut gas_counter, mut gas_allowance_counter) = prepare_gas_counters();
        let costs = ProcessCosts {
            static_page: 1.into(),
            ..Default::default()
        };
        let mut charger = GasPrecharger::new(&mut gas_counter, &mut gas_allowance_counter, &costs);
        let static_pages = 4.into();
        let allocations = Default::default();

        let res = charger.charge_gas_for_pages(&allocations, static_pages);

        // Result is static pages count
        assert_eq!(res, Ok(static_pages));

        // Charging for static pages initialization
        let charge = costs.static_page.cost_for(static_pages);
        assert_eq!(charger.counter.left(), 1_000_000 - charge);
        assert_eq!(charger.allowance_counter.left(), 4_000_000 - charge);
    }
}
