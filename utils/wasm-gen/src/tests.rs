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

use crate::wasm::WASM_PAGE_SIZE;

use super::*;
use arbitrary::Unstructured;
use gear_core::code::Code;
use gear_utils::NonEmpty;
use gear_wasm_instrument::parity_wasm::{
    self,
    elements::{Instruction, Module},
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
    assert_eq!(*ptr, (stack_end_page * WASM_PAGE_SIZE) as i32);

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
    assert_eq!(*ptr, size + (stack_end_page * WASM_PAGE_SIZE) as i32);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    #[test]
    // Test that valid config always generates a valid gear wasm.
    fn test_valid_config(buf in prop::collection::vec(any::<u8>(), UNSTRUCTURED_SIZE)) {
        use gear_wasm_instrument::rules::CustomConstantCostRules;
        let mut u = Unstructured::new(&buf);
        let configs_bundle: ValidGearWasmConfigsBundle = ValidGearWasmConfigsBundle {
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

        let gear_config = ValidGearWasmConfigsBundle::<[u8; 32]>::default();

        let first = generate_gear_program_code(&mut u, gear_config.clone()).expect("failed wasm generation");
        let second = generate_gear_program_code(&mut u2, gear_config).expect("failed wasm generation");

        assert_eq!(first, second);
    }
}

#[test]
fn test_valid() {
    use gear_wasm_instrument::wasm_instrument::gas_metering::ConstantCostRules;

    init_default_logger();

    for seed in 0..100 {
        let mut rng = SmallRng::seed_from_u64(seed as u64);

        let mut buf = vec![0; UNSTRUCTURED_SIZE];
        rng.fill_bytes(&mut buf);
        let mut u = Unstructured::new(&buf);
        let configs_bundle: ValidGearWasmConfigsBundle = ValidGearWasmConfigsBundle {
            log_info: Some("Some data".into()),
            entry_points_set: EntryPointsSet::InitHandleHandleReply,
            memory_config: MemoryPagesConfig {
                initial_size: 100,
                upper_limit: None,
                stack_end_page: None
            },
            ..Default::default()
        };

        let raw_code = generate_gear_program_code(&mut u, configs_bundle)
            .expect("failed generating wasm");

        let code_res = Code::try_new(raw_code, 1, |_| ConstantCostRules::default(), None);
        assert!(code_res.is_ok());
    }
}
