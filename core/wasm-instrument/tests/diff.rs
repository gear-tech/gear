// This file is part of Gear.
//
// Copyright (C) 2017-2024 Parity Technologies.
// Copyright (C) 2025 Gear Technologies Inc.
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

use gear_wasm_instrument::{Module, gas_metering, stack_limiter};
use std::{
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use wasmparser::validate;

fn slurp<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
    let mut f = fs::File::open(path)?;
    let mut buf = vec![];
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

fn dump<P: AsRef<Path>>(path: P, buf: &[u8]) -> io::Result<()> {
    let mut f = fs::File::create(path)?;
    f.write_all(buf)?;
    Ok(())
}

fn run_diff_test<F: FnOnce(&[u8]) -> Vec<u8>>(test_dir: &str, name: &str, test: F) {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    let mut fixture_path = manifest_dir.clone();
    fixture_path.push("tests");
    fixture_path.push("fixtures");
    fixture_path.push(test_dir);
    fixture_path.push(name);

    let mut expected_path = manifest_dir;
    expected_path.push("tests");
    expected_path.push("expectations");
    expected_path.push(test_dir);
    expected_path.push(name);

    let fixture_wasm = wat::parse_file(&fixture_path).expect("Failed to read fixture");
    validate(&fixture_wasm).expect("Fixture is invalid");

    let expected_wat = slurp(&expected_path).unwrap_or_default();
    let expected_wat = std::str::from_utf8(&expected_wat).expect("Failed to decode expected wat");

    let actual_wasm = test(fixture_wasm.as_ref());
    println!("{}", wasmprinter::print_bytes(&actual_wasm).unwrap());
    validate(&actual_wasm).expect("Result module is invalid");

    let actual_wat =
        wasmprinter::print_bytes(&actual_wasm).expect("Failed to convert result wasm to wat");

    if actual_wat != expected_wat {
        println!("difference!");
        println!("--- {}", expected_path.display());
        println!("+++ {test_dir} test {name}");
        for diff in diff::lines(expected_wat, &actual_wat) {
            match diff {
                diff::Result::Left(l) => println!("-{l}"),
                diff::Result::Both(l, _) => println!(" {l}"),
                diff::Result::Right(r) => println!("+{r}"),
            }
        }

        if std::env::var_os("BLESS").is_some() {
            dump(&expected_path, actual_wat.as_bytes()).expect("Failed to write to expected");
        } else {
            panic!();
        }
    }
}

mod stack_height {
    use super::*;

    macro_rules! def_stack_height_test {
        ( $name:ident ) => {
            #[test]
            fn $name() {
                run_diff_test(
                    "stack-height",
                    concat!(stringify!($name), ".wat"),
                    |input| {
                        let module = Module::new(input).expect("Failed to deserialize");
                        let instrumented = stack_limiter::inject(module, 1024)
                            .expect("Failed to instrument with stack counter");
                        instrumented.serialize().expect("Failed to serialize")
                    },
                );
            }
        };
    }

    def_stack_height_test!(simple);
    def_stack_height_test!(start);
    def_stack_height_test!(table);
    def_stack_height_test!(global);
    def_stack_height_test!(imports);
    def_stack_height_test!(many_locals);
    def_stack_height_test!(empty_functions);
}

mod gas {
    use super::*;

    macro_rules! def_gas_test {
        ( $name:ident ) => {
            #[test]
            fn $name() {
                run_diff_test("gas", concat!(stringify!($name), ".wat"), |input| {
                    let rules = gas_metering::ConstantCostRules::default();

                    let module = Module::new(input).expect("Failed to deserialize");

                    let instrumented = gas_metering::inject(module, &rules, "env")
                        .expect("Failed to instrument with gas metering");
                    instrumented.serialize().expect("Failed to serialize")
                });
            }
        };
    }

    def_gas_test!(ifs);
    def_gas_test!(simple);
    def_gas_test!(start);
    def_gas_test!(call);
    def_gas_test!(branch);
}
