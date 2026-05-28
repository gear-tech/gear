// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use anyhow::Result;
use cargo_metadata::{CargoOpt, MetadataCommand};
use serde::{Deserialize, Serialize};
use std::{ops::Deref, path::PathBuf};

/// Cargo metadata
pub struct Metadata {
    /// Raw cargo metadata
    inner: cargo_metadata::Metadata,

    /// Gbuild metadata
    pub gbuild: GbuildMetadata,

    /// Which manifest this metadata if parsed by
    pub manifest: PathBuf,

    /// If workspace flag is enabled
    pub workspace: bool,
}

impl Metadata {
    /// Get project metadata from command `cargo-metadata`
    pub fn parse(workspace: bool, manifest: PathBuf, features: Vec<String>) -> Result<Self> {
        let mut command = MetadataCommand::new();
        command.features(CargoOpt::SomeFeatures(features));
        command.manifest_path(&manifest);

        let inner = command.exec()?;
        let gbuild = serde_json::from_value::<MetadataField>(inner.workspace_metadata.clone())
            .map(|mut m| {
                m.gbuild.programs.dedup();
                m.gbuild
            })
            .unwrap_or_default();

        Ok(Self {
            inner,
            workspace,
            gbuild,
            manifest,
        })
    }
}

impl Deref for Metadata {
    type Target = cargo_metadata::Metadata;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Cargo gbuild metadata
///
/// In the root cargo.toml: [workspace.metadata.gbuild]
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MetadataField {
    /// Gbuild metadata,
    pub gbuild: GbuildMetadata,
}

/// Gbuild metadata
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GbuildMetadata {
    /// Gear programs in the workspace.
    pub programs: Vec<String>,
}
