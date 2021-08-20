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

/// **The `inspect!` macro**
#[macro_export]
macro_rules! inspect {
    ($btree:expr, $registry:expr, $type:ty) => {
        $btree.extend({
        let ty = <$type>::type_info().into_portable
        (&mut $registry);

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
