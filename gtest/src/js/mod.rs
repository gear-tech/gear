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

use std::path::PathBuf;
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

pub fn call_node(script_path: PathBuf, args: Vec<&str>) -> Vec<u8> {
    let script_path = script_path
        .to_str()
        .expect("Unable to convert PathBuf to str");
    let output = Command::new("node")
        .arg(script_path)
        .args(&args)
        .output()
        .expect("Unable to call node.js process");

    log::debug!(
        "js stdout:{}",
        String::from_utf8(output.stdout.clone()).unwrap()
    );
    log::debug!("js stderr:{}", String::from_utf8(output.stderr).unwrap());

    output.stdout
}

#[derive(Clone)]
pub enum MetaData {
    CodecBytes(Vec<u8>),
    Json(String),
}

impl MetaData {
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::CodecBytes(b) => b,
            Self::Json(j) => j.into_bytes(),
        }
    }

    pub fn into_json(self) -> String {
        match self {
            Self::CodecBytes(b) => String::from_utf8(b).expect("Unable to convert to string"),
            Self::Json(j) => j,
        }
    }

    pub fn convert(self, meta_wasm: &str, meta_type: &MetaType) -> Result<Self, String> {
        let mut gear_path = std::env::current_dir().expect("Unable to get current dir");
        while !gear_path.ends_with("gear") {
            if !gear_path.pop() {
                return Err("Gear root directory not found".into());
            }
        }

        let mut path = gear_path.clone();
        path.push(PathBuf::from(meta_wasm));

        if !path.exists() {
            return Err(format!(
                "Path {} do not exist",
                path.to_str().expect("Unable to convert PathBuf to str")
            ));
        }

        let path = path.to_str().expect("Unable to convert PathBuf to str");

        if !path.ends_with(".meta.wasm") {
            return Err("Path to wasm should lead to .meta.wasm extension file".into());
        }

        let mut script_path = gear_path;

        match self {
            Self::CodecBytes(bytes) => {
                script_path.push(PathBuf::from("gtest/src/js/decode.js"));
                let bytes = call_node(
                    script_path,
                    vec![
                        "-p",
                        &path,
                        "-t",
                        &meta_type.to_string(),
                        "-b",
                        &hex::encode(bytes),
                    ],
                );

                if let Ok(json) = String::from_utf8(bytes) {
                    Ok(Self::Json(json))
                } else {
                    Err("Unable to convert codec bytes to JSON string".into())
                }
            }

            Self::Json(json) => {
                script_path.push(PathBuf::from("gtest/src/js/encode.js"));
                let bytes = call_node(
                    script_path,
                    vec!["-p", &path, "-t", &meta_type.to_string(), "-j", &json],
                );

                Ok(Self::CodecBytes(bytes))
            }
        }
    }
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

        let json = MetaData::Json(json);

        let bytes = json
            .clone()
            .convert(
                "examples/target/wasm32-unknown-unknown/release/demo_meta.meta.wasm",
                &MetaType::Input,
            )
            .or(json.convert(
                "target/wasm32-unknown-unknown/release/demo_meta.meta.wasm",
                &MetaType::Input,
            ));

        let msg =
            MessageIn::decode(&mut bytes.expect("Could not find file ").into_bytes().as_ref())
                .expect("Unable to decode CodecBytes");
        let expectation = MessageIn {
            id: Id {
                decimal: 12345,
                hex: vec![1, 2, 3, 4, 5],
            },
        };

        assert_eq!(msg, expectation);
    }
}
