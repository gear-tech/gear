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

//! JS module.
//!
//! Module contains all the needed types and functionality for an easy inference of structures of messages sent to or
//! received from *init* and *handle* functions.
//!
//! Information about message structure makes it easier for an [idea](https://idea.gear-tech.io/) user to send
//! messages to smart contracts. This modules provides an internal functionality for deducting what fields
//! must be set for a message to be successfully sent to or received from *init* or *handle* functions.

use std::{path::PathBuf, process::Command, str::FromStr};

/// Helper type to perform correct encode/decode of custom types sent to/received from program functions.
///
/// Developer can define his own types, that he expects to be received from function caller, in the smart contract code.
/// However, what actually is sent to the smart contract is a byte buffer. This type tells additional information to encoder/decoder
/// about what is the structure of the sent/received message. All in all, we need this meta type information to infer JSON definitions
/// of the message types.
pub enum MetaType {
    InitInput,
    InitOutput,
    #[allow(unused)]
    AsyncInitInput,
    #[allow(unused)]
    AsyncInitOutput,
    HandleInput,
    HandleOutput,
    #[allow(unused)]
    AsyncHandleInput,
    #[allow(unused)]
    AsyncHandleOutput,
}

impl ToString for MetaType {
    fn to_string(&self) -> String {
        match self {
            MetaType::InitInput => "init_input",
            MetaType::InitOutput => "init_output",
            MetaType::AsyncInitInput => "async_init_input",
            MetaType::AsyncInitOutput => "async_init_output",
            MetaType::HandleInput => "handle_input",
            MetaType::HandleOutput => "handle_output",
            MetaType::AsyncHandleInput => "async_handle_input",
            MetaType::AsyncHandleOutput => "async_handle_output",
        }
        .into()
    }
}

/// Actually runs a node.js script, which encodes JSON data which is aimed to be sent to function as input or
/// decodes output bytes into JSON.
pub fn call_node(script_path: PathBuf, args: Vec<&str>) -> Vec<u8> {
    let script_path = script_path
        .to_str()
        .expect("Unable to convert PathBuf to str");
    let output = Command::new("node")
        .arg(script_path)
        .args(args)
        .output()
        .expect("Unable to call node.js process");

    log::debug!(
        "js stdout:{}",
        String::from_utf8(output.stdout.clone()).unwrap()
    );
    log::debug!("js stderr:{}", String::from_utf8(output.stderr).unwrap());

    output.stdout
}

/// Helper type which is used as a store for payload in two different formats, which are
/// JSON format and in SCALE codec format.
///
/// [CodecBytes](enum.MetaData.html#variant.CodecBytes) is a variant which stores encoded message payload.
/// [Json](enum.MetaData.html#variant.Json) stores decoded message payload as a JSON string.
#[derive(Clone)]
pub enum MetaData {
    CodecBytes(Vec<u8>),
    Json(String),
}

impl MetaData {
    /// Converts `Self` to bytes vector.
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::CodecBytes(b) => b,
            Self::Json(j) => j.into_bytes(),
        }
    }

    /// Converts `Self` to string.
    pub fn into_json(self) -> String {
        match self {
            Self::CodecBytes(b) => String::from_utf8(b).expect("Unable to convert to string"),
            Self::Json(j) => j,
        }
    }

    /// Encodes or decodes metadata.
    ///
    /// If `Self` stores decoded data, then the function will encode that data to SCALE codec bytes, returning [CodecBytes](enum.MetaData.html#variant.CodecBytes).
    /// If `Self` stores encoded data, then the function will decode that data to JSON string, returning [Json](enum.MetaData.html#variant.Json).
    pub fn convert(self, meta_wasm: &str, meta_type: &MetaType) -> Result<Self, String> {
        let gear_path = {
            let manifest_dir = env!("CARGO_MANIFEST_DIR");
            let mut path = PathBuf::from_str(manifest_dir).map_err(|e| {
                log::debug!("PathBuf::from_str failed: {e:?}");
                "Failed to construct PathBuf from 'CARGO_MANIFEST_DIR'"
            })?;
            if !path.pop() {
                return Err("Gear root directory not found".into());
            }

            path
        };

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
                script_path.push(PathBuf::from("gear-test/src/js/decode.js"));
                let bytes = call_node(
                    script_path,
                    vec![
                        "-p",
                        path,
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
                script_path.push(PathBuf::from("gear-test/src/js/encode.js"));
                let bytes = hex::decode(call_node(
                    script_path,
                    vec!["-p", path, "-t", &meta_type.to_string(), "-j", &json],
                ))
                .expect("Unable to decode hex from js");

                Ok(Self::CodecBytes(bytes))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::{Decode, Encode};
    use serde_json::Value;

    #[derive(Decode, Debug, PartialEq, Eq, Encode)]
    pub enum Action {
        AddMessage(MessageIn),
        ViewMessages,
    }

    #[derive(Decode, Debug, PartialEq, Eq, Encode)]
    pub struct MessageIn {
        author: String,
        msg: String,
    }

    #[test]
    fn check_enum() {
        let _ = env_logger::try_init();

        let yaml = r#"
        addMessage:
          author: Author
          msg: Some message, really huge text
        "#;
        let value = serde_yaml::from_str::<Value>(yaml).expect("Unable to create serde Value");
        let json = serde_json::to_string(&value).expect("Unable to create json from serde Value");

        println!("{json}");

        let json = MetaData::Json(json);

        let bytes = json
            .clone()
            .convert(
                "target/wasm32-unknown-unknown/release/guestbook.meta.wasm",
                &MetaType::HandleInput,
            )
            .or_else(|_| {
                json.convert(
                    "examples/target/wasm32-unknown-unknown/release/guestbook.meta.wasm",
                    &MetaType::HandleInput,
                )
            });

        let expectation = Action::AddMessage(MessageIn {
            author: "Author".into(),
            msg: "Some message, really huge text".into(),
        });

        let codec_bytes = bytes.clone().expect("Could not find file").into_bytes();

        assert_eq!(hex::encode(codec_bytes), hex::encode(expectation.encode()));

        let msg = Action::decode(&mut bytes.expect("Could not find file").into_bytes().as_ref())
            .expect("Unable to decode CodecBytes");

        assert_eq!(msg, expectation);
    }

    #[test]
    fn check_vec() {
        let yaml = r#"
        - author: Dmitry
          msg: Hello, world!
        - author: Eugene
          msg: Hello, Dmitry!
        "#;
        let value = serde_yaml::from_str::<Value>(yaml).expect("Unable to create serde Value");
        let json = serde_json::to_string(&value).expect("Unable to create json from serde Value");

        println!("{json}");

        let json = MetaData::Json(json);

        let bytes = json
            .clone()
            .convert(
                "target/wasm32-unknown-unknown/release/guestbook.meta.wasm",
                &MetaType::HandleOutput,
            )
            .or_else(|_| {
                json.convert(
                    "target/examples/wasm32-unknown-unknown/release/guestbook.meta.wasm",
                    &MetaType::HandleOutput,
                )
            });

        let expectation = vec![
            MessageIn {
                author: "Dmitry".into(),
                msg: "Hello, world!".into(),
            },
            MessageIn {
                author: "Eugene".into(),
                msg: "Hello, Dmitry!".into(),
            },
        ];

        let codec_bytes = bytes.clone().expect("Could not find file").into_bytes();

        assert_eq!(hex::encode(codec_bytes), hex::encode(expectation.encode()));

        let msg = Vec::<MessageIn>::decode(
            &mut bytes.expect("Could not find file").into_bytes().as_ref(),
        )
        .expect("Unable to decode CodecBytes");

        assert_eq!(msg, expectation);
    }
}
