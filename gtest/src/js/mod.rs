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

use std::process::Command;

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
    let path_parts: Vec<&str> = path.split("/").collect();

    if let Some(index) = path_parts.iter().position(|&r| r == "gear") {
        path_parts[..index + 1].join("/")
    } else {
        panic!("Gear root directory not found")
    }
}

pub fn get_bytes(path: &str, meta_type: MetaType, json: String) -> Vec<u8> {
    let mut wasm_path = gear_path();
    wasm_path.push_str("/");
    wasm_path.push_str(path);

    let output = Command::new("node")
        .arg("./src/js/index.js")
        .args(&["-p", &wasm_path, "-t", &meta_type.to_string(), "-j", &json])
        .output()
        .expect("Unable to call node.js process");

    output.stdout
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Decode;
    use serde_json::Value;

    #[derive(Decode, Debug, PartialEq)]
    pub struct MessageInitOut {
        pub exchange_rate: Result<u8, u8>,
        pub sum: u8,
    }

    #[test]
    fn check() {
        let yaml = r#"
        exchange_rate:
            Ok: 4
        sum: 15
        "#;
        let value = serde_yaml::from_str::<Value>(yaml).expect("Unable to create serde Value");
        
        let json = serde_json::to_string(&value).expect("Unable to create json from serde Value");
        let wasm = "examples/target/wasm32-unknown-unknown/release/demo_meta.meta.wasm";

        let bytes = get_bytes(wasm, MetaType::InitOutput, json);

        let msg = MessageInitOut::decode(&mut bytes.as_ref()).unwrap();

        assert_eq!(
            msg,
            MessageInitOut {
                exchange_rate: Ok(4),
                sum: 15
            }
        );
    }
}
