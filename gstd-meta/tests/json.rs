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

#![no_std]

use gstd_meta::*;

extern crate alloc;
use alloc::{string::String, vec::Vec};

#[derive(TypeInfo)]
pub struct SaltAmount {
    pub value: u8,
}

#[derive(TypeInfo)]
pub struct Meat {
    pub name: String,
    pub salt: Option<SaltAmount>,
}

#[derive(TypeInfo)]
pub struct Egg {
    pub weight: u32,
    pub ostrich: bool,
}

#[derive(TypeInfo)]
pub struct Sauce {
    pub eggs: Vec<Egg>,
    pub salty: Result<SaltAmount, SaltAmount>,
}

#[derive(TypeInfo)]
pub struct Meal {
    pub steak: Meat,
    pub mayonnaise: Sauce,
}

// Function for more visual testing
fn check(types: Vec<MetaType>, expectation: &'static str) {
    assert_eq!(
        gstd_meta::to_json(types),
        expectation.replace("\n", "").replace(" ", "")
    )
}

#[test]
fn primitives_json() {
    check(
        types!(bool),
        r#"[
            {
                "bool": {}
            }
        ]"#,
    );
    check(
        types!(char),
        r#"[
            {
                "char": {}
            }
        ]"#,
    );
    check(
        types!(str),
        r#"[
            {
                "String": {}
            }
        ]"#,
    );
    check(
        types!(String),
        r#"[
            {
                "String": {}
            }
        ]"#,
    );
    check(
        types!(u8),
        r#"[
            {
                "u8": {}
            }
        ]"#,
    );
    check(
        types!(u16),
        r#"[
            {
                "u16": {}
            }
        ]"#,
    );
    check(
        types!(u32),
        r#"[
            {
                "u32": {}
            }
        ]"#,
    );
    check(
        types!(u64),
        r#"[
            {
                "u64": {}
            }
        ]"#,
    );
    check(
        types!(u128),
        r#"[
            {
                "u128": {}
            }
        ]"#,
    );
    check(
        types!(i8),
        r#"[
            {
                "i8": {}
            }
        ]"#,
    );
    check(
        types!(i16),
        r#"[
            {
                "i16": {}
            }
        ]"#,
    );
    check(
        types!(i32),
        r#"[
            {
                "i32": {}
            }
        ]"#,
    );
    check(
        types!(i64),
        r#"[
            {
                "i64": {}
            }
        ]"#,
    );
    check(
        types!(i128),
        r#"[
            {
                "i128": {}
            }
        ]"#,
    );
}

#[test]
fn simple_json() {
    check(
        types!(SaltAmount),
        r#"[
            {
              "SaltAmount": {
                "value": "u8"
              }
            }
        ]"#,
    );
    check(
        types!(Meat),
        r#"[
            {
              "Meat": {
                "name": "String",
                "salt": "Option<SaltAmount>"
              }
            }
        ]"#,
    );
    check(
        types!(Egg),
        r#"[
            {
              "Egg": {
                "ostrich": "bool",
                "weight": "u32"
              }
            }
        ]"#,
    );
    check(
        types!(Sauce),
        r#"[
            {
              "Sauce": {
                "eggs": "Vec<Egg>",
                "salty": "Result<SaltAmount,SaltAmount>"
              }
            }
        ]"#,
    );
    check(
        types!(Meal),
        r#"[
            {
              "Meal": {
                "mayonnaise": "Sauce",
                "steak": "Meat"
              }
            }
        ]"#,
    );
}

#[test]
fn complex_json() {
    // If first type from `types!` contains type
    // which comes after him in macro invocation,
    // the type would be added into json...
    check(
        types!(Meat, SaltAmount),
        r#"[
            {
              "Meat": {
                "name": "String",
                "salt": "Option<SaltAmount>"
              }
            },
            {
              "SaltAmount": {
                "value": "u8"
              }
            }
        ]"#,
    );
    // ..if not, it wouldn't
    check(
        types!(Meat, Sauce),
        r#"[
            {
              "Meat": {
                "name": "String",
                "salt": "Option<SaltAmount>"
              }
            }
        ]"#,
    );
    // There is also check on repeating
    check(
        types!(Meat, SaltAmount, SaltAmount, SaltAmount),
        r#"[
            {
              "Meat": {
                "name": "String",
                "salt": "Option<SaltAmount>"
              }
            },
            {
              "SaltAmount": {
                "value": "u8"
              }
            }
        ]"#,
    );

    // Try to specify the types in such an order that the main element is always the first,
    // if type A includes type B, then type A will be to the left of type B
    check(
        types!(Meat, SaltAmount, Meal, Sauce, Egg),
        r#"[
            {
              "Meat": {
                "name": "String",
                "salt": "Option<SaltAmount>"
              }
            },
            {
              "SaltAmount": {
                "value": "u8"
              }
            }
        ]"#,
    );

    check(
        types!(Meal, Meat, Sauce, Egg, SaltAmount),
        r#"[
            {
              "Meal": {
                "mayonnaise": "Sauce",
                "steak": "Meat"
              }
            },
            {
              "Meat": {
                "name": "String",
                "salt": "Option<SaltAmount>"
              }
            },
            {
              "Sauce": {
                "eggs": "Vec<Egg>",
                "salty": "Result<SaltAmount,SaltAmount>"
              }
            },
            {
              "Egg": {
                "ostrich": "bool",
                "weight": "u32"
              }
            },
            {
              "SaltAmount": {
                "value": "u8"
              }
            }
        ]"#,
    );

    // ...or function result would be incorrect
    check(
        types!(Meal, SaltAmount, Meat, Sauce, Egg),
        r#"[
            {
              "Meal": {
                "mayonnaise": "Sauce",
                "steak": "Meat"
              }
            },
            {
              "Meat": {
                "name": "String",
                "salt": "Option<SaltAmount>"
              }
            },
            {
              "Sauce": {
                "eggs": "Vec<Egg>",
                "salty": "Result<SaltAmount,SaltAmount>"
              }
            },
            {
              "Egg": {
                "ostrich": "bool",
                "weight": "u32"
              }
            }
        ]"#,
    );
}
