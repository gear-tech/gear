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
#![cfg_attr(feature = "strict", deny(warnings))]

extern crate alloc;

pub(crate) use alloc::{collections::BTreeMap, string::ToString, vec::Vec};

pub use alloc::{boxed::Box, string::String, vec};
use scale_info::{IntoPortable, Registry};
pub use scale_info::{MetaType, TypeInfo};

mod declare;

fn inspect(meta_type: MetaType) -> (String, BTreeMap<String, String>) {
    let type_info = meta_type.type_info();

    let name = type_info.path().ident();
    let mut map = BTreeMap::new();

    let mut reg = Registry::new();
    let ty = type_info.into_portable(&mut reg);
    let mut v = serde_json::to_value(ty).expect("Unable to convert MetaType into serde::Value");

    if name.is_none() {
        let mut name = v["def"]["primitive"].to_string().replace("\"", "");
        if name == "str" {
            name = "String".into();
        }
        return (name, map);
    }

    let name = name.unwrap();

    if v["def"]["composite"].is_null() {
        panic!("Invalid data structure");
    } else {
        v = v["def"]["composite"].take();
    }

    if !v["fields"].is_array() {
        panic!("Invalid data structure");
    }

    for f in v["fields"].as_array().unwrap().iter() {
        map.insert(
            f["name"].as_str().expect("Invalid data structure").into(),
            f["typeName"]
                .as_str()
                .unwrap()
                .split("::")
                .last()
                .unwrap()
                .replace(" ", ""),
        );
    }

    (name.into(), map)
}

/// **The `meta!` macro**
#[macro_export]
macro_rules! meta {
    (
        title: $t:literal,
        input: $ti:ty,
        output: $to:ty,
        init_input: $ii:ty,
        init_output: $io:ty
    ) => {
        gstd_meta::meta!(
            title: $t, input: $ti, output: $to, init_input: $ii, init_output: $io, extra: u8
        );
    };
    (
        title: $t:literal,
        input: $ti:ty,
        output: $to:ty,
        init_input: $ii:ty,
        init_output: $io:ty,
        extra: $($x:ty), +
    ) => {
        gstd_meta::declare!(meta_input, stringify!($ti));
        gstd_meta::declare!(meta_output, stringify!($to));
        gstd_meta::declare!(meta_init_input, stringify!($ii));
        gstd_meta::declare!(meta_init_output, stringify!($io));
        gstd_meta::declare!(meta_title, $t);
        gstd_meta::declare!(meta_types, $ti, $to, $ii, $io, $($x), +);
    };
}

pub fn to_json(types: Vec<MetaType>) -> String {
    let data: Vec<(String, BTreeMap<String, String>)> = types
        .into_iter()
        .map(inspect)
        .filter(|v| !v.1.is_empty())
        .collect();

    let mut btree = BTreeMap::new();

    for note in data {
        if !btree.contains_key(&note.0) {
            btree.insert(note.0.clone(), note.1);
        }
    }

    serde_json::to_value(btree)
        .expect("Unable to convert Vec into serde::Value")
        .to_string()
}

/// **The `types!` macro**
#[macro_export]
macro_rules! types {
    ($($t:ty), +) => { gstd_meta::vec![$(gstd_meta::MetaType::new::<$t>()), +] };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_primitives() {
        assert_eq!(
            inspect(MetaType::new::<bool>()),
            ("bool".into(), BTreeMap::new())
        );
        assert_eq!(
            inspect(MetaType::new::<char>()),
            ("char".into(), BTreeMap::new())
        );
        assert_eq!(
            inspect(MetaType::new::<str>()),
            ("String".into(), BTreeMap::new())
        );
        assert_eq!(
            inspect(MetaType::new::<String>()),
            ("String".into(), BTreeMap::new())
        );
        assert_eq!(
            inspect(MetaType::new::<u8>()),
            ("u8".into(), BTreeMap::new())
        );
        assert_eq!(
            inspect(MetaType::new::<u16>()),
            ("u16".into(), BTreeMap::new())
        );
        assert_eq!(
            inspect(MetaType::new::<u32>()),
            ("u32".into(), BTreeMap::new())
        );
        assert_eq!(
            inspect(MetaType::new::<u64>()),
            ("u64".into(), BTreeMap::new())
        );
        assert_eq!(
            inspect(MetaType::new::<u128>()),
            ("u128".into(), BTreeMap::new())
        );
        assert_eq!(
            inspect(MetaType::new::<i8>()),
            ("i8".into(), BTreeMap::new())
        );
        assert_eq!(
            inspect(MetaType::new::<i16>()),
            ("i16".into(), BTreeMap::new())
        );
        assert_eq!(
            inspect(MetaType::new::<i32>()),
            ("i32".into(), BTreeMap::new())
        );
        assert_eq!(
            inspect(MetaType::new::<i64>()),
            ("i64".into(), BTreeMap::new())
        );
        assert_eq!(
            inspect(MetaType::new::<i128>()),
            ("i128".into(), BTreeMap::new())
        );
    }

    #[test]
    fn inspect_struct() {
        #[derive(TypeInfo)]
        struct StructName {
            _a: u8,
            _b: String,
            _c: (u32, i32),
            _d: Vec<i64>,
            _e: Option<bool>,
            _f: Result<char, u128>,
        }
        let mut map = BTreeMap::<String, String>::new();
        map.insert("_a".into(), "u8".into());
        map.insert("_b".into(), "String".into());
        map.insert("_c".into(), "(u32,i32)".into());
        map.insert("_d".into(), "Vec<i64>".into());
        map.insert("_e".into(), "Option<bool>".into());
        map.insert("_f".into(), "Result<char,u128>".into());

        assert_eq!(
            inspect(MetaType::new::<StructName>()),
            ("StructName".into(), map)
        );
    }

    #[test]
    #[should_panic(expected = "Invalid data structure")]
    fn inspect_solo_option() {
        inspect(MetaType::new::<Option<u8>>());
    }

    #[test]
    #[should_panic(expected = "Invalid data structure")]
    fn inspect_solo_result() {
        inspect(MetaType::new::<Result<u8, u8>>());
    }

    #[test]
    #[should_panic(expected = "Invalid data structure")]
    fn inspect_empty_struct() {
        #[derive(TypeInfo)]
        struct EmptyStruct;

        inspect(MetaType::new::<EmptyStruct>());
    }

    #[test]
    #[should_panic(expected = "Invalid data structure")]
    fn inspect_tuple_struct() {
        #[derive(TypeInfo)]
        struct TupleStruct(u8);

        inspect(MetaType::new::<TupleStruct>());
    }

    #[test]
    #[should_panic(expected = "Invalid data structure")]
    fn inspect_enum() {
        #[derive(TypeInfo)]
        enum Variant {
            _A,
            _B,
        }

        inspect(MetaType::new::<Variant>());
    }
}
