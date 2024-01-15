// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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
use gear_core::{
    code::Code,
    gas::ValueCounter,
    ids::{CodeId, ProgramId},
    memory::Memory,
    message::{
        ContextSettings, DispatchKind, IncomingDispatch, IncomingMessage, MessageContext,
        ReplyPacket,
    },
    pages::WASM_PAGE_SIZE,
};
use gear_core_backend::{
    env::{BackendReport, Environment},
    error::{ActorTerminationReason, TerminationReason, TrapExplanation},
};
use gear_core_processor::{ProcessorContext, ProcessorExternalities};
use gear_utils::NonEmpty;
use gear_wasm_instrument::{
    parity_wasm::{self, elements::Module},
    rules::CustomConstantCostRules,
};
use proptest::prelude::*;
use rand::{rngs::SmallRng, RngCore, SeedableRng};
use std::num::NonZeroUsize;

const UNSTRUCTURED_SIZE: usize = 1_000_000;

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

        let original_code = generate_gear_program_code(&mut u, configs_bundle)
            .expect("failed generating wasm");

        let code_res = Code::try_new(original_code, 1, |_| CustomConstantCostRules::default(), None);
        assert!(code_res.is_ok());
    }

    #[test]
    fn test_reproduction(buf in prop::collection::vec(any::<u8>(), UNSTRUCTURED_SIZE)) {
        let mut u = Unstructured::new(&buf);
        let mut u2 = Unstructured::new(&buf);

        let gear_config = StandardGearWasmConfigsBundle::default();

        let first = generate_gear_program_code(&mut u, gear_config.clone()).expect("failed wasm generation");
        let second = generate_gear_program_code(&mut u2, gear_config).expect("failed wasm generation");

        assert_eq!(first, second);
    }
}

#[test]
fn inject_critical_gas_limit_works() {
    let wat1 = r#"
    (module
        (memory $memory0 (import "env" "memory") 16)
        (export "handle" (func $handle))
        (func $handle
            call $f
            drop
        )
        (func $f (result i64)
            call $f
        )
        (func $g
            (loop $my_loop
                br $my_loop
            )
        )
    )"#;

    let wasm_bytes = wat::parse_str(wat1).expect("invalid wat");
    let module =
        parity_wasm::deserialize_buffer::<Module>(&wasm_bytes).expect("invalid wasm bytes");
    let module_with_critical_gas_limit = utils::inject_critical_gas_limit(module, 1_000_000);

    let wasm_bytes = module_with_critical_gas_limit
        .into_bytes()
        .expect("invalid pw module");
    assert!(wasmparser::validate(&wasm_bytes).is_ok());

    let wat = wasmprinter::print_bytes(&wasm_bytes).expect("failed printing bytes");
    println!("wat = {wat}");
}

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
fn test_source_as_address_param() {
    let mut rng = SmallRng::seed_from_u64(123);
    let mut buf = vec![0; UNSTRUCTURED_SIZE];
    rng.fill_bytes(&mut buf);
    let mut unstructured = Unstructured::new(&buf);

    let params_config = SyscallsParamsConfig::new()
        .with_default_regular_config()
        .with_ptr_rule(PtrParamAllowedValues::ActorId(ActorKind::Source));

    let mut injection_types = SyscallsInjectionTypes::all_never();
    injection_types.set(InvocableSyscall::Loose(SyscallName::Exit), 1, 1);
    let syscalls_config = SyscallsConfigBuilder::new(injection_types)
        .with_params_config(params_config)
        .with_error_processing_config(ErrorProcessingConfig::All)
        .build();

    let backend_report =
        execute_wasm_with_custom_configs(&mut unstructured, syscalls_config, None, 1024, false, 0);

    assert_eq!(
        backend_report.termination_reason,
        TerminationReason::Actor(ActorTerminationReason::Exit(message_sender()))
    );
}

#[test]
fn test_existing_address_as_address_param() {
    let mut rng = SmallRng::seed_from_u64(123);
    let mut buf = vec![0; UNSTRUCTURED_SIZE];
    rng.fill_bytes(&mut buf);
    let mut unstructured = Unstructured::new(&buf);

    let some_address = [5; 32];
    let params_config = SyscallsParamsConfig::new()
        .with_default_regular_config()
        .with_ptr_rule(PtrParamAllowedValues::ActorIdWithValue {
            actor_kind: ActorKind::ExistingAddresses(NonEmpty::new(some_address)),
            range: 0..=0,
        });

    let mut injection_types = SyscallsInjectionTypes::all_never();
    injection_types.set(InvocableSyscall::Loose(SyscallName::Send), 1, 1);
    let syscalls_config = SyscallsConfigBuilder::new(injection_types)
        .with_params_config(params_config)
        .with_error_processing_config(ErrorProcessingConfig::All)
        .build();

    let backend_report =
        execute_wasm_with_custom_configs(&mut unstructured, syscalls_config, None, 1024, false, 0);

    assert_eq!(
        backend_report.termination_reason,
        TerminationReason::Actor(ActorTerminationReason::Success)
    );

    let dispatch = {
        let (context_outcome, _) = backend_report.ext.context.message_context.drain();

        let mut dispatches = context_outcome.drain().outgoing_dispatches;
        assert_eq!(dispatches.len(), 1);

        dispatches.pop().expect("checked").0
    };

    assert_eq!(
        dispatch.destination(),
        ProgramId::from(some_address.as_ref())
    );
}

// Syscalls of a `gr_*reply*` kind are the only of those, which has `Value` input param.
// Message value param for these syscalls is set during the common syscalls params
// processing flow.
#[test]
fn test_msg_value_ptr() {
    const INITIAL_BALANCE: u128 = 10_000;
    const REPLY_VALUE: u128 = 1_000;

    let mut rng = SmallRng::seed_from_u64(123);
    let mut buf = vec![0; UNSTRUCTURED_SIZE];
    rng.fill_bytes(&mut buf);
    let mut unstructured = Unstructured::new(&buf);

    let params_config = SyscallsParamsConfig::new()
        .with_default_regular_config()
        .with_ptr_rule(PtrParamAllowedValues::Value(REPLY_VALUE..=REPLY_VALUE));

    let mut injection_types = SyscallsInjectionTypes::all_never();
    injection_types.set(InvocableSyscall::Loose(SyscallName::Reply), 1, 1);
    let syscalls_config = SyscallsConfigBuilder::new(injection_types)
        .with_params_config(params_config)
        .with_error_processing_config(ErrorProcessingConfig::All)
        .build();

    let backend_report = execute_wasm_with_custom_configs(
        &mut unstructured,
        syscalls_config,
        None,
        1024,
        false,
        INITIAL_BALANCE,
    );

    assert_eq!(
        backend_report.ext.context.value_counter.left(),
        INITIAL_BALANCE - REPLY_VALUE
    );
    assert_eq!(
        backend_report.termination_reason,
        TerminationReason::Actor(ActorTerminationReason::Success)
    );
}

// Syscalls which have destination with value param, i.e. `HashWithValue`.
// Params for these syscalls aren't processed the usual way: destination argument is
// set from existing config or set by calling `gr_source`. Should be mentioned that
// destination is not only 32 bytes hash value, but a struct of hash and message value.
// So here it tests that message value in this struct is properly set.
#[test]
fn test_msg_value_ptr_dest() {
    gear_utils::init_default_logger();
    const INITIAL_BALANCE: u128 = 10_000;
    const REPLY_VALUE: u128 = 1_000;

    let tested_syscalls = [
        InvocableSyscall::Loose(SyscallName::Send),
        InvocableSyscall::Loose(SyscallName::SendInput),
        InvocableSyscall::Precise(SyscallName::ReservationSend),
        InvocableSyscall::Precise(SyscallName::SendCommit),
        InvocableSyscall::Precise(SyscallName::ReplyDeposit),
    ];

    let some_address = [10; 32];
    let destination_variants = [
        ActorKind::Random,
        ActorKind::Source,
        ActorKind::ExistingAddresses(NonEmpty::new(some_address)),
    ];
    for dest_var in destination_variants {
        let params_config = SyscallsParamsConfig::new()
            .with_default_regular_config()
            .with_rule(RegularParamType::Gas, (0..=0).into())
            .with_ptr_rule(PtrParamAllowedValues::ActorIdWithValue {
                actor_kind: dest_var.clone(),
                range: REPLY_VALUE..=REPLY_VALUE,
            });

        for syscall in tested_syscalls {
            let mut rng = SmallRng::seed_from_u64(123);
            let mut buf = vec![0; UNSTRUCTURED_SIZE];
            rng.fill_bytes(&mut buf);
            let mut unstructured = Unstructured::new(&buf);

            let mut injection_types = SyscallsInjectionTypes::all_never();
            injection_types.set(syscall, 1, 1);
            let syscalls_config = SyscallsConfigBuilder::new(injection_types)
                .with_params_config(params_config.clone())
                .with_error_processing_config(ErrorProcessingConfig::All)
                .build();

            let backend_report = execute_wasm_with_custom_configs(
                &mut unstructured,
                syscalls_config,
                None,
                1024,
                false,
                INITIAL_BALANCE,
            );

            assert_eq!(
                backend_report.ext.context.value_counter.left(),
                INITIAL_BALANCE - REPLY_VALUE
            );
            assert_eq!(
                backend_report.termination_reason,
                TerminationReason::Actor(ActorTerminationReason::Success)
            );

            if !dest_var.is_random() {
                let dispatch = {
                    let (context_outcome, _) = backend_report.ext.context.message_context.drain();

                    let mut dispatches = context_outcome.drain().outgoing_dispatches;
                    assert_eq!(dispatches.len(), 1);

                    dispatches.pop().expect("checked").0
                };
                let destination = dispatch.destination();

                match dest_var {
                    ActorKind::Source => assert_eq!(destination, message_sender()),
                    ActorKind::ExistingAddresses(_) => {
                        assert_eq!(destination, ProgramId::from(some_address.as_ref()))
                    }
                    ActorKind::Random => {}
                }
            }
        }
    }
}

#[test]
fn error_processing_works_for_fallible_syscalls() {
    gear_utils::init_default_logger();

    // We create Unstructured from zeroes here as we just need any.
    let buf = vec![0; UNSTRUCTURED_SIZE];
    let mut unstructured = Unstructured::new(&buf);
    let mut unstructured2 = Unstructured::new(&buf);

    let fallible_syscalls = SyscallName::instrumentable()
        .into_iter()
        .filter_map(|syscall| {
            if matches!(syscall, SyscallName::PayProgramRent) {
                return None;
            }

            let invocable_syscall = InvocableSyscall::Loose(syscall);
            invocable_syscall.is_fallible().then_some(invocable_syscall)
        });

    for syscall in fallible_syscalls {
        // Prepare syscalls config & context settings for test case.
        let (params_config, initial_memory_write) = get_params_for_syscall_to_fail(syscall);

        const INJECTED_SYSCALLS: u32 = 8;

        let mut injection_types = SyscallsInjectionTypes::all_never();
        injection_types.set(syscall, INJECTED_SYSCALLS, INJECTED_SYSCALLS);

        let syscalls_config_builder =
            SyscallsConfigBuilder::new(injection_types).with_params_config(params_config);

        // Assert that syscalls results will be processed.
        let termination_reason = execute_wasm_with_custom_configs(
            &mut unstructured,
            syscalls_config_builder
                .clone()
                .with_error_processing_config(ErrorProcessingConfig::All)
                .build(),
            initial_memory_write.clone(),
            0,
            true,
            0,
        )
        .termination_reason;

        assert_eq!(
            termination_reason,
            TerminationReason::Actor(ActorTerminationReason::Trap(TrapExplanation::Unknown)),
            "syscall: {}",
            syscall.to_str()
        );

        // Assert that syscall results will be ignored.
        let termination_reason = execute_wasm_with_custom_configs(
            &mut unstructured2,
            syscalls_config_builder.build(),
            initial_memory_write.clone(),
            0,
            true,
            0,
        )
        .termination_reason;

        assert_eq!(
            termination_reason,
            TerminationReason::Actor(ActorTerminationReason::Success),
            "syscall: {}",
            syscall.to_str()
        );
    }
}

#[test]
fn precise_syscalls_works() {
    use gear_core_backend::error::ActorTerminationReason;

    gear_utils::init_default_logger();

    // Pin a specific seed for this test.
    let mut rng = SmallRng::seed_from_u64(1234);
    let mut buf = vec![0; UNSTRUCTURED_SIZE];
    rng.fill_bytes(&mut buf);
    let mut unstructured = Unstructured::new(&buf);

    let precise_syscalls = SyscallName::instrumentable()
        .into_iter()
        .filter_map(|syscall| {
            InvocableSyscall::has_precise_variant(syscall)
                .then_some(InvocableSyscall::Precise(syscall))
        });

    for syscall in precise_syscalls {
        // Prepare syscalls config & context settings for test case.
        const INJECTED_SYSCALLS: u32 = 1;

        let mut injection_types = SyscallsInjectionTypes::all_never();
        injection_types.set(syscall, INJECTED_SYSCALLS, INJECTED_SYSCALLS);

        let param_config = SyscallsParamsConfig::new()
            .with_default_regular_config()
            .with_rule(RegularParamType::Gas, (0..=0).into());

        // Assert that syscalls results will be processed.
        let termination_reason = execute_wasm_with_custom_configs(
            &mut unstructured,
            SyscallsConfigBuilder::new(injection_types)
                .with_params_config(param_config)
                .with_precise_syscalls_config(PreciseSyscallsConfig::new(3..=3, 3..=3))
                .with_error_processing_config(ErrorProcessingConfig::All)
                .build(),
            None,
            1024,
            false,
            0,
        )
        .termination_reason;

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
    syscall: InvocableSyscall,
) -> (SyscallsParamsConfig, Option<MemoryWrite>) {
    let syscall_name = match syscall {
        InvocableSyscall::Loose(name) => name,
        InvocableSyscall::Precise(name) => name,
    };
    let memory_write = match syscall_name {
        SyscallName::PayProgramRent => Some(MemoryWrite {
            offset: 0,
            content: vec![255; WASM_PAGE_SIZE],
        }),
        _ => None,
    };

    (
        SyscallsParamsConfig::const_regular_params(i32::MAX as i64),
        memory_write,
    )
}

fn execute_wasm_with_custom_configs(
    unstructured: &mut Unstructured,
    syscalls_config: SyscallsConfig,
    initial_memory_write: Option<MemoryWrite>,
    outgoing_limit: u32,
    imitate_reply: bool,
    value: u128,
) -> BackendReport<gear_core_processor::Ext> {
    const PROGRAM_STORAGE_PREFIX: [u8; 32] = *b"execute_wasm_with_custom_configs";
    const INITIAL_PAGES: u16 = 1;

    assert!(gear_lazy_pages_interface::try_to_enable_lazy_pages(
        PROGRAM_STORAGE_PREFIX
    ));

    let gear_config = (
        GearWasmGeneratorConfigBuilder::new()
            .with_memory_config(MemoryPagesConfig {
                initial_size: INITIAL_PAGES as u32,
                ..MemoryPagesConfig::default()
            })
            .with_syscalls_config(syscalls_config)
            .with_entry_points_config(EntryPointsSet::Init)
            .build(),
        SelectableParams {
            allowed_instructions: vec![],
            max_instructions: 0,
            min_funcs: NonZeroUsize::new(1).unwrap(),
            max_funcs: NonZeroUsize::new(1).unwrap(),
        },
    );

    let code =
        generate_gear_program_code(unstructured, gear_config).expect("failed wasm generation");
    let code = Code::try_new(code, 1, |_| CustomConstantCostRules::new(0, 0, 0), None)
        .expect("Failed to create Code");

    let code_id = CodeId::generate(code.original_code());
    let program_id = ProgramId::generate_from_user(code_id, b"");

    let incoming_message = IncomingMessage::new(
        Default::default(),
        message_sender(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    );
    let mut message_context = MessageContext::new(
        IncomingDispatch::new(DispatchKind::Init, incoming_message, None),
        program_id,
        ContextSettings::new(0, 0, 0, 0, 0, outgoing_limit),
    );

    if imitate_reply {
        let _ = message_context.reply_commit(ReplyPacket::auto(), None);
    }

    let processor_context = ProcessorContext {
        message_context,
        max_pages: INITIAL_PAGES.into(),
        program_id,
        value_counter: ValueCounter::new(value),
        ..ProcessorContext::new_mock()
    };

    let ext = gear_core_processor::Ext::new(processor_context);
    let env = Environment::new(
        ext,
        code.code(),
        DispatchKind::Init,
        vec![DispatchKind::Init].into_iter().collect(),
        INITIAL_PAGES.into(),
    )
    .expect("Failed to create environment");

    env.execute(|mem, _stack_end, globals_config| -> Result<(), u32> {
        gear_core_processor::Ext::lazy_pages_init_for_program(
            mem,
            program_id,
            Default::default(),
            Some(mem.size()),
            globals_config,
            Default::default(),
        );

        if let Some(mem_write) = initial_memory_write {
            return mem
                .write(mem_write.offset, &mem_write.content)
                .map_err(|_| 1);
        };

        Ok(())
    })
    .expect("Failed to execute WASM module")
}

fn message_sender() -> ProgramId {
    let bytes = [1, 2, 3, 4].repeat(8);
    ProgramId::from(bytes.as_ref())
}
