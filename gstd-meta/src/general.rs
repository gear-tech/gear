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

use crate::Deserialize;
use crate::{BTreeMap, String, Vec};

pub struct CustomType {
    name: String,
    fields: Vec<Field>,
}

impl CustomType {
    pub fn new(name: String, fields: Vec<Field>) -> Self {
        Self { name, fields }
    }

    pub fn to_map(&self) -> BTreeMap<String, BTreeMap<String, String>> {
        let mut fields = BTreeMap::new();

        for field in self.fields.clone() {
            fields.insert(field.name, field.type_name);
        }

        let mut map = BTreeMap::new();

        map.insert(self.name.clone(), fields);

        map
    }
}
#[derive(Deserialize, Clone)]
pub struct Field {
    name: String,
    #[serde(rename(deserialize = "typeName"))]
    type_name: String,
}
