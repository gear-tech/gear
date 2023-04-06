// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Tests for metadata
use crate::meta::Meta;

const WASM_METADATA_OUTPUT: &str = r#"
Metadata {
    init:  {
        input: MessageInitIn {
            amount: "u8",
            currency: "String",
        },
        output: MessageInitOut {
            exchange_rate: "Result<u8, u8>",
            sum: "u8",
        },
    },
    handle:  {
        input: MessageIn {
            id: "Id",
        },
        output: MessageOut {
            res: "Option<Wallet>",
        },
    },
    reply:  {
        input: str,
        output: [u16],
    },
    others:  {
        input: MessageAsyncIn {
            empty: "()",
        },
        output: Option<u8>,
    },
    signal: "()",
    state: [Wallet { id: "Id", person: "Person" }],
}
"#;

const META_WASM_V1_OUTPUT: &str = r#"
Exports {
    first_and_last_wallets:  {
        input: "()",
        output: (
            "Option<Wallet>",
            "Option<Wallet>",
        ),
    },
    first_wallet:  {
        input: "()",
        output: Option<Wallet>,
    },
    last_wallet:  {
        input: "()",
        output: Option<Wallet>,
    },
}
"#;

const META_WASM_V2_OUTPUT: &str = r#"
Exports {
    wallet_by_id:  {
        input: Id {
            decimal: "u64",
            hex: "Vec<u8>",
        },
        output: Option<Wallet>,
    },
    wallet_by_name_and_surname:  {
        input: (
            "str",
            "str",
        ),
        output: Option<Wallet>,
    },
    wallet_by_person:  {
        input: Person {
            surname: "String",
            name: "String",
        },
        output: Option<Wallet>,
    },
}
"#;

const META_WASM_V3_OUTPUT: &str = r#"
Exports {
    block_number:  {
        input: "()",
        output: u32,
    },
    block_timestamp:  {
        input: "()",
        output: u64,
    },
}
"#;

#[test]
fn test_parse_metadata_works() {
    use demo_new_meta::WASM_METADATA;
    let meta = Meta::decode(WASM_METADATA).expect("Failed to decode wasm metadata");
    assert_eq!(format!("{:#}", meta), WASM_METADATA_OUTPUT.trim());
}

#[test]
fn test_parse_metawasm_data_1_works() {
    use demo_new_meta::META_WASM_V1;
    let meta = Meta::decode_wasm(META_WASM_V1).unwrap();
    assert_eq!(format!("{:#}", meta), META_WASM_V1_OUTPUT.trim());
}

#[test]
fn test_parse_metawasm_data_2_works() {
    use demo_new_meta::META_WASM_V2;
    let meta = Meta::decode_wasm(META_WASM_V2).unwrap();
    assert_eq!(format!("{:#}", meta), META_WASM_V2_OUTPUT.trim());
}

#[test]
fn test_parse_metawasm_data_3_works() {
    use demo_new_meta::META_WASM_V3;
    let meta = Meta::decode_wasm(META_WASM_V3).unwrap();
    assert_eq!(format!("{:#}", meta), META_WASM_V3_OUTPUT.trim());
}
