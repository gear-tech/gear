// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use super::*;
use arbitrary::Unstructured;
use gear_backend_common::{TerminationReason, TrapExplanation};
use gear_core::{
    code::Code,
    ids::{MessageId, ProgramId},
    memory::Memory,
    message::{IncomingMessage, Payload},
    pages::WASM_PAGE_SIZE,
};
use gear_utils::NonEmpty;
use gear_wasm_instrument::{
    parity_wasm::{
        self,
        elements::{Instruction, Module},
    },
    syscalls::ParamType,
};
use proptest::prelude::*;
use rand::{rngs::SmallRng, RngCore, SeedableRng};
use std::mem;

const UNSTRUCTURED_SIZE: usize = 1_000_000;

#[test]
fn remove_trivial_recursions() {
    let wat1 = r#"
    (module
        (func (;0;)
            call 0
        )
    )"#;

    let wasm_bytes = wat::parse_str(wat1).expect("invalid wat");
    let module =
        parity_wasm::deserialize_buffer::<Module>(&wasm_bytes).expect("invalid wasm bytes");
    let no_recursions_module = utils::remove_recursion(module);

    let wasm_bytes = no_recursions_module
        .into_bytes()
        .expect("invalid pw module");
    assert!(wasmparser::validate(&wasm_bytes).is_ok());

    let wat = wasmprinter::print_bytes(&wasm_bytes).expect("failed printing bytes");
    println!("wat = {wat}");
}

#[test]
fn remove_multiple_recursions() {
    let wat2 = r#"
    (module
        (func (;0;) (result i64)
            call 1
        )
        (func (;1;) (result i64)
            call 0
        )
        (func (;2;)
            call 1
            drop
        )
    )"#;

    let wasm_bytes = wat::parse_str(wat2).expect("invalid wat");
    let module =
        parity_wasm::deserialize_buffer::<Module>(&wasm_bytes).expect("invalid wasm bytes");
    utils::find_recursion(&module, |path, call| {
        println!("path = {path:?}, call = {call}");
    });
    let no_recursions_module = utils::remove_recursion(module);
    utils::find_recursion(&no_recursions_module, |_, _| {
        unreachable!("there should be no recursions")
    });

    let wasm_bytes = no_recursions_module
        .into_bytes()
        .expect("invalid pw module");
    assert!(wasmparser::validate(&wasm_bytes).is_ok());

    let wat = wasmprinter::print_bytes(&wasm_bytes).expect("failed printing bytes");
    println!("wat = {wat}");
}

#[test]
fn injecting_addresses_works() {
    let mut rng = SmallRng::seed_from_u64(1234);
    let mut buf = vec![0; UNSTRUCTURED_SIZE];
    rng.fill_bytes(&mut buf);
    let mut u = Unstructured::new(&buf);

    let stack_end_page = 16;
    let addresses = NonEmpty::from_vec(vec![[0; 32], [1; 32]]).expect("vec wasn't empty");
    let config = GearWasmGeneratorConfigBuilder::new()
        .with_memory_config(MemoryPagesConfig {
            initial_size: 17,
            upper_limit: None,
            stack_end_page: Some(stack_end_page),
        })
        .with_sys_calls_config(
            SysCallsConfigBuilder::new(Default::default())
                .with_data_offset_msg_dest(addresses)
                .build(),
        )
        .build();
    let wasm_module = GearWasmGenerator::new_with_config(
        WasmModule::generate(&mut u).expect("failed module generation"),
        &mut u,
        config,
    )
    .generate()
    .expect("failed gear-wasm generation");

    let data_sections_entries_num = wasm_module
        .data_section()
        .expect("additional data was inserted")
        .entries()
        .len();
    // 2 addresses in the upper `addresses`.
    assert_eq!(data_sections_entries_num, 2);

    let size = mem::size_of::<gsys::HashWithValue>() as i32;
    let entries = wasm_module
        .data_section()
        .expect("additional data was inserted")
        .entries();

    let first_addr_offset = entries
        .get(0)
        .and_then(|segment| segment.offset().as_ref())
        .map(|expr| &expr.code()[0])
        .expect("checked");
    let Instruction::I32Const(ptr) = first_addr_offset else {
        panic!("invalid instruction in init expression")
    };
    // No additional data, except for addresses.
    // First entry set to the 0 offset.
    assert_eq!(*ptr, (stack_end_page * WASM_PAGE_SIZE as u32) as i32);

    let second_addr_offset = entries
        .get(1)
        .and_then(|segment| segment.offset().as_ref())
        .map(|expr| &expr.code()[0])
        .expect("checked");
    let Instruction::I32Const(ptr) = second_addr_offset else {
        panic!("invalid instruction in init expression")
    };
    // No additional data, except for addresses.
    // First entry set to the 0 offset.
    assert_eq!(*ptr, size + (stack_end_page * WASM_PAGE_SIZE as u32) as i32);
}

#[test]
fn error_processing_works_for_fallible_syscalls() {
    use gear_backend_common::ActorTerminationReason;

    let fallible_syscalls = SysCallName::instrumentable()
        .into_iter()
        .filter(|sc| InvocableSysCall::Loose(*sc).is_fallible());

    for syscall in fallible_syscalls {
        let (params_config, initial_memory_write, payload) =
            get_params_for_syscall_to_fail(&syscall);

        // Assert that syscalls results will be processed.
        let termination_reason = execute_wasm_with_syscall_injected(
            syscall,
            false,
            params_config.clone(),
            initial_memory_write.clone(),
            payload.clone(),
        );

        assert_eq!(
            termination_reason,
            TerminationReason::Actor(ActorTerminationReason::Trap(TrapExplanation::Unknown)),
            "syscall: {}",
            syscall.to_str()
        );

        // Assert that syscall results will be ignored.
        let termination_reason = execute_wasm_with_syscall_injected(
            syscall,
            true,
            params_config,
            initial_memory_write,
            payload,
        );

        assert_eq!(
            termination_reason,
            TerminationReason::Actor(ActorTerminationReason::Success),
            "syscall: {}",
            syscall.to_str()
        );
    }
}

#[derive(Clone)]
struct MemoryWrite {
    offset: u32,
    content: Vec<u8>,
}

fn get_params_for_syscall_to_fail(
    syscall: &SysCallName,
) -> (SysCallsParamsConfig, Option<MemoryWrite>, Payload) {
    match *syscall {
        SysCallName::ReplyPush => {
            let mut params_config = SysCallsParamsConfig::constant_value(i32::MAX as i64);
            params_config.add_rule(ParamType::Ptr(None), SysCallParamAllowedValues::zero());

            (params_config, None, Payload::default())
        }
        SysCallName::ReplyPushInput => {
            let mut params_config = SysCallsParamsConfig::constant_value(i32::MAX as i64);
            params_config.add_rule(ParamType::Ptr(None), SysCallParamAllowedValues::zero());

            let mut payload = Payload::default();
            payload.extend_with(0);

            params_config.add_rule(
                ParamType::Size,
                // offset = length = MAX_PAYLOAD_SIZE / 2, so MAX_PAYLOAD_SIZE / 2 would be read
                // when the payload with max possible length is provided.
                SysCallParamAllowedValues::constant(payload.inner().len() as i64 / 2),
            );

            (params_config, None, payload)
        }
        SysCallName::Read => {
            let mut params_config = SysCallsParamsConfig::constant_value(0);
            params_config.add_rule(
                ParamType::Size,
                SysCallParamAllowedValues::constant(i32::MAX as i64),
            );
            (params_config, None, Payload::default())
        }
        SysCallName::PayProgramRent => (
            SysCallsParamsConfig::constant_value(0),
            Some(MemoryWrite {
                offset: 0,
                content: std::iter::repeat(255).take(128).collect(),
            }),
            Payload::default(),
        ),
        _ => (
            SysCallsParamsConfig::constant_value(0),
            None,
            Payload::default(),
        ),
    }
}

fn execute_wasm_with_syscall_injected(
    syscall: SysCallName,
    ignore_fallible_errors: bool,
    params_config: SysCallsParamsConfig,
    initial_memory_write: Option<MemoryWrite>,
    payload: Payload,
) -> TerminationReason {
    use gear_backend_common::{BackendReport, Environment};
    use gear_backend_wasmi::WasmiEnvironment;
    use gear_core::{
        gas::{GasAllowanceCounter, GasCounter, ValueCounter},
        memory::AllocationsContext,
        message::{ContextSettings, DispatchKind, IncomingDispatch, MessageContext},
        reservation::GasReserver,
    };
    use gear_core_processor::{configs::PageCosts, ProcessorContext, ProcessorExternalities};

    const INITIAL_PAGES: u16 = 1;
    const INJECTED_SYSCALLS: u32 = 8;

    let buf = vec![0; UNSTRUCTURED_SIZE];
    let mut unstructured = Unstructured::new(&buf);

    let mut injection_amounts = SysCallsInjectionAmounts::all_never();
    injection_amounts.set(syscall, INJECTED_SYSCALLS, INJECTED_SYSCALLS);

    let gear_config = (
        GearWasmGeneratorConfigBuilder::new()
            .with_memory_config(MemoryPagesConfig {
                initial_size: INITIAL_PAGES as u32,
                ..MemoryPagesConfig::default()
            })
            .with_sys_calls_config(
                SysCallsConfigBuilder::new(injection_amounts)
                    .with_params_config(params_config)
                    .set_ignore_fallible_syscall_errors(ignore_fallible_errors)
                    .build(),
            )
            .with_entry_points_config(EntryPointsSet::Init)
            .with_recursions_removed(true)
            .build(),
        SelectableParams {
            call_indirect_enabled: false,
            allowed_instructions: vec![],
            max_instructions: 0,
            min_funcs: 1,
            max_funcs: 1,
        },
    );

    let module = generate_gear_program_module(&mut unstructured, gear_config)
        .expect("failed wasm generation");

    let module = gear_wasm_instrument::inject(
        module,
        &gear_wasm_instrument::rules::CustomConstantCostRules::new(0, 0, 0),
        "env",
    )
    .unwrap();
    let code = module.into_bytes().unwrap();

    let init_msg = IncomingMessage::new(
        MessageId::default(),
        ProgramId::default(),
        payload,
        0,
        0,
        None,
    );

    let default_pc = ProcessorContext {
        gas_counter: GasCounter::new(0),
        gas_allowance_counter: GasAllowanceCounter::new(0),
        gas_reserver: GasReserver::new(
            &<IncomingDispatch as Default>::default(),
            Default::default(),
            Default::default(),
        ),
        system_reservation: None,
        value_counter: ValueCounter::new(0),
        allocations_context: AllocationsContext::new(
            Default::default(),
            Default::default(),
            Default::default(),
        ),
        message_context: MessageContext::new(
            IncomingDispatch::new(DispatchKind::Init, init_msg, None),
            Default::default(),
            ContextSettings::new(0, 0, 0, 0, 0, 0),
        ),
        block_info: Default::default(),
        max_pages: INITIAL_PAGES.into(),
        page_costs: PageCosts::new_for_tests(),
        existential_deposit: 0,
        program_id: Default::default(),
        program_candidates_data: Default::default(),
        program_rents: Default::default(),
        host_fn_weights: Default::default(),
        forbidden_funcs: Default::default(),
        mailbox_threshold: 0,
        waitlist_cost: 0,
        dispatch_hold_cost: 0,
        reserve_for: 0,
        reservation: 0,
        random_data: ([0u8; 32].to_vec(), 0),
        rent_cost: 10,
    };

    let ext = gear_core_processor::Ext::new(default_pc);
    let env = WasmiEnvironment::new(
        ext,
        &code,
        DispatchKind::Init,
        vec![DispatchKind::Init].into_iter().collect(),
        INITIAL_PAGES.into(),
    )
    .unwrap();

    let report = env
        .execute(|mem, _, _| -> Result<(), u32> {
            if let Some(mem_write) = initial_memory_write {
                return mem
                    .write(mem_write.offset, &mem_write.content)
                    .map_err(|_| 1);
            };

            Ok(())
        })
        .unwrap();

    let BackendReport {
        termination_reason, ..
    } = report;

    termination_reason
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    #[test]
    // Test that valid config always generates a valid gear wasm.
    fn test_standard_config(buf in prop::collection::vec(any::<u8>(), UNSTRUCTURED_SIZE)) {
        use gear_wasm_instrument::rules::CustomConstantCostRules;
        let mut u = Unstructured::new(&buf);
        let configs_bundle: StandardGearWasmConfigsBundle = StandardGearWasmConfigsBundle {
            log_info: Some("Some data".into()),
            entry_points_set: EntryPointsSet::InitHandleHandleReply,
            ..Default::default()
        };

        let raw_code = generate_gear_program_code(&mut u, configs_bundle)
            .expect("failed generating wasm");

        let code_res = Code::try_new(raw_code, 1, |_| CustomConstantCostRules::default(), None);
        assert!(code_res.is_ok());
    }

    #[test]
    fn test_reproduction(buf in prop::collection::vec(any::<u8>(), UNSTRUCTURED_SIZE)) {
        let mut u = Unstructured::new(&buf);
        let mut u2 = Unstructured::new(&buf);

        let gear_config = StandardGearWasmConfigsBundle::<[u8; 32]>::default();

        let first = generate_gear_program_code(&mut u, gear_config.clone()).expect("failed wasm generation");
        let second = generate_gear_program_code(&mut u2, gear_config).expect("failed wasm generation");

        assert_eq!(first, second);
    }
}
