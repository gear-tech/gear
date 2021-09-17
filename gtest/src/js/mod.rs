// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

use log::debug;
use std::path::Path;
use std::process::Command;

#[derive(Clone)]
pub enum MetaType {
    InitInput,
    InitOutput,
    Input,
    Output,
}

impl ToString for MetaType {
    fn to_string(&self) -> String {
        match self {
            MetaType::InitInput => "init_input",
            MetaType::InitOutput => "init_output",
            MetaType::Input => "input",
            MetaType::Output => "output",
        }
        .into()
    }
}

pub fn gear_path() -> String {
    let pwd = Command::new("pwd")
        .output()
        .expect("Unable to call pwd command");

    let path = String::from_utf8(pwd.stdout).expect("Unable to parse pwd output bytes");
    let path_parts: Vec<String> = path.split("/").map(|v| v.replace("\n", "")).collect();

    if let Some(index) = path_parts.iter().rposition(|r| r == "gear") {
        path_parts[..index + 1].join("/")
    } else {
        panic!("Gear root directory not found")
    }
}

pub fn get_bytes(path: &str, meta_type: MetaType, json: String) -> Vec<u8> {
    let mut wasm_path = gear_path();
    wasm_path.push_str("/");
    wasm_path.push_str(path);

    if !wasm_path.ends_with(".meta.wasm") {
        wasm_path = wasm_path.replace(".wasm", ".meta.wasm");
    }

    if !Path::new(&wasm_path).exists() {
        panic!("Could not find file {}", wasm_path);
    }

    let mut script_path = gear_path();
    script_path.push_str("/gtest/src/js/encode.js");

    let output = Command::new("node")
        .arg(script_path)
        .args(&["-p", &wasm_path, "-t", &meta_type.to_string(), "-j", &json])
        .output()
        .expect("Unable to call node.js process");

    debug!(
        "js get_bytes stdout:{}",
        String::from_utf8(output.stdout.clone()).unwrap()
    );
    debug!(
        "js get_bytes stderr:{}",
        String::from_utf8(output.stderr).unwrap()
    );

    output.stdout
}

pub fn get_json(path: &str, meta_type: MetaType, hex: String) -> String {
    let mut wasm_path = gear_path();
    wasm_path.push_str("/");
    wasm_path.push_str(path);

    if !wasm_path.ends_with(".meta.wasm") {
        wasm_path = wasm_path.replace(".wasm", ".meta.wasm");
    }

    if !Path::new(&wasm_path).exists() {
        panic!("Could not find file {}", wasm_path);
    }

    let mut script_path = gear_path();
    script_path.push_str("/gtest/src/js/decode.js");

    let output = Command::new("node")
        .arg(script_path)
        .args(&["-p", &wasm_path, "-t", &meta_type.to_string(), "-b", &hex])
        .output()
        .expect("Unable to call node.js process");

    debug!(
        "js get_json stdout:{}",
        String::from_utf8(output.stdout.clone()).unwrap()
    );
    debug!(
        "js get_json stderr:{}",
        String::from_utf8(output.stderr).unwrap()
    );

    String::from_utf8(output.stdout).expect("Cannot parse u8 seq to string")
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Decode;
    use serde_json::Value;

    #[derive(Decode, Debug, PartialEq)]
    pub struct Id {
        pub decimal: u64,
        pub hex: Vec<u8>,
    }

    #[derive(Decode, Debug, PartialEq)]
    pub struct MessageIn {
        pub id: Id,
    }

    #[test]
    fn check() {
        let yaml = r#"
        id:
          decimal: 12345
          hex: [1, 2, 3, 4, 5]
        "#;
        let value = serde_yaml::from_str::<Value>(yaml).expect("Unable to create serde Value");
        let json = serde_json::to_string(&value).expect("Unable to create json from serde Value");

        let mut wasm_path = gear_path();
        wasm_path.push_str("/examples/target/wasm32-unknown-unknown/release/demo_meta.wasm");

        let wasm = if Path::new(&wasm_path).exists() {
            "examples/target/wasm32-unknown-unknown/release/demo_meta.wasm"
        } else {
            "target/wasm32-unknown-unknown/release/demo_meta.wasm"
        };

        let bytes = get_bytes(wasm, MetaType::Input, json.into());

        let msg = MessageIn::decode(&mut bytes.as_ref()).unwrap();

        assert_eq!(
            msg,
            MessageIn {
                id: Id {
                    decimal: 12345,
                    hex: vec![1, 2, 3, 4, 5]
                }
            }
        );
    }
}
