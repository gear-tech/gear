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

use crate::{profile, wasm32_target_dir, UnderscoreString};
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::{Seek, SeekFrom},
    path::PathBuf,
};

pub fn file_path(pkg_name: impl AsRef<str>) -> PathBuf {
    let pkg_name = pkg_name.as_ref().replace('-', "_");
    wasm32_target_dir()
        .join(profile())
        .join(format!("{}.lock", pkg_name))
}

#[derive(Debug, Serialize, Deserialize, derive_more::Unwrap)]
#[serde(rename_all = "kebab-case")]
pub enum LockFileConfig {
    Demo(DemoLockFileConfig),
    Builder(BuilderLockFileConfig),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DemoLockFileConfig {
    pub features: BTreeSet<UnderscoreString>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuilderLockFileConfig {
    pub features: BTreeSet<String>,
}

#[derive(Debug)]
pub struct DemoLockFile {
    file: fs::File,
}

impl DemoLockFile {
    pub fn open(pkg_name: impl AsRef<str>) -> Self {
        let path = file_path(pkg_name);
        println!("cargo:warning=[DEMO] lock: {}", path.display());
        let file = fs::File::options()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .unwrap();
        file.lock_exclusive().unwrap();

        Self { file }
    }

    pub fn write(&mut self, config: DemoLockFileConfig) {
        serde_json::to_writer(&mut self.file, &LockFileConfig::Demo(config)).unwrap();
    }
}

#[derive(Debug)]
pub struct BuilderLockFile {
    file: fs::File,
}

impl BuilderLockFile {
    pub fn open(pkg_name: impl AsRef<str>) -> Self {
        let path = file_path(pkg_name);
        let file = fs::File::options()
            .create(true)
            .write(true)
            .read(true)
            .open(path)
            .unwrap();
        file.lock_exclusive().unwrap();

        Self { file }
    }

    pub fn read(&mut self) -> LockFileConfig {
        serde_json::from_reader(&mut self.file).unwrap()
    }

    pub fn write(&mut self, config: BuilderLockFileConfig) {
        self.file.set_len(0).unwrap();
        self.file.seek(SeekFrom::Start(0)).unwrap();
        serde_json::to_writer(&mut self.file, &LockFileConfig::Builder(config)).unwrap();
    }
}
