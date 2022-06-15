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

use gear_core::ids::ProgramId;
use once_cell::sync::Lazy;
use primitive_types::H256;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

static ACCOUNTS: Lazy<HashMap<&'static str, H256>> = Lazy::new(|| {
    fn public_key(s: &'static str) -> H256 {
        H256::from_slice(hex::decode(s).unwrap().as_slice())
    }

    let mut accounts = HashMap::new();
    accounts.insert(
        "alice",
        public_key("d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"),
    );
    accounts.insert(
        "bob",
        public_key("8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48"),
    );
    accounts.insert(
        "eve",
        public_key("e659a7a1628cdd93febc04a4e0646ea20e9f5f0ce097d9a05290d4a9e054df4e"),
    );
    accounts
});

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value")]
pub enum Address {
    #[serde(rename = "account")]
    Account(String),
    #[serde(rename = "id")]
    ProgramId(u64),
    #[serde(rename = "h256")]
    H256(H256),
}

impl Default for Address {
    fn default() -> Self {
        Self::Account("alice".to_string())
    }
}

impl Address {
    pub fn to_program_id(&self) -> ProgramId {
        match self {
            Self::Account(s) => ProgramId::from(ACCOUNTS.get(s.as_str()).unwrap().as_bytes()),
            Self::ProgramId(id) => ProgramId::from(*id),
            Self::H256(id) => ProgramId::from(id.as_bytes()),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum UntaggedAddress {
    Integer(u64),
    Address(Address),
}

impl From<UntaggedAddress> for Address {
    fn from(a: UntaggedAddress) -> Self {
        match a {
            UntaggedAddress::Address(s) => s,
            UntaggedAddress::Integer(n) => Address::ProgramId(n),
        }
    }
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Address, D::Error> {
    UntaggedAddress::deserialize(deserializer).map(|a| a.into())
}
