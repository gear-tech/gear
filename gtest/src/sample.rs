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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Program {
    pub path: String,
    #[serde(deserialize_with = "address::deserialize")]
    pub id: ChainAddress,
    pub source: Option<ChainAddress>,
    pub init_message: Option<PayloadVariant>,
    pub init_gas_limit: Option<u64>,
    pub init_value: Option<u128>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Expectation {
    pub step: Option<u64>,
    pub messages: Option<Vec<Message>>,
    pub allocations: Option<Vec<Allocations>>,
    pub memory: Option<Vec<BytesAt>>,
    pub log: Option<Vec<Message>>,
    #[serde(rename = "allowError")]
    pub allow_error: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Fixture {
    pub title: String,
    pub messages: Vec<Message>,
    pub expected: Vec<Expectation>,
}

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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BytesAt {
    pub program_id: u64, // required for static memory
    #[serde(rename = "at")]
    #[serde(deserialize_with = "de_address")]
    pub address: usize,
    #[serde(deserialize_with = "de_bytes")]
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Allocations {
    pub program_id: u64,
    pub filter: Option<AllocationFilter>,
    #[serde(flatten)]
    pub kind: AllocationExpectationKind,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationExpectationKind {
    PageCount(u64),
    ExactPages(Vec<u32>),
    ContainsPages(Vec<u32>),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
    #[serde(rename = "exitCode")]
    pub exit_code: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Test {
    pub programs: Vec<Program>,
    pub fixtures: Vec<Fixture>,
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

    assert_eq!(test.fixtures[0].messages.len(), 1);
    assert_eq!(test.fixtures[0].messages.len(), 1);
}
