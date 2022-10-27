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

//! Module describes structural "bricks" of tests in [spec](https://github.com/gear-tech/gear/tree/master/gear-test/spec) directory .
//!
//! For now tests are defined in "yaml" format and parsed into models defined in the module by using [serde_yaml](https://docs.serde.rs/serde_yaml/index.html).

use crate::address::{self, Address as ChainAddress};
use hex::FromHex;
use serde::{de, Deserialize, Deserializer, Serialize};
use serde_yaml::Value;

fn de_address<'de, D: Deserializer<'de>>(deserializer: D) -> Result<usize, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(s) => {
            let without_prefix = s.trim_start_matches("0x");
            usize::from_str_radix(without_prefix, 16).map_err(de::Error::custom)?
        }
        _ => return Err(de::Error::custom("wrong type")),
    })
}

fn de_bytes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(s) => {
            let without_prefix = s.trim_start_matches("0x");
            Vec::from_hex(without_prefix).map_err(de::Error::custom)?
        }
        _ => return Err(de::Error::custom("wrong type")),
    })
}

/// Program being tested and it's initialization data.
///
/// In test nested structure *program* is one the highest fields. The other one is *fixture*.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Program {
    /// Path to program's wasm blob.
    pub path: String,
    /// Program's id.
    #[serde(deserialize_with = "address::deserialize")]
    pub id: ChainAddress,
    /// Optional message source.
    ///
    /// If is `None`, then fixed user address is used.
    pub source: Option<ChainAddress>,
    /// Optional payload sent to program's `init` function.
    pub init_message: Option<PayloadVariant>,
    /// Optional gas limit used when initializing program
    ///
    /// If is `None` then `u64::MAX` is provided.
    pub init_gas_limit: Option<u64>,
    /// Optional message value provided to program's `init` function.
    pub init_value: Option<u128>,
}

/// Code saved in the persistent layer before fixtures are run
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Code {
    /// Path to program's wasm blob.
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ChainProgram {
    #[serde(flatten)]
    pub address: ChainAddress,
    pub terminated: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Programs {
    pub only: Option<bool>,
    pub ids: Vec<ChainProgram>,
}

/// Expected data after running messages, defined in the fixture.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Expectation {
    /// Step number.
    ///
    /// By defining the field we control how many messaged does the test runner actually process.
    /// So, we can perform test checks after different processing steps and look into interim state.
    pub step: Option<usize>,
    /// Expected messages in the message queue.
    pub messages: Option<Vec<Message>>,
    /// Expected allocations after program run.
    pub allocations: Option<Vec<Allocations>>,
    /// Expected data to be in the memory.
    pub memory: Option<Vec<BytesAt>>,
    /// Expected messages in the log.
    pub log: Option<Vec<Message>>,
    /// Flag, which points that errors are allowed. Could be used to check traps.
    #[serde(rename = "allowError")]
    pub allow_error: Option<bool>,
    /// Expected active programs (not failed in the init) ids
    pub programs: Option<Programs>,
}

/// Data describing program being tested.
///
/// In test nested structure *fixture* is one the highest fields. The other one is *program*.
/// `Fixture` is a logical union of:
/// 1) the set of messages sent to programs defined in the test;
/// 2) expected results of message processing.
/// Tests can have multiple `Fixtures`, which means we can define specialized "messages & expectation" block sets, each with its own `title`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Fixture {
    /// Fixture title
    pub title: String,
    /// Messages being sent to programs, defined in the test
    pub messages: Option<Vec<Message>>,
    /// Expected results of the test run.
    pub expected: Option<Vec<Expectation>>,
}

/// Payload data types being used in messages.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value")]
pub enum PayloadVariant {
    #[serde(rename = "custom")]
    Custom(Value),
    #[serde(rename = "utf-8")]
    Utf8(String),
    #[serde(rename = "i32")]
    Int32(i32),
    #[serde(rename = "i64")]
    Int64(i64),
    #[serde(rename = "f32")]
    Float32(f32),
    #[serde(rename = "f64")]
    Float64(f64),
    #[serde(rename = "bytes", deserialize_with = "de_bytes")]
    Bytes(Vec<u8>),
}

impl Default for PayloadVariant {
    fn default() -> Self {
        Self::Bytes(Vec::new())
    }
}

impl PayloadVariant {
    pub fn into_raw(self) -> Vec<u8> {
        match self {
            Self::Custom(v) => serde_json::to_string(&v).unwrap().as_bytes().to_vec(),
            Self::Utf8(v) => v.into_bytes(),
            Self::Int32(v) => v.to_le_bytes().to_vec(),
            Self::Int64(v) => v.to_le_bytes().to_vec(),
            Self::Float32(v) => v.to_le_bytes().to_vec(),
            Self::Float64(v) => v.to_le_bytes().to_vec(),
            Self::Bytes(v) => v,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.clone().into_raw()
    }

    pub fn equals(&self, val: &[u8]) -> bool {
        let bytes = self.to_bytes();
        &bytes[..] == val
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BytesAt {
    /// Program's id.
    #[serde(deserialize_with = "address::deserialize")]
    pub id: ChainAddress,
    #[serde(rename = "at")]
    #[serde(deserialize_with = "de_address")]
    pub address: usize,
    #[serde(deserialize_with = "de_bytes")]
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Allocations {
    /// Program's id.
    #[serde(deserialize_with = "address::deserialize")]
    pub id: ChainAddress,
    pub filter: Option<AllocationFilter>,
    #[serde(flatten)]
    pub kind: AllocationExpectationKind,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationExpectationKind {
    PageCount(u64),
    ExactPages(Vec<u32>),
    ContainsPages(Vec<u32>),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationFilter {
    Static,
    Dynamic,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Message {
    pub source: Option<ChainAddress>,
    #[serde(deserialize_with = "address::deserialize")]
    pub destination: ChainAddress,
    pub init: Option<bool>,
    pub payload: Option<PayloadVariant>,
    pub gas_limit: Option<u64>,
    pub value: Option<u128>,
    #[serde(rename = "statusCode")]
    pub status_code: Option<i32>,
}

/// Main model describing test structure
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Test {
    /// Short name of the test describing its logic
    pub title: String,
    /// Code that are needed to be submitted for tests
    pub codes: Option<Vec<Code>>,
    /// Programs and related data used for tests
    pub programs: Vec<Program>,
    /// A set of messages and expected results of running them in the context of defined [programs](todo-field-ref).
    pub fixtures: Vec<Fixture>,
}

/// get path to meta wasm file.
/// `wasm_path` is path to wasm file.
pub fn get_meta_wasm_path(wasm_path: String) -> String {
    wasm_path
        .replace(".opt.wasm", ".wasm")
        .replace(".wasm", ".meta.wasm")
}

#[test]
fn check_sample() {
    let yaml = r#"
    title: basic

    programs:
    - id: 1
      path: examples/target/wasm32-unknown-unknown/release/demo_ping.wasm

    fixtures:
    - title: ping-pong
      messages:
      - destination: 1
        payload:
          kind: utf-8
          value: PING
      expected:
      - step: 1
        log:
        - destination: 0
          payload:
            kind: utf-8
            value: PONG
    "#;

    let test: Test = serde_yaml::from_str(yaml).unwrap();
    let path = test.programs.get(0).expect("Must have one").path.clone();

    assert_eq!(test.title, "basic");
    assert_eq!(test.fixtures[0].messages.as_ref().unwrap().len(), 1);
    assert_eq!(test.fixtures[0].messages.as_ref().unwrap().len(), 1);
    assert_eq!(
        path,
        "examples/target/wasm32-unknown-unknown/release/demo_ping.wasm"
    );

    let meta_wasm = get_meta_wasm_path(path);
    assert_eq!(
        meta_wasm,
        "examples/target/wasm32-unknown-unknown/release/demo_ping.meta.wasm"
    );
}
