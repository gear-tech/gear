// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    gas_metering::CustomConstantCostRules,
    ids::{ActorId, CodeId, prelude::*},
    memory::Memory,
    message::{
        ContextSettings, DispatchKind, IncomingDispatch, IncomingMessage, MessageContext,
        ReplyPacket,
    },
};
use gear_core_backend::{
    env::{BackendReport, Environment},
    error::{ActorTerminationReason, TerminationReason, TrapExplanation},
};
use gear_core_processor::{ProcessorContext, ProcessorExternalities};
use gear_lazy_pages::LazyPagesVersion;
use gear_lazy_pages_common::LazyPagesInitContext;
use gear_lazy_pages_native_interface::LazyPagesNative;
use gear_utils::NonEmpty;
use nonempty::nonempty;
use proptest::prelude::*;
use rand::{RngCore, SeedableRng, rngs::SmallRng};
use std::num::NonZero;

const UNSTRUCTURED_SIZE: usize = 1_000_000;
const WASM_PAGE_SIZE: u32 = 64 * 1024;
const INITIAL_PAGES: u32 = 1;

type Ext = gear_core_processor::Ext<LazyPagesNative>;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    #[test]
    // Test that valid config always generates a valid gear wasm.
    fn test_standard_config(buf in prop::collection::vec(any::<u8>(), UNSTRUCTURED_SIZE)) {
        let mut u = Unstructured::new(&buf);
        let configs_bundle: StandardGearWasmConfigsBundle = StandardGearWasmConfigsBundle {
            log_info: Some("Some data".into()),
            entry_points_set: EntryPointsSet::InitHandleHandleReply,
            ..Default::default()
        };

        let original_code = generate_gear_program_code(&mut u, configs_bundle)
            .expect("failed generating wasm");

        let _code = Code::try_new(original_code.clone(), 1, |_| CustomConstantCostRules::default(), None, None, None, None).unwrap();
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

    #[test]
    fn test_randomized_config(buf in prop::collection::vec(any::<u8>(), UNSTRUCTURED_SIZE)) {
        let mut u = Unstructured::new(&buf);

        let configs_bundle: RandomizedGearWasmConfigBundle = RandomizedGearWasmConfigBundle::new_arbitrary(
            &mut u,
            Default::default(),
            Default::default()
        );

        let original_code = generate_gear_program_code(&mut u, configs_bundle)
            .expect("failed generating wasm");

        let code_res = Code::try_new(original_code, 1, |_| CustomConstantCostRules::default(), None, None, None, None);
        assert!(code_res.is_ok());
    }

    #[test]
    fn test_randomized_config_reproducible(buf in prop::collection::vec(any::<u8>(), UNSTRUCTURED_SIZE)) {
        let mut u = Unstructured::new(&buf);
        let mut u2 = Unstructured::new(&buf);
        let configs_bundle1: RandomizedGearWasmConfigBundle = RandomizedGearWasmConfigBundle::new_arbitrary(
            &mut u,
            Default::default(),
            Default::default()
        );

        let configs_bundle2: RandomizedGearWasmConfigBundle = RandomizedGearWasmConfigBundle::new_arbitrary(
            &mut u2,
            Default::default(),
            Default::default()
        );

        let first = generate_gear_program_code(&mut u, configs_bundle1)
            .expect("failed generating wasm");
        let second = generate_gear_program_code(&mut u2, configs_bundle2)
            .expect("failed generating wasm");

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
    let module = Module::new(&wasm_bytes).expect("invalid wasm bytes");
    let module_with_critical_gas_limit = utils::inject_critical_gas_limit(module, 1_000_000);

    let wasm_bytes = module_with_critical_gas_limit
        .serialize()
        .expect("invalid pw module");

    let wat = wasmprinter::print_bytes(&wasm_bytes).expect("failed printing bytes");
    println!("wat = {wat}");

    wasmparser::validate(&wasm_bytes).unwrap();
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
    let module = Module::new(&wasm_bytes).expect("invalid wasm bytes");
    let no_recursions_module = utils::remove_recursion(module);

    let wasm_bytes = no_recursions_module.serialize().expect("invalid pw module");
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
    let module = Module::new(&wasm_bytes).expect("invalid wasm bytes");
    utils::find_recursion(&module, |path, call| {
        println!("path = {path:?}, call = {call}");
    });
    let no_recursions_module = utils::remove_recursion(module);
    utils::find_recursion(&no_recursions_module, |_, _| {
        unreachable!("there should be no recursions")
    });

    let wasm_bytes = no_recursions_module.serialize().expect("invalid pw module");
    assert!(wasmparser::validate(&wasm_bytes).is_ok());

    let wat = wasmprinter::print_bytes(&wasm_bytes).expect("failed printing bytes");
    println!("wat = {wat}");
}

#[test]
fn test_avoid_waits_works() {
    gear_utils::init_default_logger();

    let mut rng = SmallRng::seed_from_u64(123);
    let mut buf = vec![0; UNSTRUCTURED_SIZE];
    rng.fill_bytes(&mut buf);
    let mut unstructured = Unstructured::new(&buf);

    let mut injection_types = SyscallsInjectionTypes::all_never();
    injection_types.set(InvocableSyscall::Loose(SyscallName::Wait), 1, 1);
    let syscalls_config = SyscallsConfigBuilder::new(injection_types)
        .with_waiting_probability(NonZero::<u32>::new(4).unwrap())
        .build();

    let backend_report = execute_wasm_with_custom_configs(
        &mut unstructured,
        syscalls_config,
        ExecuteParams {
            // This test supposed to check if waiting probability works correctly,
            // so we have to set `init_called flag` to make wait probability code reachable.
            // And the second, we have to set waiting probability counter to non-zero value,
            // because the wasm check looks like this `if *wait_called_ptr % waiting_probability == 0 { orig_wait_syscall(); }`
            initial_memory_write: nonempty![set_init_called_flag(), set_wait_called_counter(1)]
                .into(),
            ..Default::default()
        },
    );

    assert_eq!(
        backend_report.termination_reason,
        TerminationReason::Actor(ActorTerminationReason::Success)
    );
}

#[test]
fn test_wait_stores_message_id() {
    gear_utils::init_default_logger();

    const EXPECTED_MSG_ID: u64 = 12345678;

    let mut rng = SmallRng::seed_from_u64(123);
    let mut buf = vec![0; UNSTRUCTURED_SIZE];
    rng.fill_bytes(&mut buf);
    let mut unstructured = Unstructured::new(&buf);

    let params_config = SyscallsParamsConfig::new().with_default_regular_config();

    let mut injection_types = SyscallsInjectionTypes::all_never();
    injection_types.set(InvocableSyscall::Loose(SyscallName::Wait), 1, 1);
    // Only for test, syscall `message_id` import added automatically when `wake` syscall is enabled in config.
    injection_types.enable_syscall_import(InvocableSyscall::Loose(SyscallName::MessageId));

    let syscalls_config_with_waiting_probability =
        SyscallsConfigBuilder::new(injection_types.clone())
            .with_params_config(params_config.clone())
            .with_waiting_probability(NonZero::<u32>::new(4).unwrap())
            .build();

    let syscalls_config_wo_waiting_probability = SyscallsConfigBuilder::new(injection_types)
        .with_params_config(params_config)
        .build();

    // Test both configs:
    for syscalls_config in [
        syscalls_config_with_waiting_probability,
        syscalls_config_wo_waiting_probability,
    ] {
        let BackendReport {
            termination_reason,
            store,
            memory,
            ..
        } = execute_wasm_with_custom_configs(
            &mut unstructured,
            syscalls_config,
            ExecuteParams {
                initial_memory_write: nonempty![set_init_called_flag(), set_wait_called_counter(0)]
                    .into(),
                message_id: EXPECTED_MSG_ID,
                ..Default::default()
            },
        );

        // It's Ok, message id is 32 bytes in size, but we use u64 for testing purposes.
        let mut message_id = [0u8; 8];
        let waited_message_id_ptr =
            MemoryLayout::from(WASM_PAGE_SIZE * INITIAL_PAGES).waited_message_id_ptr;
        memory
            .read(&store, waited_message_id_ptr as u32, &mut message_id)
            .unwrap();
        assert_eq!(u64::from_le_bytes(message_id), EXPECTED_MSG_ID);

        assert!(matches!(
            termination_reason,
            TerminationReason::Actor(ActorTerminationReason::Wait(..))
        ));
    }
}

#[test]
fn test_wake_uses_stored_message_id() {
    gear_utils::init_default_logger();

    const EXPECTED_MSG_ID: u64 = 12345678;

    let mut rng = SmallRng::seed_from_u64(123);
    let mut buf = vec![0; UNSTRUCTURED_SIZE];
    rng.fill_bytes(&mut buf);
    let mut unstructured = Unstructured::new(&buf);

    let params_config = SyscallsParamsConfig::new()
        .with_default_regular_config()
        .with_ptr_rule(PtrParamAllowedValues::WaitedMessageId);

    let mut injection_types = SyscallsInjectionTypes::all_never();
    injection_types.set(InvocableSyscall::Loose(SyscallName::Wake), 1, 1);
    let syscalls_config = SyscallsConfigBuilder::new(injection_types)
        .with_params_config(params_config)
        .build();

    let BackendReport {
        termination_reason,
        mut store,
        memory,
        ext,
    } = execute_wasm_with_custom_configs(
        &mut unstructured,
        syscalls_config,
        ExecuteParams {
            initial_memory_write: nonempty![set_waited_message_id(EXPECTED_MSG_ID)].into(),
            ..Default::default()
        },
    );

    let info = ext.into_ext_info(&mut store, &memory).unwrap();
    let msg_id = info.awakening.first().unwrap().0;
    let msg_id_bytes = msg_id.into_bytes();
    assert_eq!(
        u64::from_le_bytes(msg_id_bytes[..8].try_into().unwrap()),
        EXPECTED_MSG_ID
    );

    assert_eq!(
        termination_reason,
        TerminationReason::Actor(ActorTerminationReason::Success)
    );
}

#[test]
fn test_source_as_address_param() {
    gear_utils::init_default_logger();

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
        execute_wasm_with_custom_configs(&mut unstructured, syscalls_config, Default::default());

    assert_eq!(
        backend_report.termination_reason,
        TerminationReason::Actor(ActorTerminationReason::Exit(message_sender()))
    );
}

#[test]
fn test_existing_address_as_address_param() {
    gear_utils::init_default_logger();

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
        execute_wasm_with_custom_configs(&mut unstructured, syscalls_config, Default::default());

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
        ActorId::try_from(some_address.as_ref()).unwrap()
    );
}

// Syscalls of a `gr_*reply*` kind are the only of those, which has `Value` input param.
// Message value param for these syscalls is set during the common syscalls params
// processing flow.
#[test]
fn test_msg_value_ptr() {
    gear_utils::init_default_logger();

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
        ExecuteParams {
            value: INITIAL_BALANCE,
            ..Default::default()
        },
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
            .with_rule(RegularParamType::Offset, (0..=0).into())
            .with_rule(RegularParamType::Length, (0..=1).into())
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
                ExecuteParams {
                    value: INITIAL_BALANCE,
                    ..Default::default()
                },
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
                        assert_eq!(
                            destination,
                            ActorId::try_from(some_address.as_ref()).unwrap()
                        )
                    }
                    ActorKind::Random => {}
                }
            }
        }
    }
}

/// The `send` and `send_init` syscalls increase count of handles, but we only
/// need to take the last `send_init` handle since sending marks the handle as
/// already used.
#[test]
fn test_send_init_with_send() {
    gear_utils::init_default_logger();

    let mut rng = SmallRng::seed_from_u64(123);
    let mut buf = vec![0; UNSTRUCTURED_SIZE];
    rng.fill_bytes(&mut buf);
    let mut unstructured = Unstructured::new(&buf);

    let params_config = SyscallsParamsConfig::new()
        .with_default_regular_config()
        .with_ptr_rule(PtrParamAllowedValues::ActorIdWithValue {
            actor_kind: ActorKind::Source,
            range: 0..=0,
        });

    let mut injection_types = SyscallsInjectionTypes::all_never();
    injection_types.set(InvocableSyscall::Loose(SyscallName::SendInit), 1, 1);
    injection_types.set(InvocableSyscall::Loose(SyscallName::Send), 1, 1);
    injection_types.set(InvocableSyscall::Loose(SyscallName::SendCommit), 1, 1);
    let syscalls_config = SyscallsConfigBuilder::new(injection_types)
        .with_params_config(params_config)
        .with_error_processing_config(ErrorProcessingConfig::All)
        .with_keeping_insertion_order(true)
        .build();

    let backend_report =
        execute_wasm_with_custom_configs(&mut unstructured, syscalls_config, Default::default());

    assert_eq!(
        backend_report.termination_reason,
        TerminationReason::Actor(ActorTerminationReason::Success)
    );
}

#[test]
fn test_reservation_id_with_value_ptr() {
    gear_utils::init_default_logger();

    let mut rng = SmallRng::seed_from_u64(123);
    let mut buf = vec![0; UNSTRUCTURED_SIZE];
    rng.fill_bytes(&mut buf);
    let mut unstructured = Unstructured::new(&buf);

    let params_config = SyscallsParamsConfig::new()
        .with_default_regular_config()
        .with_ptr_rule(PtrParamAllowedValues::ReservationIdWithValue(0..=0));

    let mut injection_types = SyscallsInjectionTypes::all_never();
    injection_types.set(InvocableSyscall::Loose(SyscallName::ReserveGas), 1, 1);
    injection_types.set(InvocableSyscall::Loose(SyscallName::ReservationReply), 1, 1);
    let syscalls_config = SyscallsConfigBuilder::new(injection_types)
        .with_params_config(params_config)
        .with_error_processing_config(ErrorProcessingConfig::All)
        .with_keeping_insertion_order(true)
        .build();

    let backend_report = execute_wasm_with_custom_configs(
        &mut unstructured,
        syscalls_config,
        ExecuteParams {
            gas: 250_000_000_000,
            ..Default::default()
        },
    );

    assert_eq!(
        backend_report.termination_reason,
        TerminationReason::Actor(ActorTerminationReason::Success)
    );
}

#[test]
fn test_reservation_id_with_actor_id_and_value_ptr() {
    gear_utils::init_default_logger();

    let mut rng = SmallRng::seed_from_u64(321);
    let mut buf = vec![0; UNSTRUCTURED_SIZE];
    rng.fill_bytes(&mut buf);
    let mut unstructured = Unstructured::new(&buf);

    let some_address = [5; 32];
    let params_config = SyscallsParamsConfig::new()
        .with_default_regular_config()
        .with_ptr_rule(PtrParamAllowedValues::ReservationIdWithActorIdAndValue {
            actor_kind: ActorKind::ExistingAddresses(NonEmpty::new(some_address)),
            range: 0..=0,
        });

    let mut injection_types = SyscallsInjectionTypes::all_never();
    injection_types.set(InvocableSyscall::Loose(SyscallName::ReserveGas), 2, 2);
    injection_types.set(InvocableSyscall::Loose(SyscallName::ReservationSend), 1, 1);
    injection_types.set(InvocableSyscall::Loose(SyscallName::SendInit), 1, 1);
    injection_types.set(
        InvocableSyscall::Loose(SyscallName::ReservationSendCommit),
        1,
        1,
    );
    let syscalls_config = SyscallsConfigBuilder::new(injection_types)
        .with_params_config(params_config)
        .with_error_processing_config(ErrorProcessingConfig::All)
        .with_keeping_insertion_order(true)
        .build();

    let backend_report = execute_wasm_with_custom_configs(
        &mut unstructured,
        syscalls_config,
        ExecuteParams {
            gas: 250_000_000_000,
            ..Default::default()
        },
    );

    assert_eq!(
        backend_report.termination_reason,
        TerminationReason::Actor(ActorTerminationReason::Success)
    );
}

#[test]
fn test_reservation_id_ptr() {
    gear_utils::init_default_logger();

    let mut rng = SmallRng::seed_from_u64(123);
    let mut buf = vec![0; UNSTRUCTURED_SIZE];
    rng.fill_bytes(&mut buf);
    let mut unstructured = Unstructured::new(&buf);

    let params_config = SyscallsParamsConfig::new()
        .with_default_regular_config()
        .with_ptr_rule(PtrParamAllowedValues::ReservationId);

    let mut injection_types = SyscallsInjectionTypes::all_never();
    injection_types.set(InvocableSyscall::Loose(SyscallName::ReserveGas), 1, 1);
    injection_types.set(InvocableSyscall::Loose(SyscallName::UnreserveGas), 1, 1);
    let syscalls_config = SyscallsConfigBuilder::new(injection_types)
        .with_params_config(params_config)
        .with_error_processing_config(ErrorProcessingConfig::All)
        .with_keeping_insertion_order(true)
        .build();

    let backend_report = execute_wasm_with_custom_configs(
        &mut unstructured,
        syscalls_config,
        ExecuteParams {
            gas: 250_000_000_000,
            ..Default::default()
        },
    );

    assert_eq!(
        backend_report.termination_reason,
        TerminationReason::Actor(ActorTerminationReason::Success)
    );
}

#[test]
fn test_code_id_with_value_ptr() {
    gear_utils::init_default_logger();

    const INITIAL_BALANCE: u128 = 10_000;
    const REPLY_VALUE: u128 = 1_000;

    let some_code_id = CodeId::from([10; 32]);

    let tested_syscalls = [
        InvocableSyscall::Loose(SyscallName::CreateProgram),
        InvocableSyscall::Loose(SyscallName::CreateProgramWGas),
    ];

    let params_config = SyscallsParamsConfig::new()
        .with_default_regular_config()
        .with_rule(RegularParamType::Gas, (0..=0).into())
        .with_ptr_rule(PtrParamAllowedValues::CodeIdsWithValue {
            code_ids: NonEmpty::new(some_code_id),
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
            ExecuteParams {
                value: INITIAL_BALANCE,
                ..Default::default()
            },
        );

        assert_eq!(
            backend_report.ext.context.value_counter.left(),
            INITIAL_BALANCE - REPLY_VALUE
        );
        assert!(
            backend_report
                .ext
                .context
                .program_candidates_data
                .contains_key(&some_code_id)
        );
        assert_eq!(
            backend_report.termination_reason,
            TerminationReason::Actor(ActorTerminationReason::Success)
        );
    }
}

#[test]
fn error_processing_works_for_fallible_syscalls() {
    gear_utils::init_default_logger();

    // We create Unstructured from zeroes here as we just need any.
    let buf = vec![0; UNSTRUCTURED_SIZE];
    let mut unstructured = Unstructured::new(&buf);
    let mut unstructured2 = Unstructured::new(&buf);

    let fallible_syscalls = SyscallName::instrumentable().filter_map(|syscall| {
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
            ExecuteParams {
                initial_memory_write: initial_memory_write.clone(),
                outgoing_limit: 0,
                imitate_reply: true,
                ..Default::default()
            },
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
            ExecuteParams {
                initial_memory_write: initial_memory_write.clone(),
                outgoing_limit: 0,
                imitate_reply: true,
                ..Default::default()
            },
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

    let precise_syscalls = SyscallName::instrumentable().filter_map(|syscall| {
        InvocableSyscall::has_precise_variant(syscall).then_some(InvocableSyscall::Precise(syscall))
    });

    for syscall in precise_syscalls {
        // Prepare syscalls config & context settings for test case.
        const INJECTED_SYSCALLS: u32 = 1;

        let mut injection_types = SyscallsInjectionTypes::all_never();
        injection_types.set(syscall, INJECTED_SYSCALLS, INJECTED_SYSCALLS);

        let param_config = SyscallsParamsConfig::new()
            .with_default_regular_config()
            .with_rule(RegularParamType::Gas, (0..=0).into())
            .with_rule(RegularParamType::Offset, (0..=0).into())
            .with_rule(RegularParamType::Length, (0..=1).into());

        // Assert that syscalls results will be processed.
        let termination_reason = execute_wasm_with_custom_configs(
            &mut unstructured,
            SyscallsConfigBuilder::new(injection_types)
                .with_params_config(param_config)
                .with_precise_syscalls_config(PreciseSyscallsConfig::new(3..=3, 3..=3))
                .with_error_processing_config(ErrorProcessingConfig::All)
                .build(),
            Default::default(),
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
    _syscall: InvocableSyscall,
) -> (SyscallsParamsConfig, Option<NonEmpty<MemoryWrite>>) {
    (
        SyscallsParamsConfig::const_regular_params(i32::MAX as i64),
        None,
    )
}

fn set_init_called_flag() -> MemoryWrite {
    let mem_layout = MemoryLayout::from(WASM_PAGE_SIZE * INITIAL_PAGES);
    let offset = mem_layout.init_called_ptr as u32;
    let content = 0x01u32.to_le_bytes().to_vec();

    MemoryWrite { offset, content }
}

fn set_wait_called_counter(counter: u8) -> MemoryWrite {
    let mem_layout = MemoryLayout::from(WASM_PAGE_SIZE * INITIAL_PAGES);
    let offset = mem_layout.wait_called_ptr as u32;
    let content = vec![counter];

    MemoryWrite { offset, content }
}

fn set_waited_message_id(message_id: u64) -> MemoryWrite {
    let mem_layout = MemoryLayout::from(WASM_PAGE_SIZE * INITIAL_PAGES);
    let offset = mem_layout.waited_message_id_ptr as u32;
    let content = message_id.to_le_bytes().to_vec();

    MemoryWrite { offset, content }
}

struct ExecuteParams {
    initial_memory_write: Option<NonEmpty<MemoryWrite>>,
    outgoing_limit: u32,
    imitate_reply: bool,
    value: u128,
    gas: u64,
    message_id: u64,
}

impl Default for ExecuteParams {
    fn default() -> Self {
        Self {
            initial_memory_write: None,
            outgoing_limit: 1024,
            imitate_reply: false,
            value: 0,
            gas: 0,
            message_id: 0,
        }
    }
}

fn execute_wasm_with_custom_configs(
    unstructured: &mut Unstructured,
    syscalls_config: SyscallsConfig,
    ExecuteParams {
        initial_memory_write,
        outgoing_limit,
        imitate_reply,
        value,
        gas,
        message_id,
    }: ExecuteParams,
) -> BackendReport<Ext> {
    const PROGRAM_STORAGE_PREFIX: [u8; 32] = *b"execute_wasm_with_custom_configs";

    gear_lazy_pages::init(
        LazyPagesVersion::Version1,
        LazyPagesInitContext::new(PROGRAM_STORAGE_PREFIX),
        (),
    )
    .expect("Failed to init lazy-pages");

    let gear_config = (
        GearWasmGeneratorConfigBuilder::new()
            .with_memory_config(MemoryPagesConfig {
                initial_size: INITIAL_PAGES,
                ..MemoryPagesConfig::default()
            })
            .with_syscalls_config(syscalls_config)
            .with_entry_points_config(EntryPointsSet::Init)
            .build(),
        SelectableParams {
            allowed_instructions: vec![],
            max_instructions: 0,
            min_funcs: NonZero::<usize>::new(1).unwrap(),
            max_funcs: NonZero::<usize>::new(1).unwrap(),
        },
    );

    let code =
        generate_gear_program_code(unstructured, gear_config).expect("failed wasm generation");
    let code = Code::try_new(
        code,
        1,
        |_| CustomConstantCostRules::new(0, 0, 0),
        None,
        None,
        None,
        None,
    )
    .expect("Failed to create Code");

    let code_id = CodeId::generate(code.original_code());
    let program_id = ActorId::generate_from_user(code_id, b"");

    let incoming_message = IncomingMessage::new(
        message_id.into(),
        message_sender(),
        vec![1, 2, 3].try_into().unwrap(),
        Default::default(),
        Default::default(),
        Default::default(),
    );
    let mut message_context = MessageContext::new(
        IncomingDispatch::new(DispatchKind::Init, incoming_message, None),
        program_id,
        ContextSettings::with_outgoing_limits(outgoing_limit, u32::MAX),
    );

    if imitate_reply {
        let _ = message_context.reply_commit(ReplyPacket::auto(), None);
    }

    let processor_context = ProcessorContext {
        message_context,
        program_id,
        value_counter: ValueCounter::new(value),
        gas_counter: GasCounter::new(gas),
        gas_allowance_counter: GasAllowanceCounter::new(gas),
        ..ProcessorContext::new_mock()
    };

    let ext = Ext::new(processor_context);
    let env = Environment::new(
        ext,
        code.instrumented_code().bytes(),
        DispatchKind::Init,
        vec![DispatchKind::Init].into_iter().collect(),
        (INITIAL_PAGES as u16).into(),
    )
    .expect("Failed to create environment");

    env.execute(|ctx, mem, globals_config| {
        Ext::lazy_pages_init_for_program(
            ctx,
            mem,
            program_id,
            Default::default(),
            Some(
                mem.size(ctx)
                    .to_page_number()
                    .expect("Memory size is 4GB, so cannot be stack end"),
            ),
            globals_config,
            Default::default(),
        );

        if let Some(mem_writes) = initial_memory_write {
            for mem_write in mem_writes {
                mem.write(ctx, mem_write.offset, &mem_write.content)
                    .expect("Failed to write to memory");
            }
        };
    })
    .expect("Failed to execute WASM module")
}

fn message_sender() -> ActorId {
    let bytes = [1, 2, 3, 4].repeat(8);
    ActorId::try_from(bytes.as_ref()).unwrap()
}
