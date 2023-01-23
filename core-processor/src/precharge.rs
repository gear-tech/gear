// Copyright (C)  2023 Gear Technologies Inc.
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
//

use crate::{
    common::{
        DispatchResult, ExecutableActorData, ExecutionErrorReason, JournalNote, PrechargedDispatch,
    },
    configs::{BlockConfig, PagesConfig},
    context::{ContextChargedForCodeLength, ContextChargedForMemory, ContextData},
    processing::{
        process_allowance_exceed, process_error, process_non_executable, process_success,
    },
    ContextChargedForCode, ContextChargedForInstrumentation,
};
use alloc::{collections::BTreeSet, vec::Vec};
use codec::{Decode, Encode};
use gear_backend_common::SystemReservationContext;
use gear_core::{
    gas::{ChargeResult, GasAllowanceCounter, GasCounter},
    ids::ProgramId,
    memory::{PageU32Size, WasmPage},
    message::{DispatchKind, IncomingDispatch, MessageWaitedType, StatusCode},
};
use scale_info::TypeInfo;

/// Operation related to gas charging.
#[derive(Encode, Decode, TypeInfo, Debug, PartialEq, Eq, PartialOrd, Ord, derive_more::Display)]
pub enum GasOperation {
    /// Load existing memory.
    #[display(fmt = "load memory")]
    LoadMemory,
    /// Grow memory size.
    #[display(fmt = "grow memory size")]
    GrowMemory,
    /// Handle initial memory.
    #[display(fmt = "handle initial memory")]
    InitialMemory,
    /// Handle program data.
    #[display(fmt = "handle program data")]
    ProgramData,
    /// Obtain code length.
    #[display(fmt = "obtain code length")]
    CodeLen,
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
    GasExceeded(GasOperation),
}

struct GasPrecharger<'a> {
    counter: &'a mut GasCounter,
    allowance_counter: &'a mut GasAllowanceCounter,
}

impl<'a> GasPrecharger<'a> {
    pub fn new(
        counter: &'a mut GasCounter,
        allowance_counter: &'a mut GasAllowanceCounter,
    ) -> Self {
        Self {
            counter,
            allowance_counter,
        }
    }

    fn charge_gas(&mut self, operation: GasOperation, amount: u64) -> Result<(), PrechargeError> {
        if self.allowance_counter.charge(amount) != ChargeResult::Enough {
            return Err(PrechargeError::BlockGasExceeded);
        }
        if self.counter.charge(amount) != ChargeResult::Enough {
            return Err(PrechargeError::GasExceeded(operation));
        }

        Ok(())
    }

    pub fn charge_gas_for_program_data(
        &mut self,
        read_cost: u64,
        per_byte_cost: u64,
    ) -> Result<(), PrechargeError> {
        self.charge_gas(
            GasOperation::ProgramData,
            calculate_gas_for_program(read_cost, per_byte_cost),
        )
    }

    pub fn charge_gas_for_program_code_len(
        &mut self,
        read_cost: u64,
    ) -> Result<(), PrechargeError> {
        self.charge_gas(GasOperation::CodeLen, read_cost)
    }

    pub fn charge_gas_for_program_code(
        &mut self,
        read_cost: u64,
        per_byte_cost: u64,
        code_len_bytes: u32,
    ) -> Result<(), PrechargeError> {
        self.charge_gas(
            GasOperation::ProgramCode,
            calculate_gas_for_code(read_cost, per_byte_cost, code_len_bytes.into()),
        )
    }

    pub fn charge_gas_for_instantiation(
        &mut self,
        gas_per_byte: u64,
        code_length: u32,
    ) -> Result<(), PrechargeError> {
        let amount = gas_per_byte * code_length as u64;
        self.charge_gas(GasOperation::ModuleInstantiation, amount)
    }

    pub fn charge_gas_for_instrumentation(
        &mut self,
        instrumentation_cost: u64,
        instrumentation_byte_cost: u64,
        original_code_len_bytes: u32,
    ) -> Result<(), PrechargeError> {
        let amount = instrumentation_cost.saturating_add(
            instrumentation_byte_cost.saturating_mul(original_code_len_bytes.into()),
        );
        self.charge_gas(GasOperation::ModuleInstrumentation, amount)
    }

    fn charge_gas_for_initial_memory(
        &mut self,
        settings: &PagesConfig,
        static_pages: WasmPage,
    ) -> Result<(), PrechargeError> {
        // TODO: check calculation is safe: #2007.
        let amount = settings.init_cost * static_pages.raw() as u64;
        self.charge_gas(GasOperation::InitialMemory, amount)
    }

    fn charge_gas_for_load_memory(
        &mut self,
        settings: &PagesConfig,
        allocations: &BTreeSet<WasmPage>,
        static_pages: WasmPage,
    ) -> Result<(), PrechargeError> {
        // TODO: check calculation is safe: #2007.
        let amount =
            settings.load_page_cost * (allocations.len() as u64 + static_pages.raw() as u64);
        self.charge_gas(GasOperation::LoadMemory, amount)
    }

    fn charge_gas_for_grow_memory(
        &mut self,
        settings: &PagesConfig,
        max_wasm_page: WasmPage,
        static_pages: WasmPage,
    ) -> Result<(), PrechargeError> {
        // TODO: make separate class for size in pages (here is static_pages): #2008.
        // TODO: check calculation is safe: #2007.
        let amount =
            settings.mem_grow_cost * (max_wasm_page.raw() as u64 + 1 - static_pages.raw() as u64);
        self.charge_gas(GasOperation::GrowMemory, amount)
    }

    /// Charge gas for pages init/load/grow and checks that there is enough gas for that.
    /// Returns size of wasm memory buffer which must be created in execution environment.
    // TODO: (issue #1894) remove charging for pages access/write/read/upload. But we should charge for
    // potential situation when gas limit/allowance exceeded during lazy-pages handling,
    // but we should continue execution until the end of block. During that execution
    // another signals can occur, which also take some time to process them.
    pub fn charge_gas_for_pages(
        &mut self,
        settings: &PagesConfig,
        allocations: &BTreeSet<WasmPage>,
        static_pages: WasmPage,
        initial_execution: bool,
        subsequent_execution: bool,
    ) -> Result<WasmPage, PrechargeError> {
        // Initial execution: just charge for static pages
        if initial_execution {
            // Charging gas for initial pages
            self.charge_gas_for_initial_memory(settings, static_pages)?;
            return Ok(static_pages);
        }

        let max_wasm_page = if let Some(page) = allocations.iter().next_back() {
            *page
        } else if let Ok(max_wasm_page) = static_pages.dec() {
            max_wasm_page
        } else {
            return Ok(WasmPage::zero());
        };

        if !subsequent_execution {
            // Charging gas for loaded pages
            self.charge_gas_for_load_memory(settings, allocations, static_pages)?;
        }

        // Charging gas for mem size
        self.charge_gas_for_grow_memory(settings, max_wasm_page, static_pages)?;

        // +1 because pages numeration begins from 0
        let wasm_mem_size = max_wasm_page
            .inc()
            // It means we somehow violated some constraints:
            // 1. one of allocated pages > MAX_WASM_PAGE_COUNT
            // 2. static pages > MAX_WASM_PAGE_COUNT
            .expect("WASM memory size is too big");

        Ok(wasm_mem_size)
    }
}

/// Calculates gas amount required to charge for program loading.
pub fn calculate_gas_for_program(read_cost: u64, _per_byte_cost: u64) -> u64 {
    read_cost
}

/// Calculates gas amount required to charge for code loading.
pub fn calculate_gas_for_code(read_cost: u64, per_byte_cost: u64, code_len_bytes: u64) -> u64 {
    read_cost.saturating_add(code_len_bytes.saturating_mul(per_byte_cost))
}

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
    let read_per_byte_cost = block_config.read_per_byte_cost;
    let read_cost = block_config.read_cost;

    let mut gas_counter = GasCounter::new(dispatch.gas_limit());
    let mut gas_allowance_counter = GasAllowanceCounter::new(gas_allowance);
    let mut charger = GasPrecharger::new(&mut gas_counter, &mut gas_allowance_counter);

    match charger.charge_gas_for_program_data(read_cost, read_per_byte_cost) {
        Ok(()) => Ok((dispatch, gas_counter, gas_allowance_counter).into()),
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
            Err(process_error(
                dispatch,
                destination_id,
                gas_burned,
                system_reservation_ctx,
                ExecutionErrorReason::GasExceeded(op),
                false,
            ))
        }
    }
}

fn check_is_executable(
    executable_data: Option<ExecutableActorData>,
    dispatch: &IncomingDispatch,
) -> Result<ExecutableActorData, StatusCode> {
    executable_data
        .map(|data| {
            if data.initialized & matches!(dispatch.kind(), DispatchKind::Init) {
                Err(crate::RE_INIT_STATUS_CODE)
            } else {
                Ok(data)
            }
        })
        .unwrap_or(Err(crate::UNAVAILABLE_DEST_STATUS_CODE))
}

/// Charge a message for fetching the actual length of the binary code
/// from a storage. The updated value of binary code length
/// should be kept in standalone storage. The caller has to call this
/// function to charge gas-counters accrodingly before fetching the value.
///
/// The function also performs several additional checks:
/// - if an actor is executable
/// - if a required dispatch method is exported.
pub fn precharge_for_code_length(
    block_config: &BlockConfig,
    dispatch: PrechargedDispatch,
    destination_id: ProgramId,
    executable_data: Option<ExecutableActorData>,
) -> PrechargeResult<ContextChargedForCodeLength> {
    let read_cost = block_config.read_cost;

    let (dispatch, mut gas_counter, mut gas_allowance_counter) = dispatch.into_parts();

    let actor_data = match check_is_executable(executable_data, &dispatch) {
        Err(status_code) => {
            let system_reservation_ctx = SystemReservationContext::from_dispatch(&dispatch);
            return Err(process_non_executable(
                dispatch,
                destination_id,
                status_code,
                system_reservation_ctx,
            ));
        }
        Ok(data) => data,
    };

    if !actor_data.code_exports.contains(&dispatch.kind()) {
        return Err(process_success(
            SuccessfulDispatchResultKind::Success,
            DispatchResult::success(dispatch, destination_id, gas_counter.into()),
        ));
    }

    let mut charger = GasPrecharger::new(&mut gas_counter, &mut gas_allowance_counter);
    match charger.charge_gas_for_program_code_len(read_cost) {
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
            Err(process_error(
                dispatch,
                destination_id,
                gas_counter.burned(),
                system_reservation_ctx,
                ExecutionErrorReason::GasExceeded(op),
                false,
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
    let read_per_byte_cost = block_config.read_per_byte_cost;
    let read_cost = block_config.read_cost;

    let mut charger = GasPrecharger::new(
        &mut context.data.gas_counter,
        &mut context.data.gas_allowance_counter,
    );

    match charger.charge_gas_for_program_code(read_cost, read_per_byte_cost, code_len_bytes) {
        Ok(()) => Ok((context, code_len_bytes).into()),
        Err(PrechargeError::BlockGasExceeded) => Err(process_allowance_exceed(
            context.data.dispatch,
            context.data.destination_id,
            context.data.gas_counter.burned(),
        )),
        Err(PrechargeError::GasExceeded(op)) => {
            let system_reservation_ctx =
                SystemReservationContext::from_dispatch(&context.data.dispatch);
            Err(process_error(
                context.data.dispatch,
                context.data.destination_id,
                context.data.gas_counter.burned(),
                system_reservation_ctx,
                ExecutionErrorReason::GasExceeded(op),
                false,
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
    let cost_base = block_config.code_instrumentation_cost;
    let cost_per_byte = block_config.code_instrumentation_byte_cost;

    let mut charger = GasPrecharger::new(
        &mut context.data.gas_counter,
        &mut context.data.gas_allowance_counter,
    );

    match charger.charge_gas_for_instrumentation(cost_base, cost_per_byte, original_code_len_bytes)
    {
        Ok(()) => Ok(context.into()),
        Err(PrechargeError::BlockGasExceeded) => Err(process_allowance_exceed(
            context.data.dispatch,
            context.data.destination_id,
            context.data.gas_counter.burned(),
        )),
        Err(PrechargeError::GasExceeded(op)) => {
            let system_reservation_ctx =
                SystemReservationContext::from_dispatch(&context.data.dispatch);
            Err(process_error(
                context.data.dispatch,
                context.data.destination_id,
                context.data.gas_counter.burned(),
                system_reservation_ctx,
                ExecutionErrorReason::GasExceeded(op),
                false,
            ))
        }
    }
}

/// Charge a message for program memory and module instantiation beforehand.
pub fn precharge_for_memory(
    block_config: &BlockConfig,
    mut context: ContextChargedForInstrumentation,
    subsequent_execution: bool,
) -> PrechargeResult<ContextChargedForMemory> {
    let ContextChargedForInstrumentation {
        data:
            ContextData {
                gas_counter,
                gas_allowance_counter,
                actor_data,
                dispatch,
                ..
            },
        code_len_bytes,
    } = &mut context;

    let mut f = || {
        let mut charger = GasPrecharger::new(gas_counter, gas_allowance_counter);

        let is_initial_execution =
            dispatch.context().is_none() && matches!(dispatch.kind(), DispatchKind::Init);
        let memory_size = charger.charge_gas_for_pages(
            &block_config.pages_config,
            &actor_data.allocations,
            actor_data.static_pages,
            is_initial_execution,
            subsequent_execution,
        )?;

        charger.charge_gas_for_instantiation(
            block_config.module_instantiation_byte_cost,
            *code_len_bytes,
        )?;

        Ok(memory_size)
    };

    let memory_size = match f() {
        Ok(size) => {
            log::debug!("Charged for module instantiation and memory pages. Size: {size:?}");
            size
        }
        Err(err) => {
            log::debug!("Failed to charge for module instantiation or memory pages: {err:?}");
            let reason = match err {
                PrechargeError::BlockGasExceeded => {
                    return Err(process_allowance_exceed(
                        context.data.dispatch,
                        context.data.destination_id,
                        context.data.gas_counter.burned(),
                    ));
                }
                PrechargeError::GasExceeded(op) => ExecutionErrorReason::GasExceeded(op),
            };

            let system_reservation_ctx =
                SystemReservationContext::from_dispatch(&context.data.dispatch);
            return Err(process_error(
                context.data.dispatch,
                context.data.destination_id,
                context.data.gas_counter.burned(),
                system_reservation_ctx,
                reason,
                false,
            ));
        }
    };

    Ok(ContextChargedForMemory {
        data: context.data,
        max_reservations: block_config.max_reservations,
        memory_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gear_backend_common::lazy_pages::LazyPagesWeights;
    use gear_core::memory::GearPage;

    fn prepare_allocs() -> BTreeSet<WasmPage> {
        let data = [0u16, 1, 2, 8, 18, 25, 27, 28, 93, 146, 240, 518];
        data.map(Into::into).map(|p: GearPage| p.to_page()).into()
    }

    fn prepare_alloc_config() -> PagesConfig {
        PagesConfig {
            max_pages: 32.into(),
            lazy_pages_weights: LazyPagesWeights {
                read: 100,
                write: 100,
                write_after_read: 100,
            },
            init_cost: 1000,
            alloc_cost: 2000,
            mem_grow_cost: 3000,
            load_page_cost: 4000,
        }
    }

    fn prepare_gas_counters() -> (GasCounter, GasAllowanceCounter) {
        (
            GasCounter::new(1_000_000),
            GasAllowanceCounter::new(4_000_000),
        )
    }

    #[test]
    fn gas_for_pages_initial() {
        let settings = prepare_alloc_config();
        let (mut gas_counter, mut gas_allowance_counter) = prepare_gas_counters();
        let mut charger = GasPrecharger::new(&mut gas_counter, &mut gas_allowance_counter);
        let static_pages = 4;
        let res = charger.charge_gas_for_pages(
            &settings,
            &Default::default(),
            static_pages.into(),
            true,
            false,
        );
        // Result is static pages count
        assert_eq!(res, Ok(static_pages.into()));
        // Charging for static pages initialization
        let charge = settings.init_cost * static_pages as u64;
        assert_eq!(charger.counter.left(), 1_000_000 - charge);
        assert_eq!(charger.allowance_counter.left(), 4_000_000 - charge);
    }

    #[test]
    fn gas_for_pages_static() {
        let settings = prepare_alloc_config();
        let (mut gas_counter, mut gas_allowance_counter) = prepare_gas_counters();
        let mut charger = GasPrecharger::new(&mut gas_counter, &mut gas_allowance_counter);
        let static_pages = 4;
        let res = charger.charge_gas_for_pages(
            &settings,
            &Default::default(),
            static_pages.into(),
            false,
            false,
        );
        // Result is static pages count
        assert_eq!(res, Ok(static_pages.into()));
        // Charge for the first load of static pages
        let charge = settings.load_page_cost * static_pages as u64;
        assert_eq!(charger.counter.left(), 1_000_000 - charge);
        assert_eq!(charger.allowance_counter.left(), 4_000_000 - charge);
    }

    #[test]
    fn gas_for_pages_alloc() {
        let settings = prepare_alloc_config();
        let (mut gas_counter, mut gas_allowance_counter) = prepare_gas_counters();
        let mut charger = GasPrecharger::new(&mut gas_counter, &mut gas_allowance_counter);
        let allocs = prepare_allocs();
        let static_pages = 4;
        let res =
            charger.charge_gas_for_pages(&settings, &allocs, static_pages.into(), false, false);
        // Result is the last page plus one
        let last = *allocs.iter().last().unwrap();
        assert_eq!(res, Ok(last.inc().unwrap()));
        // Charge for loading and mem grow
        let load_charge = settings.load_page_cost * (allocs.len() as u64 + static_pages as u64);
        let grow_charge = settings.mem_grow_cost * (last.raw() as u64 + 1 - static_pages as u64);
        assert_eq!(
            charger.counter.left(),
            1_000_000 - load_charge - grow_charge
        );
        assert_eq!(
            charger.allowance_counter.left(),
            4_000_000 - load_charge - grow_charge
        );

        // Use the second time (`subsequent` = `true`)
        let (mut gas_counter, mut gas_allowance_counter) = prepare_gas_counters();
        let mut charger = GasPrecharger::new(&mut gas_counter, &mut gas_allowance_counter);
        let res =
            charger.charge_gas_for_pages(&settings, &allocs, static_pages.into(), false, true);
        assert_eq!(res, Ok(last.inc().unwrap()));
        // Charge for mem grow only
        assert_eq!(charger.counter.left(), 1_000_000 - grow_charge);
        assert_eq!(charger.allowance_counter.left(), 4_000_000 - grow_charge);
    }
}
