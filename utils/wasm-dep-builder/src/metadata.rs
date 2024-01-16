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

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PackageMetadata {
    wasm_dep_builder: Option<WasmDepBuilderMetadata>,
}

#[derive(Deserialize, derive_more::Unwrap)]
#[serde(rename_all = "kebab-case")]
enum WasmDepBuilderMetadata {
    Demo(DemoMetadata),
    Builder(BuilderMetadata),
}

impl WasmDepBuilderMetadata {
    fn from_value(value: serde_json::Value) -> Option<Self> {
        serde_json::from_value::<Option<PackageMetadata>>(value)
            .unwrap()
            .and_then(|metadata| metadata.wasm_dep_builder)
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DemoMetadata {
    #[serde(default)]
    pub exclude_features: BTreeSet<String>,
}

impl DemoMetadata {
    pub fn from_value(value: serde_json::Value) -> Self {
        WasmDepBuilderMetadata::from_value(value)
            .map(|metadata| metadata.unwrap_demo())
            .unwrap_or_default()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BuilderMetadata {
    #[serde(default = "BuilderMetadata::default_include")]
    include: GlobSet,
    #[serde(default)]
    exclude: BTreeSet<String>,
}

impl Default for BuilderMetadata {
    fn default() -> Self {
        Self {
            include: Self::default_include(),
            exclude: Default::default(),
        }
    }
}

impl BuilderMetadata {
    pub fn from_value(value: serde_json::Value) -> Self {
        WasmDepBuilderMetadata::from_value(value)
            .map(|metadata| metadata.unwrap_builder())
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
