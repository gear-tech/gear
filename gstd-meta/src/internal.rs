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

use crate::prelude::{BTreeMap, String, ToString, Vec};
use scale_info::{IntoPortable, MetaType, Registry};
use serde_json::to_value;

pub(crate) fn inspect(meta_type: MetaType) -> (String, BTreeMap<String, String>) {
    let type_info = meta_type.type_info();

    let name = type_info.path().ident();
    let mut map = BTreeMap::<String, String>::new();

    let mut reg = Registry::new();
    let ty = type_info.into_portable(&mut reg);
    let mut v = to_value(ty).expect("Unable to convert MetaType into serde::Value");

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

    for f in v["fields"].as_array().unwrap().iter() {
        map.insert(
            f["name"].as_str().unwrap().into(),
            f["typeName"]
                .as_str()
                .unwrap()
                .split("::")
                .last()
                .unwrap()
                .into(),
        );
    }

    (name.into(), map)
}

pub(crate) fn inspect_many(types: Vec<MetaType>) -> Vec<(String, BTreeMap<String, String>)> {
    types.iter().map(|ty| inspect(*ty)).collect()
}

pub(crate) fn to_map(
    head: &(String, BTreeMap<String, String>),
) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut map = BTreeMap::new();

    if head.1.is_empty() {
        map.insert(head.0.clone(), BTreeMap::new());
    } else {
        map.insert(head.0.clone(), head.1.clone());
    }

    map
}
