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

use crate::{NO_BUILD_ENV, NO_BUILD_INNER_ENV, NO_PATH_REMAP_ENV};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, env, fmt, path::PathBuf};

pub fn manifest_dir() -> PathBuf {
    env::var("CARGO_MANIFEST_DIR").unwrap().into()
}

pub fn out_dir() -> PathBuf {
    env::var("OUT_DIR").unwrap().into()
}

pub fn profile() -> String {
    out_dir()
        .components()
        .rev()
        .take_while(|c| c.as_os_str() != "target")
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .take_while(|c| c.as_os_str() != "build")
        .last()
        .expect("Path should have subdirs in the `target` dir")
        .as_os_str()
        .to_string_lossy()
        .into()
}

pub fn wasm_projects_dir() -> PathBuf {
    let profile = profile();

    out_dir()
        .ancestors()
        .find(|path| path.ends_with(&profile))
        .and_then(|path| path.parent())
        .map(|p| p.to_owned())
        .expect("Could not find target directory")
        .join("wasm-projects")
}

pub fn wasm32_target_dir() -> PathBuf {
    wasm_projects_dir().join("wasm32-unknown-unknown")
}

pub fn cargo_home_dir() -> PathBuf {
    env::var("CARGO_HOME").unwrap().into()
}

pub fn get_no_build_env() -> bool {
    env::var(NO_BUILD_ENV).is_ok()
}

pub fn get_no_build_inner_env() -> bool {
    env::var(NO_BUILD_INNER_ENV).is_ok()
}

pub fn get_no_path_remap_env() -> bool {
    env::var(NO_PATH_REMAP_ENV).is_ok()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnderscoreString(pub String);

impl UnderscoreString {
    pub fn original(&self) -> &String {
        &self.0
    }

    fn underscore(&self) -> String {
        self.0.replace('-', "_")
    }
}

impl<T: Into<String>> From<T> for UnderscoreString {
    fn from(s: T) -> Self {
        Self(s.into())
    }
}

impl fmt::Display for UnderscoreString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.underscore(), f)
    }
}

impl fmt::Debug for UnderscoreString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl PartialEq for UnderscoreString {
    fn eq(&self, other: &Self) -> bool {
        self.underscore() == other.underscore()
    }
}

impl Eq for UnderscoreString {}

impl PartialOrd for UnderscoreString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for UnderscoreString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.underscore().cmp(&other.underscore())
    }
}
