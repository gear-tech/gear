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

use anyhow::Context;
use lexopt::{Arg, ValueExt};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

#[derive(Debug)]
pub struct RustcArgs {
    pub(crate) input: String,
    pub(crate) crate_name: Option<String>,
    pub(crate) crate_type: Vec<String>,
    pub(crate) target: Option<String>,
    pub(crate) cfg: HashMap<String, Vec<String>>,
}

impl RustcArgs {
    pub(crate) fn new(args: String) -> anyhow::Result<Self> {
        let args = args.split(' ');
        let mut parser = lexopt::Parser::from_iter(args);

        let mut input = None;
        let mut crate_name = None;
        let mut crate_type = vec![];
        let mut target = None;
        let mut cfg = HashMap::<String, Vec<String>>::new();

        while let Some(arg) = parser.next()? {
            match arg {
                Arg::Value(value) => {
                    input = Some(value.string()?);
                }
                Arg::Long("crate-name") => {
                    let value = parser.value()?.string()?;
                    crate_name = Some(value);
                }
                Arg::Long("crate-type") => {
                    let value = parser.value()?.string()?;
                    let value = value.split(',').map(str::to_string);
                    crate_type.extend(value);
                }
                Arg::Long("target") => {
                    let value = parser.value()?.string()?;
                    target = Some(value)
                }
                Arg::Long("cfg") => {
                    let value = parser.value()?.string()?;
                    let mut value = value.splitn(2, '=');
                    let key = value.next().expect("always Some").to_string();
                    let value = value
                        .next()
                        .map(|s| s.trim_matches('"'))
                        .map(str::to_string);
                    cfg.entry(key).or_default().extend(value);
                }
                // we don't care about other rustc flags
                Arg::Long(_) | Arg::Short(_) => {
                    let _ = parser.value();
                }
            }
        }

        Ok(Self {
            input: input.context("`INPUT` argument expected")?,
            crate_name,
            crate_type,
            target,
            cfg,
        })
    }
}

#[derive(Debug)]
pub struct CargoArgs {
    features: HashSet<String>,
    profile: Option<String>,
    release: bool,
    target_dir: Option<PathBuf>,
}

impl CargoArgs {
    pub(crate) fn from_env() -> anyhow::Result<Self> {
        let mut parser = lexopt::Parser::from_env();

        let mut features = HashSet::new();
        let mut profile = None;
        let mut release = false;
        let mut target_dir = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Arg::Short('F') | Arg::Long("features") => {
                    parser.values()?.try_fold(
                        &mut features,
                        |features, value| -> anyhow::Result<&mut _> {
                            let value = value.string()?;
                            features.extend(value.split(',').map(str::to_string));
                            Ok(features)
                        },
                    )?;
                }
                Arg::Long("profile") => {
                    let value = parser.value()?.string()?;
                    profile = Some(value);
                }
                Arg::Short('r') | Arg::Long("release") => {
                    release = true;
                }
                Arg::Long("target-dir") => {
                    let value: PathBuf = parser.value()?.parse()?;
                    target_dir = Some(value);
                }
                Arg::Value(_) => continue,
                // we don't care about other cargo flags
                Arg::Short(_) | Arg::Long(_) => {
                    let _ = parser.value();
                }
            }
        }

        anyhow::ensure!(
            !(release && profile.is_some()),
            "`--release` and `--profile` flags are mutually inclusive"
        );

        Ok(Self {
            features,
            profile,
            release,
            target_dir,
        })
    }

    pub(crate) fn cargo_profile(&self) -> String {
        if let Some(profile) = self.profile.clone() {
            profile
        } else if self.release {
            "release".to_string()
        } else {
            "dev".to_string()
        }
    }

    pub(crate) fn dir_profile(&self) -> String {
        let profile = self.cargo_profile();
        if profile == "dev" {
            "debug".to_string()
        } else {
            profile
        }
    }

    pub(crate) fn features(&self) -> &HashSet<String> {
        &self.features
    }

    pub(crate) fn target_dir(&self) -> Option<&PathBuf> {
        self.target_dir.as_ref()
    }
}
