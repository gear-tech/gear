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
        ActorExecutionErrorReplyReason, DispatchResult, JournalNote, LazyStorageAccess, Program,
    },
    configs::{BlockConfig, ProcessCosts},
    context::SystemReservationContext,
    processing::{
        process_allowance_exceed, process_error, process_execution_error, process_success,
        ProcessErrorCase,
    },
    ProcessExecutionContext,
};
use alloc::vec::Vec;
use gear_core::{
    costs::BytesAmount,
    gas::{ChargeResult, GasAllowanceCounter, GasCounter},
    ids::ProgramId,
    message::{DispatchKind, IncomingDispatch, MessageWaitedType},
    pages::{numerated::tree::IntervalsTree, WasmPage, WasmPagesAmount},
    program::ProgramState,
    reservation::GasReserver,
};

/// +_+_+
pub fn precharge(
    storage: &impl LazyStorageAccess,
    block_config: &BlockConfig,
    gas_allowance: u64,
    dispatch: IncomingDispatch,
    program_id: ProgramId,
    balance: u128,
) -> Result<ProcessExecutionContext, Vec<JournalNote>> {
    let mut gas_counter = GasCounter::new(dispatch.gas_limit());
    let mut gas_allowance_counter = GasAllowanceCounter::new(gas_allowance);
    let mut charger = GasPrecharger::new(
        &mut gas_counter,
        &mut gas_allowance_counter,
        &block_config.costs,
    );

    let process_pre_charge_error =
        |err: PrechargeError, gas_counter: GasCounter, dispatch: IncomingDispatch| match err {
            PrechargeError::BlockGasExceeded => {
                process_allowance_exceed(dispatch, program_id, gas_counter.burned())
            }
            PrechargeError::GasExceeded(op) => {
                let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);
                process_execution_error(
                    dispatch,
                    program_id,
                    gas_counter.burned(),
                    system_reservation_ctx,
                    ActorExecutionErrorReplyReason::PreChargeGasLimitExceeded(op),
                )
            }
        };

    let process_error =
        |err_case: ProcessErrorCase, gas_counter: GasCounter, dispatch: IncomingDispatch| {
            let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);
            process_error(
                dispatch,
                program_id,
                gas_counter.burned(),
                system_reservation_ctx,
                err_case,
            )
        };

    if let Err(err) = charger.charge_gas_for_program_data() {
        return Err(process_pre_charge_error(err, gas_counter, dispatch));
    }

    let program = match storage.program_info(program_id) {
        Some(program) => program,
        None => {
            return Err(process_error(
                ProcessErrorCase::NonExecutable,
                gas_counter,
                dispatch,
            ))
        }
    };

    if program.state == ProgramState::Initialized && dispatch.kind() == DispatchKind::Init {
        unreachable!("+_+_+");
    }

    // If the destination program is uninitialized, then we allow
    // to process message, if it's a reply or init message.
    // Otherwise, we return error reply.
    if matches!(program.state, ProgramState::Uninitialized { message_id }
            if message_id != dispatch.message().id() && dispatch.kind() != DispatchKind::Reply)
    {
        if dispatch.kind() == DispatchKind::Init {
            unreachable!("+_+_+");
        }

        return Err(process_error(
            ProcessErrorCase::NonExecutable,
            gas_counter,
            dispatch,
        ));
    }

    if !program.code_exports.contains(&dispatch.kind()) {
        return Err(process_success(
            SuccessfulDispatchResultKind::Success,
            DispatchResult::success(dispatch, program_id, gas_counter.to_amount()),
        ));
    }

    if let Err(err) = charger.charge_gas_for_program_code_len() {
        return Err(process_pre_charge_error(err, gas_counter, dispatch));
    }

    let code_len_bytes = storage
        .code_len(program.code_id)
        .unwrap_or_else(|| unreachable!("+_+_+"));

    if let Err(err) = charger.charge_gas_for_program_code(code_len_bytes.into()) {
        return Err(process_pre_charge_error(err, gas_counter, dispatch));
    }

    let mut code = storage
        .code(program.code_id)
        .unwrap_or_else(|| unreachable!("+_+_+"));

    // Reinstrument the code if necessary.
    if storage.need_reinstrumentation(&code) {
        if let Err(err) = charger.charge_gas_for_instrumentation(code.original_code_len().into()) {
            return Err(process_pre_charge_error(err, gas_counter, dispatch));
        }

        match storage.reinstrument_code(program.code_id) {
            Ok(new_code) => code = new_code,
            Err(_) => {
                return Err(process_error(
                    ProcessErrorCase::ReinstrumentationFailed,
                    gas_counter,
                    dispatch,
                ))
            }
        }
    };

    let memory_size = match charger.charge_gas_for_pages(&program.allocations, code.static_pages())
    {
        Ok(memory_size) => memory_size,
        Err(err) => return Err(process_pre_charge_error(err, gas_counter, dispatch)),
    };

    if let Err(err) = charger.charge_gas_for_instantiation((code.code().len() as u32).into()) {
        return Err(process_pre_charge_error(err, gas_counter, dispatch));
    }

    let gas_reserver = GasReserver::new(
        &dispatch,
        program.gas_reservation_map,
        block_config.max_reservations,
    );

    Ok(ProcessExecutionContext {
        gas_counter,
        gas_allowance_counter,
        dispatch,
        gas_reserver,
        balance,
        program: Program {
            id: program_id,
            memory_infix: program.memory_infix,
            code,
            allocations: program.allocations,
        },
        memory_size,
    })
}

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

// +_+_+ move to common
/// Possible variants of the `DispatchResult` if the latter contains value.
#[allow(missing_docs)]
#[derive(Debug)]
pub enum SuccessfulDispatchResultKind {
    Exit(ProgramId),
    Wait(Option<u32>, MessageWaitedType),
    Success,
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
