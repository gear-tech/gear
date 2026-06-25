// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Packages publisher.

use crate::{
    CRATES_IO_CATEGORIES, GEAR_SUBSTRATE_DEPENDENCIES, Manifest, PACKAGES, PackageStatus,
    SAFE_DEPENDENCIES, STACKED_DEPENDENCIES, Simulator, TEAM_OWNER, Workspace, handler,
};
use anyhow::{Result, bail};
use cargo_metadata::{Metadata, MetadataCommand};
use std::{collections::BTreeMap, path::PathBuf};

/// crates-io packages publisher.
pub struct Publisher {
    metadata: Metadata,
    graph: Vec<Manifest>,
    index: Vec<&'static str>,
    workspace: Option<Workspace>,
    simulator: Option<Simulator>,
}

impl Publisher {
    /// Create a new publisher.
    pub fn new() -> Result<Self> {
        Self::with_simulation(false, None)
    }

    /// Create a new publisher with simulation.
    pub fn with_simulation(simulate: bool, registry_path: Option<PathBuf>) -> Result<Self> {
        Ok(Self {
            metadata: MetadataCommand::new().no_deps().exec()?,
            graph: vec![],
            index: [
                GEAR_SUBSTRATE_DEPENDENCIES,
                SAFE_DEPENDENCIES,
                STACKED_DEPENDENCIES,
                PACKAGES,
            ]
            .concat(),
            workspace: None,
            simulator: simulate
                .then(|| Simulator::new(registry_path))
                .transpose()?,
        })
    }

    /// Build package graphs
    ///
    /// 1. Replace git dependencies to crates-io dependencies.
    /// 2. Rename version of all local packages
    /// 3. Patch dependencies if needed
    pub async fn build(mut self, verify: bool, version: Option<String>) -> Result<Self> {
        let mut workspace = Workspace::lookup(version)?;
        let workspace_version = workspace.version()?;
        let mut package_versions = BTreeMap::new();

        for name in self.index.iter() {
            let Some(pkg) = self.metadata.packages.iter().find(|pkg| *pkg.name == *name) else {
                bail!("Package {name}@{workspace_version} not found in cargo metadata!");
            };

            if pkg.authors.is_empty() {
                bail!("Package {name} has empty authors!");
            }

            if pkg
                .description
                .as_ref()
                .map(|v| v.is_empty())
                .unwrap_or(true)
            {
                bail!("Package {name} has empty description!");
            }

            if pkg.license.is_none() {
                bail!("Package {name} has empty license!");
            }

            // https://doc.rust-lang.org/cargo/reference/manifest.html#the-categories-field
            if pkg.categories.is_empty() {
                bail!("Package {name} has empty categories!");
            }

            if pkg.categories.len() > 5 {
                bail!("Package {name} has more than 5 categories!");
            }

            for category in &pkg.categories {
                if !CRATES_IO_CATEGORIES.contains(&category.as_str()) {
                    bail!("Package {name} has invalid category `{category}`!");
                }
            }

            // https://doc.rust-lang.org/cargo/reference/manifest.html#the-keywords-field
            if pkg.keywords.is_empty() {
                bail!("Package {name} has empty keywords!");
            }

            if pkg.keywords.len() > 5 {
                bail!("Package {name} has more than 5 keywords!");
            }

            for keyword in &pkg.keywords {
                if keyword.len() > 20 {
                    bail!("Package {name} has keyword `{keyword}` longer than 20 characters!");
                }

                if !keyword
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_alphanumeric())
                {
                    bail!(
                        "Package {name} has keyword `{keyword}` that does not start with an \
                        alphanumeric character!"
                    );
                }

                if !keyword
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '+'))
                {
                    bail!("Package {name} has keyword `{keyword}` with disallowed characters!");
                }
            }

            if pkg.repository.is_none() {
                bail!("Package {name} has empty repository!");
            }

            if pkg.homepage.is_none() {
                bail!("Package {name} has empty homepage!");
            }

            if pkg.documentation.is_none() {
                bail!("Package {name} has empty documentation!");
            }

            if pkg.rust_version.is_none() {
                bail!("Package {name} has empty rust-version!");
            }

            let package_version = if GEAR_SUBSTRATE_DEPENDENCIES.contains(name) {
                pkg.version.to_string()
            } else {
                workspace_version.clone()
            };
            package_versions.insert(name.to_string(), package_version.clone());

            let mut is_published = false;
            let mut is_actualized = false;

            if verify {
                match crate::verify_owners(name).await? {
                    PackageStatus::InvalidOwners => {
                        // bail!("Package {name} has invalid owners!")
                    }
                    PackageStatus::NotPublished => is_published = false,
                    PackageStatus::ValidOwners => is_published = true,
                }
            }

            if verify && crate::verify(name, &package_version, self.simulator.as_ref()).await? {
                println!("Package {name}@{package_version} already published!");
                is_actualized = true;
            }

            let manifest = Manifest::new(pkg, is_published, is_actualized)?;
            self.graph.push(manifest);
        }

        workspace.complete(
            self.index.clone(),
            &package_versions,
            self.simulator.is_some(),
        )?;

        self.workspace = Some(workspace);

        self.patch()?;

        Ok(self)
    }

    /// Restore local files
    pub fn restore(&self) -> Result<()> {
        self.manifests()
            .map(|manifest| manifest.restore())
            .collect::<Result<Vec<_>>>()?;

        if let Some(workspace) = self.workspace.as_ref() {
            workspace.lock_file().restore()?;
        }

        if let Some(simulator) = self.simulator.as_ref() {
            simulator.restore()?;
        }

        Ok(())
    }

    /// Patch local files
    fn patch(&self) -> Result<()> {
        self.manifests()
            .map(|manifest| manifest.patch())
            .collect::<Result<Vec<_>>>()?;

        if let Some(simulator) = self.simulator.as_ref() {
            simulator.patch()?;
        }

        Ok(())
    }

    /// Returns an iterator of manifests that have been potentially patched
    fn manifests(&self) -> impl Iterator<Item = &Manifest> {
        self.graph.iter().chain(self.workspace.as_deref())
    }

    /// Check the to-be-published packages
    pub fn check(&self) -> Result<()> {
        // Post tests for gtest
        for (pkg, test) in [
            ("demo-syscall-error", "program_can_be_initialized"),
            ("gsdk", "timeout"),
        ] {
            if !crate::test(pkg, test)?.success() {
                bail!("{pkg}:{test} failed to pass the test ...");
            }
        }

        Ok(())
    }

    /// Apply publish-only workspace dependency rewrites.
    pub fn prepare_publish(&mut self) -> Result<()> {
        for manifest in self.graph.iter_mut() {
            handler::patch_publish(&manifest.name, &mut manifest.mutable_manifest);
        }

        if let Some(workspace) = self.workspace.as_mut() {
            workspace.rename()?;
        }

        self.patch()
    }

    /// Publish packages
    pub fn publish(&self) -> Result<()> {
        for Manifest {
            name,
            path,
            is_published,
            ..
        } in self.graph.iter().filter(|m| !m.is_actualized)
        {
            println!("Publishing {path:?}");
            if let Some(simulator) = self.simulator.as_ref() {
                simulator.clear_cache()?;
            }
            let status = crate::publish(&path.to_string_lossy())?;
            if !status.success() {
                bail!("Failed to publish package {path:?} ...");
            }

            if self.simulator.is_none() && !is_published {
                let status = crate::add_owner(handler::crates_io_name(name), TEAM_OWNER)?;
                if !status.success() {
                    bail!("Failed to add owner to package {name} ...");
                }
            }

            // TODO: impl here rate-limit check for crates.io
            if self.simulator.is_none() {}
        }

        Ok(())
    }
}
