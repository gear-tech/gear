// // This file is part of Gear.

// // Copyright (C) 2021 Gear Technologies Inc.
// // SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// // This program is free software: you can redistribute it and/or modify
// // it under the terms of the GNU General Public License as published by
// // the Free Software Foundation, either version 3 of the License, or
// // (at your option) any later version.

// // This program is distributed in the hope that it will be useful,
// // but WITHOUT ANY WARRANTY; without even the implied warranty of
// // MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// // GNU General Public License for more details.

// // You should have received a copy of the GNU General Public License
// // along with this program. If not, see <https://www.gnu.org/licenses/>.

// #![no_std]

// use gstd_meta::*;

// extern crate alloc;
// use alloc::{string::String, vec::Vec};

// #[derive(TypeInfo)]
// pub struct SaltAmount {
//     pub value: u8,
// }

// #[derive(TypeInfo)]
// pub struct FreshMeat {
//     pub name: String,
//     pub salt: Option<SaltAmount>,
// }

// #[derive(TypeInfo)]
// pub struct Egg {
//     pub weight: u32,
//     pub ostrich: bool,
// }

// #[derive(TypeInfo)]
// pub struct Sauce {
//     pub eggs: Vec<Egg>,
//     pub salty: Result<SaltAmount, SaltAmount>,
// }

// #[derive(TypeInfo)]
// pub struct Meal {
//     pub steak: FreshMeat,
//     pub mayonnaise: Sauce,
// }

// // Function for more visual testing
// fn check(types: Vec<MetaType>, expectation: &'static str) {
//     assert_eq!(
//         gstd_meta::to_json(types),
//         expectation.replace("\n", "").replace(" ", "")
//     )
// }

// #[test]
// fn primitives_json() {
//     check(
//         types!(bool, char, str, String, u8, u16, u32, u64, u128, i8, i16, i32, i64, i128),
//         "{}",
//     );
// }

// #[test]
// fn simple_json() {
//     check(
//         types!(SaltAmount),
//         r#"{
//           "SaltAmount": {
//             "value": "u8"
//           }
//         }"#,
//     );

//     check(
//         types!(FreshMeat),
//         r#"{
//           "FreshMeat": {
//             "name": "String",
//             "salt": "Option<SaltAmount>"
//           }
//         }"#,
//     );

//     check(
//         types!(Egg),
//         r#"{
//           "Egg": {
//             "ostrich": "bool",
//             "weight": "u32"
//           }
//         }"#,
//     );

//     check(
//         types!(Sauce),
//         r#"{
//           "Sauce": {
//             "eggs": "Vec<Egg>",
//             "salty": "Result<SaltAmount,SaltAmount>"
//           }
//         }"#,
//     );

//     check(
//         types!(Meal),
//         r#"{
//           "Meal": {
//             "mayonnaise": "Sauce",
//             "steak": "FreshMeat"
//           }
//         }"#,
//     );
// }

// #[test]
// fn complex_json() {
//     check(
//         types!(FreshMeat, SaltAmount),
//         r#"{
//           "FreshMeat": {
//             "name": "String",
//             "salt": "Option<SaltAmount>"
//           },
//           "SaltAmount": {
//             "value": "u8"
//           }
//         }"#,
//     );

//     // There is also check on repeating
//     check(
//         types!(FreshMeat, SaltAmount, SaltAmount, SaltAmount),
//         r#"{
//           "FreshMeat": {
//             "name": "String",
//             "salt": "Option<SaltAmount>"
//           },
//           "SaltAmount": {
//             "value": "u8"
//           }
//         }"#,
//     );

//     check(
//         types!(Meal, FreshMeat, Sauce, Egg, SaltAmount),
//         r#"{
//           "Egg": {
//             "ostrich": "bool",
//             "weight": "u32"
//           },
//           "FreshMeat": {
//             "name": "String",
//             "salt": "Option<SaltAmount>"
//           },
//           "Meal": {
//             "mayonnaise": "Sauce",
//             "steak": "FreshMeat"
//           },
//           "SaltAmount": {
//             "value": "u8"
//           },
//           "Sauce": {
//             "eggs": "Vec<Egg>",
//             "salty": "Result<SaltAmount,SaltAmount>"
//           }
//         }"#,
//     );
// }
