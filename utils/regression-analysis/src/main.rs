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

use std::collections::BTreeMap;
use std::path::{PathBuf, Path};

use clap::Parser;

use quick_xml::de::from_str;

mod junit_parser;
mod output;

use common::TestSuites;

const PALLET_NAMES: [&str; 7] = [
    "pallet-gas",
    "pallet-gear",
    "pallet-gear-debug",
    "pallet-gear-messenger",
    "pallet-gear-program",
    "pallet-gear-payment",
    "pallet-usage",
];

#[derive(Parser)]
struct Cli {
    #[clap(long)]
    master_junit_xml: PathBuf,
    #[clap(long)]
    current_junit_xml: PathBuf,
    #[clap(long)]
    disable_filter: bool,
}

fn build_tree(disable_filter: bool, path: &Path) -> BTreeMap<String, BTreeMap<String, f64>> {
    let filter = |pallet_name: &str| {
        if disable_filter {
            return true;
        }

        PALLET_NAMES.iter().any(|&name| name == pallet_name)
    };

    let junit_xml = std::fs::read_to_string(path).unwrap();
    let test_suites: TestSuites = from_str(&junit_xml).unwrap();
    junit_parser::build_tree(filter, test_suites)
}

fn main() {
    let cli = Cli::parse();

    let executions_master = build_tree(cli.disable_filter, &cli.master_junit_xml);
    let executions_current = build_tree(cli.disable_filter, &cli.current_junit_xml);

    let compared = executions_current
        .iter()
        .filter_map(|(key, tests_current)| {
            executions_master.get(key).map(|tests_master| {
                let tests = tests_current
                    .iter()
                    .filter_map(|(key, &master_time)| {
                        tests_master.get(key).map(|&current_time| output::Test {
                            name: key.clone(),
                            master_time,
                            current_time,
                        })
                    })
                    .collect::<Vec<_>>();

                (key, tests)
            })
        })
        .collect::<BTreeMap<_, _>>();

    for (name, stats) in compared {
        println!("name = {}", name);
        let table = tabled::Table::new(stats);
        println!("{}", table);
        println!("");
    }
}
