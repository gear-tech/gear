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

/// **The `meta!` macro**
#[macro_export]
macro_rules! meta {
    (
        title: $title:literal,
        input: $type_in:ty,
        output: $type_out:ty,
        init_input: $type_init_in:ty,
        init_output: $type_init_out:ty
    ) => {
        declare!(meta_title, $title);
        declare!(meta_input, $type_in);
        declare!(meta_output, $type_out);
        declare!(meta_init_input, $type_init_in);
        declare!(meta_init_output, $type_init_out);
    };
    (
        title: $title:literal,
        input: $type_in:ty,
        output: $type_out:ty,
        init_input: $type_init_in:ty,
        init_output: $type_init_out:ty,
        extra_types: $($extra:ty), +
    ) => {
        declare!(meta_title, $title);
        declare!(meta_input, $type_in : $($extra), +);
        declare!(meta_output, $type_out: $($extra), +);
        declare!(meta_init_input, $type_init_in: $($extra), +);
        declare!(meta_init_output, $type_init_out: $($extra), +);
    };
}

macro_rules! declare {
    ($func_name:ident, $text:literal) => {
        #[allow(improper_ctypes_definitions)]
        #[no_mangle]
        pub unsafe extern "C" fn $func_name() -> String {
            $text.into()
        }
    };
    ($func_name:ident, $type:ty) => {
        #[allow(improper_ctypes_definitions)]
        #[no_mangle]
        pub unsafe extern "C" fn $func_name() -> String {
            let mut registry = Registry::new();

            let mut btree: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

            inspect!(btree, registry, $type);

            let string = serde_json::to_string(&serde_json::to_value(btree).unwrap()).unwrap();

            string
        }
    };
    ($func_name:ident, $type:ty : $($others:ty), +) => {
        #[allow(improper_ctypes_definitions)]
        #[no_mangle]
        pub unsafe extern "C" fn $func_name() -> String {
            let mut registry = Registry::new();

            let mut btree: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

            inspect!(btree, registry, $type, $($others), +);

            let string = serde_json::to_string(&serde_json::to_value(btree).unwrap()).unwrap();

            string
        }
    };
}

macro_rules! inspect {
    ($btree:expr, $registry:expr, $type:ty) => {
        $btree.extend({
        let ty = <$type>::type_info().into_portable(&mut $registry);

        let v = serde_json::to_value(ty).unwrap();

        let name =
            serde_json::to_string(&v["path"].as_array().unwrap().last().unwrap())
            .unwrap()
                .replace("\"", "");

        let fields_value: Vec<Value> = serde_json::from_str(
            &serde_json::to_string(&v["def"]["composite"]["fields"]).unwrap(),
        )
        .unwrap();

        let mut fields: Vec<Field> = vec![];

        for value in fields_value {
            fields.push(serde_json::from_value(value).unwrap());
        }

        let tt = CustomType::new(name, fields);

        tt.to_map()
    })};
    ($btree:expr, $registry:expr, $type:ty, $($others:ty), +) => {
        inspect!($btree, $registry, $type);
        inspect!($btree, $registry, $($others), +);
    };
}