// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use crate::{gen_gear_program_code, GearConfig};
use arbitrary::Unstructured;
use rand::{rngs::SmallRng, RngCore, SeedableRng};

#[allow(unused)]
use indicatif::ProgressIterator;

#[test]
fn gen_wasm() {
    let mut rng = SmallRng::seed_from_u64(1234);
    for _ in 0..100 {
        let mut buf = vec![0; 1000000];
        rng.fill_bytes(&mut buf);
        let mut u = Unstructured::new(&buf);
        let code = gen_gear_program_code(&mut u, GearConfig::default());
        let _wat = wasmprinter::print_bytes(&code).unwrap();
    }
}

#[test]
fn gen_wasm_rare() {
    let mut rng = SmallRng::seed_from_u64(12345);
    for _ in 0..100 {
        let mut buf = vec![0; 1000000];
        rng.fill_bytes(&mut buf);
        let mut u = Unstructured::new(&buf);
        let code = gen_gear_program_code(&mut u, GearConfig::new_for_rare_cases());
        let _wat = wasmprinter::print_bytes(&code).unwrap();
    }
}

#[test]
fn gen_wasm_valid() {
    let mut rng = SmallRng::seed_from_u64(33333);
    for _ in 0..100 {
        let mut buf = vec![0; 1000000];
        rng.fill_bytes(&mut buf);
        let mut u = Unstructured::new(&buf);
        let code = gen_gear_program_code(&mut u, GearConfig::new_valid());
        let _wat = wasmprinter::print_bytes(&code).unwrap();
        wasmparser::validate(&code).unwrap();
    }
}
