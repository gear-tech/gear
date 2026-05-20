// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{builder_error::BuilderError, multiple_crate_versions};
use anyhow::{Context, Result, ensure};
use cargo_metadata::{CrateType, Dependency, Metadata, MetadataCommand, Package};
use std::{collections::BTreeMap, path::Path};

/// Helper to get a crate info extracted from the `Cargo.toml`.
#[derive(Debug, Default)]
pub struct CrateInfo {
    /// Original name of the crate.
    pub name: String,
    /// Crate name converted to the snake case.
    pub snake_case_name: String,
    /// Crate version.
    pub version: String,
    /// Crate dependencies.
    pub dependencies: Vec<Dependency>,
    /// Crate features.
    pub features: BTreeMap<String, Vec<String>>,
    /// Crate custom profiles
    pub profiles: BTreeMap<String, toml::Value>,
    /// Workspace patches
    pub patch: BTreeMap<String, toml::Value>,
}

impl CrateInfo {
    /// Create a new `CrateInfo` from a path to the `Cargo.toml`.
    pub fn from_manifest(manifest_path: &Path) -> Result<Self> {
        ensure!(
            manifest_path.exists(),
            BuilderError::ManifestPathInvalid(manifest_path.to_path_buf())
        );

        let mut meta_cmd = MetadataCommand::new();
        let metadata = meta_cmd
            .manifest_path(manifest_path)
            // As we are being called inside a build-script, this env variable is set.
            // However, this can lead to cross-compilation errors.
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .exec()
            .context("unable to invoke `cargo metadata`")?;

        let root_package = Self::root_package(&metadata)
            .ok_or_else(|| BuilderError::RootPackageNotFound.into())
            .and_then(Self::check)?;

        let manifest = cargo_toml::Manifest::from_path(metadata.workspace_root.join("Cargo.toml"))
            .context("manifest parsing failed")?;
        let profiles = manifest
            .profile
            .custom
            .into_iter()
            .map(|(k, v)| Ok((k, toml::Value::try_from(v)?)))
            .collect::<Result<_>>()
            .context("failed to convert profile to `toml::Value`")?;
        let patch = manifest
            .patch
            .into_iter()
            .map(|(k, v)| Ok((k, toml::Value::try_from(v)?)))
            .collect::<Result<_>>()
            .context("failed to convert patch to `toml::Value`")?;

        multiple_crate_versions::check(&metadata, &root_package.id)?;

        let name = root_package.name.clone().into_inner();
        let snake_case_name = name.replace('-', "_");
        let version = root_package.version.to_string();
        let dependencies = root_package.dependencies.clone();
        let features = root_package.features.clone();

        Ok(Self {
            name,
            snake_case_name,
            version,
            dependencies,
            features,
            profiles,
            patch,
        })
    }

    fn root_package(metadata: &Metadata) -> Option<&Package> {
        let root_id = metadata.resolve.as_ref()?.root.as_ref()?;
        metadata
            .packages
            .iter()
            .find(|package| package.id == *root_id)
    }

    /// check package
    ///
    /// - if crate-type contains "lib" or "rlib":
    fn check(pkg: &Package) -> Result<&Package> {
        // cargo can't import executables (bin, cdylib etc), but libs
        // only (rlib).
        //
        // if no `[lib]` table, the `crate_types` will be [ "lib" ]
        // by default, we can not detect if this is "rlib" because it
        // is the "compiler recommended" style of library.
        //
        // see also https://doc.rust-lang.org/reference/linkage.html
        let validated_lib = |ty: &CrateType| matches!(ty, CrateType::Lib | CrateType::RLib);
        let pkg_snake_case_name = pkg.name.replace('-', "_");

        let _ = pkg
            .targets
            .iter()
            .find(|target| {
                target.name.eq(&pkg_snake_case_name) && target.crate_types.iter().any(validated_lib)
            })
            .ok_or(BuilderError::CrateTypeInvalid)?;

        Ok(pkg)
    }
}
