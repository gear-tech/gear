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

use crate::internal::*;
use crate::{vec, BTreeMap, MetaType, String, ToString, Vec};

pub fn to_json(types: Vec<MetaType>) -> String {
    let data: Vec<(String, BTreeMap<String, String>)> = types.into_iter().map(inspect).collect();

    let mut json = vec![];

    let mut add = vec![];

    if !data.is_empty() {
        let head = &data[0];
        json.push(to_map(head));
        if !head.1.is_empty() {
            for i in 1..data.len() {
                if !add.contains(&data[i].0)
                    && json.iter().any(|h| {
                        h.values()
                            .any(|v| v.values().any(|k| k.contains(&data[i].0)))
                    })
                {
                    json.push(to_map(&data[i]));
                    add.push(data[i].0.clone());
                }
            }
        }
    }

    serde_json::to_value(json)
        .expect("Unable to convert Vec into serde::Value")
        .to_string()
}

/// **The `types!` macro**
#[macro_export]
macro_rules! types {
    ($($t:ty), +) => { gstd_meta::vec![$(gstd_meta::MetaType::new::<$t>()), +] };
}
