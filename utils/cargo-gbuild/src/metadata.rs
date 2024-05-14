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

use crate::utils;
use anyhow::{anyhow, Result};
use cargo_metadata::{Artifact, CargoOpt, Message, MetadataCommand};
use serde::{Deserialize, Serialize};
use std::{
    io::BufReader,
    path::PathBuf,
    process::{Command, Stdio},
};

/// Cargo metadata
pub struct Metadata {
    artifact: Vec<Artifact>,
    inner: cargo_metadata::Metadata,
}

impl Metadata {
    /// Get project metadata from command `cargo-metadata`
    pub fn parse(manifest: Option<PathBuf>, features: Vec<String>) -> Result<Self> {
        let mut command = MetadataCommand::new();
        command.features(CargoOpt::SomeFeatures(features));
        if let Some(manifest) = manifest {
            command.manifest_path(manifest);
        }

        Ok(Self {
            artifact: Self::artifacts()?,
            inner: command.exec()?,
        })
    }

    /// Parse the artifact path
    fn artifacts() -> Result<Vec<Artifact>> {
        let mut check = Command::new("cargo")
            .args([
                "check",
                "--workspace",
                "--message-format=json-render-diagnostics",
            ])
            .stdout(Stdio::piped())
            .spawn()?;

        let reader = BufReader::new(
            check
                .stdout
                .take()
                .ok_or(anyhow!("Failed to get stdout, strerr: {:?}", check.stderr))?,
        );
        let mut artifacts: Vec<Artifact> = Default::default();
        for message in Message::parse_stream(reader).flatten() {
            if let Message::CompilerArtifact(artifact) = message {
                artifacts.push(artifact);
            }
        }

        Ok(artifacts)
    }
}

/// Gbuild metadata
#[derive(Serialize, Deserialize)]
pub struct GbuildMetadata {
    /// Gear programs in the workspace.
    programs: Vec<String>,
    /// Gear program metas in the workspace.
    metas: Vec<String>,
}

impl GbuildMetadata {
    /// Collect all gear programs
    pub fn programs(&self) -> Result<Vec<PathBuf>> {
        utils::collect_crates(&self.programs)
    }

    /// Collect all gear metas
    pub fn metas(&self) -> Result<Vec<PathBuf>> {
        utils::collect_crates(&self.metas)
    }
}

/// Cargo gbuild metadata
#[derive(Serialize, Deserialize)]
pub struct MetadataField {
    /// Gbuild metadata,
    pub gbuild: GbuildMetadata,
}
