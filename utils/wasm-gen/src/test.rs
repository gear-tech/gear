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

use crate::{gen_gear_program_code, utils, GearConfig};
use arbitrary::Unstructured;
use gear_wasm_instrument::parity_wasm::{self, elements};
use rand::{rngs::SmallRng, RngCore, SeedableRng};

#[allow(unused)]
use indicatif::ProgressIterator;

const MODULES_AMOUNT: usize = 100;
const UNSTRUCTURED_SIZE: usize = 1000000;

#[test]
fn gen_wasm_normal() {
    let mut rng = SmallRng::seed_from_u64(1234);
    for _ in 0..MODULES_AMOUNT {
        let mut buf = vec![0; UNSTRUCTURED_SIZE];
        rng.fill_bytes(&mut buf);
        let mut u = Unstructured::new(&buf);
        let code = gen_gear_program_code(&mut u, GearConfig::new_normal());
        let _wat = wasmprinter::print_bytes(code).unwrap();
    }
}

#[test]
fn gen_wasm_rare() {
    let mut rng = SmallRng::seed_from_u64(12345);
    for _ in 0..MODULES_AMOUNT {
        let mut buf = vec![0; UNSTRUCTURED_SIZE];
        rng.fill_bytes(&mut buf);
        let mut u = Unstructured::new(&buf);
        let code = gen_gear_program_code(&mut u, GearConfig::new_for_rare_cases());
        let _wat = wasmprinter::print_bytes(code).unwrap();
    }
}

#[test]
fn gen_wasm_valid() {
    let mut rng = SmallRng::seed_from_u64(33333);
    let mut config = GearConfig::new_valid();
    config.print_test_info = Some("HEY GEAR".to_owned());
    for _ in 0..MODULES_AMOUNT {
        let mut buf = vec![0; UNSTRUCTURED_SIZE];
        rng.fill_bytes(&mut buf);
        let mut u = Unstructured::new(&buf);
        let code = gen_gear_program_code(&mut u, config.clone());
        let _wat = wasmprinter::print_bytes(&code).unwrap();
        wasmparser::validate(&code).unwrap();
    }
}

#[test]
fn remove_trivial_recursions() {
    let wat = r#"
    (module
        (func (;0;)
            call 0
        )
    )"#;

    let wasm = wat::parse_str(wat).unwrap();
    let module: elements::Module = parity_wasm::deserialize_buffer(&wasm).unwrap();
    let module = utils::remove_recursion(module);
    let wasm = parity_wasm::serialize(module).unwrap();
    wasmparser::validate(&wasm).unwrap();

    let wat = wasmprinter::print_bytes(&wasm).unwrap();
    println!("wat = {wat}");

    let wat = r#"
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

    let wasm = wat::parse_str(wat).unwrap();
    let module: elements::Module = parity_wasm::deserialize_buffer(&wasm).unwrap();
    utils::find_recursion(&module, |path, call| {
        println!("path = {path:?}, call = {call}");
    });
    let module = utils::remove_recursion(module);
    utils::find_recursion(&module, |_path, _call| {
        unreachable!("there should be no recursions")
    });

    let wasm = parity_wasm::serialize(module).unwrap();
    wasmparser::validate(&wasm).unwrap();

    let wat = wasmprinter::print_bytes(&wasm).unwrap();
    println!("wat = {wat}");
}
