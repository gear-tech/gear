// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use globset::{Glob, GlobSet};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Deserialize, derive_more::Unwrap)]
#[serde(rename_all = "kebab-case")]
enum CrateMetadata {
    Program(ProgramMetadata),
    Binaries(BinariesMetadata),
}

impl CrateMetadata {
    fn from_value(mut value: serde_json::Value) -> Option<Self> {
        let value = value.get_mut(env!("CARGO_PKG_NAME"))?.take();
        Some(serde_json::from_value::<Self>(value).unwrap())
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProgramMetadata {
    #[serde(default)]
    pub exclude_features: BTreeSet<String>,
}

impl ProgramMetadata {
    pub fn from_value(value: serde_json::Value) -> Self {
        CrateMetadata::from_value(value)
            .map(|metadata| metadata.unwrap_program())
            .unwrap_or_default()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BinariesMetadata {
    #[serde(default = "BinariesMetadata::default_include")]
    include: GlobSet,
    #[serde(default)]
    exclude: BTreeSet<String>,
}

impl Default for BinariesMetadata {
    fn default() -> Self {
        Self {
            include: Self::default_include(),
            exclude: Default::default(),
        }
    }
}

impl BinariesMetadata {
    pub fn from_value(value: serde_json::Value) -> Self {
        CrateMetadata::from_value(value)
            .map(|metadata| metadata.unwrap_binaries())
            .unwrap_or_default()
    }

    fn default_include() -> GlobSet {
        GlobSet::builder()
            .add(Glob::new("demo-*").unwrap())
            .build()
            .unwrap()
    }

    pub fn filter_dep(&self, pkg_name: &str) -> bool {
        !self.exclude.contains(pkg_name) && self.include.is_match(pkg_name)
    }
}
