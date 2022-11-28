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

use crate::{
    common::{
        DispatchResult, DispatchResultKind, ExecutionError, ExecutionErrorReason, GasOperation,
        WasmExecutionContext,
    },
    configs::{AllocationsConfig, BlockInfo, ExecutionSettings},
    ext::{ProcessorContext, ProcessorExt},
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec::Vec,
};
use gear_backend_common::{
    BackendReport, Environment, GetGasAmount, IntoExtInfo, TerminationReason,
};
use gear_core::{
    code::{Code, CodeAndId, InstrumentedCodeAndId},
    env::Ext as EnvExt,
    gas::{ChargeResult, GasAllowanceCounter, GasCounter, ValueCounter},
    ids::ProgramId,
    memory::{AllocationsContext, Memory, PageBuf, PageNumber, WasmPageNumber},
    message::{ContextSettings, DispatchKind, IncomingDispatch, IncomingMessage, MessageContext},
    program::Program,
    reservation::GasReserver,
};

pub(crate) enum ChargeForBytesResult {
    Ok,
    BlockGasExceeded,
    GasExceeded,
}

/// Calculates gas amount required to charge for program loading.
pub fn calculate_gas_for_program(read_cost: u64, _per_byte_cost: u64) -> u64 {
    read_cost
}

/// Calculates gas amount required to charge for code loading.
pub fn calculate_gas_for_code(read_cost: u64, per_byte_cost: u64, code_len_bytes: u64) -> u64 {
    read_cost.saturating_add(code_len_bytes.saturating_mul(per_byte_cost))
}

fn charge_gas(
    operation: GasOperation,
    amount: u64,
    gas_counter: &mut GasCounter,
    gas_allowance_counter: &mut GasAllowanceCounter,
) -> Result<(), ExecutionErrorReason> {
    log::trace!("Charge {} of gas to {}", amount, operation);
    if gas_allowance_counter.charge(amount) != ChargeResult::Enough {
        return Err(ExecutionErrorReason::BlockGasExceeded(operation));
    }
    if gas_counter.charge(amount) != ChargeResult::Enough {
        return Err(ExecutionErrorReason::GasExceeded(operation));
    }

    Ok(())
}

fn charge_gas_for_bytes(
    amount: u64,
    gas_counter: &mut GasCounter,
    gas_allowance_counter: &mut GasAllowanceCounter,
) -> ChargeForBytesResult {
    if gas_allowance_counter.charge(amount) != ChargeResult::Enough {
        return ChargeForBytesResult::BlockGasExceeded;
    }
    if gas_counter.charge(amount) != ChargeResult::Enough {
        return ChargeForBytesResult::GasExceeded;
    }

    ChargeForBytesResult::Ok
}

pub(crate) fn charge_gas_for_program(
    read_cost: u64,
    per_byte_cost: u64,
    gas_counter: &mut GasCounter,
    gas_allowance_counter: &mut GasAllowanceCounter,
) -> ChargeForBytesResult {
    charge_gas_for_bytes(
        calculate_gas_for_program(read_cost, per_byte_cost),
        gas_counter,
        gas_allowance_counter,
    )
}

pub(crate) fn charge_gas_for_code(
    read_cost: u64,
    per_byte_cost: u64,
    code_len_bytes: u32,
    gas_counter: &mut GasCounter,
    gas_allowance_counter: &mut GasAllowanceCounter,
) -> ChargeForBytesResult {
    charge_gas_for_bytes(
        calculate_gas_for_code(read_cost, per_byte_cost, code_len_bytes.into()),
        gas_counter,
        gas_allowance_counter,
    )
}

/// Make checks that everything with memory goes well.
fn check_memory<'a>(
    allocations: &BTreeSet<WasmPageNumber>,
    pages_with_data: impl Iterator<Item = &'a PageNumber>,
    static_pages: WasmPageNumber,
    memory_size: WasmPageNumber,
) -> Result<(), ExecutionErrorReason> {
    if memory_size < static_pages {
        log::error!(
            "Mem size less then static pages num: mem_size = {:?}, static_pages = {:?}",
            memory_size,
            static_pages
        );
        return Err(ExecutionErrorReason::InsufficientMemorySize);
    }

    // Checks that all pages with data are in allocations set.
    for page in pages_with_data {
        let wasm_page = page.to_wasm_page();
        if wasm_page >= static_pages && !allocations.contains(&wasm_page) {
            return Err(ExecutionErrorReason::PageIsNotAllocated(*page));
        }
    }

    Ok(())
}

pub(crate) fn charge_gas_for_instantiation(
    gas_per_byte: u64,
    code_length: u32,
    gas_counter: &mut GasCounter,
    gas_allowance_counter: &mut GasAllowanceCounter,
) -> Result<(), ExecutionErrorReason> {
    let amount = gas_per_byte * code_length as u64;
    charge_gas(
        GasOperation::ModuleInstantiation,
        amount,
        gas_counter,
        gas_allowance_counter,
    )
}

/// Charge gas for pages init/load/grow and checks that there is enough gas for that.
/// Returns size of wasm memory buffer which must be created in execution environment.
pub(crate) fn charge_gas_for_pages(
    settings: &AllocationsConfig,
    gas_counter: &mut GasCounter,
    gas_allowance_counter: &mut GasAllowanceCounter,
    allocations: &BTreeSet<WasmPageNumber>,
    static_pages: WasmPageNumber,
    initial_execution: bool,
    subsequent_execution: bool,
) -> Result<WasmPageNumber, ExecutionErrorReason> {
    // Initial execution: just charge for static pages
    if initial_execution {
        // Charging gas for initial pages
        let amount = settings.init_cost * static_pages.0 as u64;
        charge_gas(
            GasOperation::InitialMemory,
            amount,
            gas_counter,
            gas_allowance_counter,
        )?;

        return Ok(static_pages);
    }

    let max_wasm_page = if let Some(page) = allocations.iter().next_back() {
        *page
    } else if static_pages != WasmPageNumber(0) {
        static_pages - 1.into()
    } else {
        return Ok(0.into());
    };

    if !subsequent_execution {
        // Charging gas for loaded pages
        let amount = settings.load_page_cost * (allocations.len() as u64 + static_pages.0 as u64);
        charge_gas(
            GasOperation::LoadMemory,
            amount,
            gas_counter,
            gas_allowance_counter,
        )?;
    }

    // Charging gas for mem size
    let amount = settings.mem_grow_cost * (max_wasm_page.0 as u64 + 1 - static_pages.0 as u64);
    charge_gas(
        GasOperation::GrowMemory,
        amount,
        gas_counter,
        gas_allowance_counter,
    )?;

    // +1 because pages numeration begins from 0
    Ok(max_wasm_page + 1.into())
}

/// Writes initial pages data to memory and prepare memory for execution.
fn prepare_memory<A: ProcessorExt, M: Memory>(
    program_id: ProgramId,
    pages_data: &mut BTreeMap<PageNumber, PageBuf>,
    static_pages: WasmPageNumber,
    stack_end: Option<WasmPageNumber>,
    mem: &mut M,
) -> Result<(), ExecutionErrorReason> {
    if let Some(stack_end) = stack_end {
        if stack_end > static_pages {
            return Err(ExecutionErrorReason::StackEndPageBiggerWasmMemSize(
                stack_end,
                static_pages,
            ));
        }
    }

    // Set initial data for pages
    for (page, data) in pages_data.iter_mut() {
        mem.write(page.offset(), data.as_slice())
            .map_err(|err| ExecutionErrorReason::InitialDataWriteFailed(*page, err))?;
    }

    if A::LAZY_PAGES_ENABLED {
        if !pages_data.is_empty() {
            return Err(ExecutionErrorReason::InitialPagesContainsDataInLazyPagesMode);
        }
        A::lazy_pages_init_for_program(mem, program_id, stack_end);
    } else {
        // If we executes without lazy pages, then we have to save all initial data for static pages,
        // in order to be able to identify pages, which has been changed during execution.
        // Skip stack page if they are specified.
        let begin = stack_end.unwrap_or(WasmPageNumber(0));

        if pages_data.keys().any(|&p| p < begin.to_gear_page()) {
            return Err(ExecutionErrorReason::StackPagesHaveInitialData);
        }

        for page in (begin.0..static_pages.0)
            .map(WasmPageNumber)
            .flat_map(|p| p.to_gear_pages_iter())
        {
            if pages_data.contains_key(&page) {
                // This page already has initial data
                continue;
            }
            let mut data = PageBuf::new_zeroed();
            mem.read(page.offset(), data.as_mut_slice())
                .map_err(|err| ExecutionErrorReason::InitialMemoryReadFailed(page, err))?;
            pages_data.insert(page, data);
        }
    }
    Ok(())
}

/// Returns pages and their new data, which must be updated or uploaded to storage.
fn get_pages_to_be_updated<A: ProcessorExt>(
    old_pages_data: BTreeMap<PageNumber, PageBuf>,
    new_pages_data: BTreeMap<PageNumber, PageBuf>,
    static_pages: WasmPageNumber,
) -> BTreeMap<PageNumber, PageBuf> {
    if A::LAZY_PAGES_ENABLED {
        // In lazy pages mode we update some page data in storage,
        // when it has been write accessed, so no need to compare old and new page data.
        new_pages_data.keys().for_each(|page| {
            log::trace!("{:?} has been write accessed, update it in storage", page)
        });
        return new_pages_data;
    }

    let mut page_update = BTreeMap::new();
    let mut old_pages_data = old_pages_data;
    let static_gear_pages = static_pages.to_gear_page();
    for (page, new_data) in new_pages_data {
        let initial_data = if let Some(initial_data) = old_pages_data.remove(&page) {
            initial_data
        } else {
            // If it's static page without initial data,
            // then it's stack page and we skip this page update.
            if page < static_gear_pages {
                continue;
            }

            // If page has no data in `pages_initial_data` then data is zeros.
            // Because it's default data for wasm pages which is not static,
            // and for all static pages we save data in `pages_initial_data` in E::new.
            PageBuf::new_zeroed()
        };

        if new_data != initial_data {
            page_update.insert(page, new_data);
            log::trace!(
                "Page {} has been changed - will be updated in storage",
                page.0
            );
        }
    }
    page_update
}

#[allow(clippy::result_large_err)]
/// Execute wasm with dispatch and return dispatch result.
pub fn execute_wasm<
    A: ProcessorExt + EnvExt + IntoExtInfo<<A as EnvExt>::Error> + 'static,
    E: Environment<A>,
>(
    balance: u128,
    dispatch: IncomingDispatch,
    context: WasmExecutionContext,
    settings: ExecutionSettings,
    msg_ctx_settings: ContextSettings,
) -> Result<DispatchResult, ExecutionError> {
    let WasmExecutionContext {
        gas_counter,
        gas_allowance_counter,
        gas_reserver,
        origin,
        program,
        mut pages_initial_data,
        memory_size,
    } = context;

    let program_id = program.id();
    let kind = dispatch.kind();

    log::debug!("Executing program {}", program_id);
    log::debug!("Executing dispatch {:?}", dispatch);

    let static_pages = program.static_pages();
    let allocations = program.allocations();

    if let Err(reason) = check_memory(
        allocations,
        pages_initial_data.keys(),
        static_pages,
        memory_size,
    ) {
        return Err(ExecutionError {
            program_id,
            gas_amount: gas_counter.into(),
            reason,
        });
    }

    // Creating allocations context.
    let allocations_context =
        AllocationsContext::new(allocations.clone(), static_pages, settings.max_pages());

    // Creating message context.
    let message_context = MessageContext::new(
        dispatch.message().clone(),
        program_id,
        dispatch.context().clone(),
        msg_ctx_settings,
    );

    // Creating value counter.
    let value_counter = ValueCounter::new(balance + dispatch.value());

    let context = ProcessorContext {
        gas_counter,
        gas_allowance_counter,
        gas_reserver,
        system_reservation: None,
        value_counter,
        allocations_context,
        message_context,
        block_info: settings.block_info,
        config: settings.allocations_config,
        existential_deposit: settings.existential_deposit,
        origin,
        program_id,
        program_candidates_data: Default::default(),
        host_fn_weights: settings.host_fn_weights,
        forbidden_funcs: settings.forbidden_funcs,
        mailbox_threshold: settings.mailbox_threshold,
        waitlist_cost: settings.waitlist_cost,
        reserve_for: settings.reserve_for,
        reservation: settings.reservation,
        random_data: settings.random_data,
    };

    // Creating externalities.
    let ext = A::new(context);

    // Execute program in backend env.
    let f = || {
        let env = E::new(
            ext,
            program.raw_code(),
            program.code().exports().clone(),
            memory_size,
        )?;
        env.execute(kind, |memory, stack_end| {
            prepare_memory::<A, E::Memory>(
                program_id,
                &mut pages_initial_data,
                static_pages,
                stack_end,
                memory,
            )
        })
    };
    let (termination, memory, ext) = match f() {
        Ok(BackendReport {
            termination_reason: termination,
            memory_wrap: mut memory,
            ext,
        }) => {
            // released pages initial data will be added to `pages_initial_data` after execution.
            if A::LAZY_PAGES_ENABLED {
                A::lazy_pages_post_execution_actions(&mut memory);
            }

            (termination, memory, ext)
        }

        Err(e) => {
            return Err(ExecutionError {
                program_id,
                gas_amount: e.gas_amount(),
                reason: ExecutionErrorReason::Backend(e.to_string()),
            })
        }
    };

    log::debug!("Termination reason: {:?}", termination);

    let info = ext
        .into_ext_info(&memory)
        .map_err(|(err, gas_amount)| ExecutionError {
            program_id,
            gas_amount,
            reason: ExecutionErrorReason::Backend(err.to_string()),
        })?;

    if A::LAZY_PAGES_ENABLED && !pages_initial_data.is_empty() {
        return Err(ExecutionError {
            program_id,
            gas_amount: info.gas_amount,
            reason: ExecutionErrorReason::InitialPagesContainsDataInLazyPagesMode,
        });
    }

    // Parsing outcome.
    let kind = match termination {
        TerminationReason::Exit(value_dest) => DispatchResultKind::Exit(value_dest),
        TerminationReason::Leave | TerminationReason::Success => DispatchResultKind::Success,
        TerminationReason::Trap(explanation) => {
            log::debug!(
                "💥 Trap during execution of {}\n📔 Explanation: {}",
                program_id,
                explanation,
            );

            DispatchResultKind::Trap(explanation)
        }
        TerminationReason::Wait(duration, waited_type) => {
            DispatchResultKind::Wait(duration, waited_type)
        }
        TerminationReason::GasAllowanceExceeded => DispatchResultKind::GasAllowanceExceed,
    };

    let page_update =
        get_pages_to_be_updated::<A>(pages_initial_data, info.pages_data, static_pages);

    // Getting new programs that are scheduled to be initialized (respected messages are in `generated_dispatches` collection)
    let program_candidates = info.program_candidates_data;

    // Output
    Ok(DispatchResult {
        kind,
        dispatch,
        program_id,
        context_store: info.context_store,
        generated_dispatches: info.generated_dispatches,
        awakening: info.awakening,
        program_candidates,
        gas_amount: info.gas_amount,
        gas_reserver: Some(info.gas_reserver),
        system_reservation_context: info.system_reservation_context,
        page_update,
        allocations: info.allocations,
    })
}

/// !!! FOR TESTING / INFORMATIONAL USAGE ONLY
pub fn execute_for_reply<
    A: ProcessorExt + EnvExt + IntoExtInfo<<A as EnvExt>::Error> + 'static,
    E: Environment<A>,
>(
    code: Code,
    function: String,
    payload: Vec<u8>,
) -> Result<Vec<u8>, &'static str> {
    let code_and_id = CodeAndId::new(code);
    let instrumented_code_and_id: InstrumentedCodeAndId = code_and_id.into();
    let instrumented_code = instrumented_code_and_id.into_parts().0;

    let program = Program::new(ProgramId::from(0), instrumented_code);
    let memory_size = program.static_pages();
    let mut pages_initial_data = Default::default();
    let static_pages = program.static_pages();
    let allocations = program.allocations();

    let context = ProcessorContext {
        gas_counter: GasCounter::new(500_000_000_000),
        gas_allowance_counter: GasAllowanceCounter::new(500_000_000_000),
        gas_reserver: GasReserver::new(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        ),
        value_counter: ValueCounter::new(Default::default()),
        allocations_context: AllocationsContext::new(allocations.clone(), static_pages, 512.into()),
        message_context: MessageContext::new(
            IncomingMessage::new(
                Default::default(),
                Default::default(),
                payload
                    .try_into()
                    .map_err(|_| "Failed to convert payload")?,
                500_000_000_000,
                Default::default(),
                Default::default(),
            ),
            program.id(),
            None,
            ContextSettings::new(0, 0, 0, 0, 0),
        ),
        block_info: BlockInfo {
            height: Default::default(),
            timestamp: Default::default(),
        },
        config: AllocationsConfig {
            max_pages: 512.into(),
            init_cost: Default::default(),
            alloc_cost: Default::default(),
            mem_grow_cost: Default::default(),
            load_page_cost: Default::default(),
        },
        existential_deposit: Default::default(),
        origin: Default::default(),
        program_id: Default::default(),
        program_candidates_data: Default::default(),
        host_fn_weights: Default::default(),
        forbidden_funcs: Default::default(),
        mailbox_threshold: Default::default(),
        waitlist_cost: Default::default(),
        reserve_for: Default::default(),
        reservation: Default::default(),
        random_data: Default::default(),
        system_reservation: Default::default(),
    };

    // Creating externalities.
    let ext = A::new(context);

    // Execute program in backend env.
    let f = || {
        let env = E::new(
            ext,
            program.raw_code(),
            program.code().exports().clone(),
            memory_size,
        )?;
        env.execute(function, |memory, stack_end| {
            prepare_memory::<A, E::Memory>(
                program.id(),
                &mut pages_initial_data,
                static_pages,
                stack_end,
                memory,
            )
        })
    };

    let (termination, memory, ext) = match f() {
        Ok(BackendReport {
            termination_reason: termination,
            memory_wrap: memory,
            ext,
        }) => (termination, memory, ext),
        _ => return Err("Backend error"),
    };

    if !matches!(termination, TerminationReason::Success) {
        return Err("Program execution wasn't succeed");
    }

    let info = ext.into_ext_info(&memory).map_err(|_| "Backend error")?;

    for (dispatch, _, _) in info.generated_dispatches {
        if matches!(dispatch.kind(), DispatchKind::Reply) {
            return Ok(dispatch.payload().to_vec());
        }
    }

    Err("Reply not found")
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{vec, vec::Vec};
    use gear_core::memory::WasmPageNumber;

    struct TestExt;
    struct LazyTestExt;

    impl ProcessorExt for TestExt {
        const LAZY_PAGES_ENABLED: bool = false;
        fn new(_context: ProcessorContext) -> Self {
            Self
        }

        fn lazy_pages_init_for_program(
            _mem: &mut impl Memory,
            _prog_id: ProgramId,
            _stack_end: Option<WasmPageNumber>,
        ) {
        }

        fn lazy_pages_post_execution_actions(_mem: &mut impl Memory) {}
    }

    impl ProcessorExt for LazyTestExt {
        const LAZY_PAGES_ENABLED: bool = true;

        fn new(_context: ProcessorContext) -> Self {
            Self
        }

        fn lazy_pages_init_for_program(
            _mem: &mut impl Memory,
            _prog_id: ProgramId,
            _stack_end: Option<WasmPageNumber>,
        ) {
        }

        fn lazy_pages_post_execution_actions(_mem: &mut impl Memory) {}
    }

    fn prepare_pages_and_allocs() -> (Vec<PageNumber>, BTreeSet<WasmPageNumber>) {
        let data = [0, 1, 2, 8, 18, 25, 27, 28, 93, 146, 240, 518];
        let pages = data.map(PageNumber);
        let allocs = data.map(|p| WasmPageNumber(p / PageNumber::num_in_one_wasm_page()));
        (pages.to_vec(), allocs.into())
    }

    fn prepare_alloc_config() -> AllocationsConfig {
        AllocationsConfig {
            max_pages: 32.into(),
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

    fn prepare_pages() -> BTreeMap<PageNumber, PageBuf> {
        let mut pages = BTreeMap::new();
        for i in 0..=255 {
            pages.insert(
                (i as u32).into(),
                PageBuf::new_from_vec(vec![i; 4096]).unwrap(),
            );
        }
        pages
    }

    #[test]
    fn check_memory_insufficient() {
        let res = check_memory(&[].into(), [].iter(), 8.into(), 4.into());
        assert_eq!(res, Err(ExecutionErrorReason::InsufficientMemorySize));
    }

    #[test]
    fn check_memory_not_allocated() {
        let (pages, mut allocs) = prepare_pages_and_allocs();
        let last = *allocs.iter().last().unwrap();
        allocs.remove(&last);
        let res = check_memory(&allocs, pages.iter(), 2.into(), 4.into());
        assert_eq!(
            res,
            Err(ExecutionErrorReason::PageIsNotAllocated(
                *pages.last().unwrap()
            ))
        );
    }

    #[test]
    fn check_memory_ok() {
        let (pages, allocs) = prepare_pages_and_allocs();
        let res = check_memory(&allocs, pages.iter(), 4.into(), 8.into());
        assert!(res.is_ok());
    }

    #[test]
    fn gas_for_pages_initial() {
        let settings = prepare_alloc_config();
        let (mut counter, mut allowance_counter) = prepare_gas_counters();
        let static_pages = 4u32;
        let res = charge_gas_for_pages(
            &settings,
            &mut counter,
            &mut allowance_counter,
            &Default::default(),
            static_pages.into(),
            true,
            false,
        );
        // Result is static pages count
        assert_eq!(res, Ok(static_pages.into()));
        // Charging for static pages initialization
        let charge = settings.init_cost * static_pages as u64;
        assert_eq!(counter.left(), 1_000_000 - charge);
        assert_eq!(allowance_counter.left(), 4_000_000 - charge);
    }

    #[test]
    fn gas_for_pages_static() {
        let settings = prepare_alloc_config();
        let (mut counter, mut allowance_counter) = prepare_gas_counters();
        let static_pages = 4u32;
        let res = charge_gas_for_pages(
            &settings,
            &mut counter,
            &mut allowance_counter,
            &Default::default(),
            static_pages.into(),
            false,
            false,
        );
        // Result is static pages count
        assert_eq!(res, Ok(static_pages.into()));
        // Charge for the first load of static pages
        let charge = settings.load_page_cost * static_pages as u64;
        assert_eq!(counter.left(), 1_000_000 - charge);
        assert_eq!(allowance_counter.left(), 4_000_000 - charge);
    }

    #[test]
    fn gas_for_pages_alloc() {
        let settings = prepare_alloc_config();
        let (mut counter, mut allowance_counter) = prepare_gas_counters();
        let (_, allocs) = prepare_pages_and_allocs();
        let static_pages = 4u32;
        let res = charge_gas_for_pages(
            &settings,
            &mut counter,
            &mut allowance_counter,
            &allocs,
            static_pages.into(),
            false,
            false,
        );
        // Result is the last page plus one
        let last = *allocs.iter().last().unwrap();
        assert_eq!(res, Ok(last + 1.into()));
        // Charge for loading and mem grow
        let load_charge = settings.load_page_cost * (allocs.len() as u64 + static_pages as u64);
        let grow_charge = settings.mem_grow_cost * (last.0 as u64 + 1 - static_pages as u64);
        assert_eq!(counter.left(), 1_000_000 - load_charge - grow_charge);
        assert_eq!(
            allowance_counter.left(),
            4_000_000 - load_charge - grow_charge
        );

        // Use the second time (`subsequent` = `true`)
        let (mut counter, mut allowance_counter) = prepare_gas_counters();
        let res = charge_gas_for_pages(
            &settings,
            &mut counter,
            &mut allowance_counter,
            &allocs,
            static_pages.into(),
            false,
            true,
        );
        assert_eq!(res, Ok(last + 1.into()));
        // Charge for mem grow only
        assert_eq!(counter.left(), 1_000_000 - grow_charge);
        assert_eq!(allowance_counter.left(), 4_000_000 - grow_charge);
    }

    #[test]
    fn lazy_pages_to_update() {
        let new_pages = prepare_pages();
        let res =
            get_pages_to_be_updated::<LazyTestExt>(Default::default(), new_pages.clone(), 0.into());
        // All touched pages are to be updated in lazy mode
        assert_eq!(res, new_pages);
    }

    #[test]
    fn no_pages_to_update() {
        let old_pages = prepare_pages();
        let mut new_pages = old_pages.clone();
        let static_pages = 4u32;
        let res =
            get_pages_to_be_updated::<TestExt>(old_pages, new_pages.clone(), static_pages.into());
        assert_eq!(res, Default::default());

        // Change static pages
        for i in 0..static_pages {
            new_pages.insert(i.into(), PageBuf::new_from_vec(vec![42u8; 4096]).unwrap());
        }
        // Do not include non-static pages
        let new_pages = new_pages
            .into_iter()
            .take(WasmPageNumber(static_pages).to_gear_page().0 as _)
            .collect();
        let res =
            get_pages_to_be_updated::<TestExt>(Default::default(), new_pages, static_pages.into());
        assert_eq!(res, Default::default());
    }

    #[test]
    fn pages_to_update() {
        let old_pages = prepare_pages();
        let mut new_pages = prepare_pages();

        // Change pages
        new_pages.insert(1.into(), PageBuf::new_from_vec(vec![42u8; 4096]).unwrap());
        new_pages.insert(5.into(), PageBuf::new_from_vec(vec![84u8; 4096]).unwrap());
        new_pages.insert(30.into(), PageBuf::new_zeroed());
        let static_pages = 4u32.into();
        let res = get_pages_to_be_updated::<TestExt>(old_pages, new_pages.clone(), static_pages);
        assert_eq!(
            res,
            [
                (1.into(), PageBuf::new_from_vec(vec![42u8; 4096]).unwrap()),
                (5.into(), PageBuf::new_from_vec(vec![84u8; 4096]).unwrap()),
                (30.into(), PageBuf::new_zeroed())
            ]
            .into()
        );

        // There was no any old page
        let res =
            get_pages_to_be_updated::<TestExt>(Default::default(), new_pages.clone(), static_pages);
        // The result is all pages except the static ones
        (0..static_pages.to_gear_page().0).for_each(|i| {
            new_pages.remove(&i.into());
        });
        assert_eq!(res, new_pages,);
    }
}
