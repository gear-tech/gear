// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Packages publisher.

use crate::{
    CRATES_IO_ALLOWED_CATEGORIES, EXPECTED_OWNERS, GEAR_SUBSTRATE_DEPENDENCIES, Manifest, PACKAGES,
    PackageStatus, SAFE_DEPENDENCIES, STACKED_DEPENDENCIES, Simulator, Workspace, handler,
};
use anyhow::{Result, bail};
use cargo_metadata::{Error as MetadataError, Metadata, MetadataCommand, Result as MetadataResult};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
use tokio::{process::Command, time};

trait MetadataCommandExt {
    async fn exec_async(&self) -> MetadataResult<Metadata>;
}

impl MetadataCommandExt for MetadataCommand {
    /// Executes the command and returns the metadata.
    /// This is an async version of[`MetadataCommand::exec`].
    async fn exec_async(&self) -> MetadataResult<Metadata> {
        let output = Command::from(self.cargo_command()).output().await?;
        if !output.status.success() {
            return Err(MetadataError::CargoMetadata {
                stderr: String::from_utf8(output.stderr)?,
            });
        }
        let stdout = str::from_utf8(&output.stdout)?
            .lines()
            .find(|line| line.starts_with('{'))
            .ok_or(MetadataError::NoJson)?;
        Self::parse(stdout)
    }
}

#[derive(Debug, Clone, Copy)]
struct PublishRateLimit {
    burst: usize,
    refill_interval: Duration,
}

impl PublishRateLimit {
    const CRATES_IO_RATE_LIMIT_SAFETY_DELAY: Duration = Duration::from_secs(30);

    const fn new(burst: usize, refill_interval: Duration) -> Self {
        Self {
            burst,
            refill_interval,
        }
    }

    fn wait_interval(self) -> Duration {
        self.refill_interval + Self::CRATES_IO_RATE_LIMIT_SAFETY_DELAY
    }
}

#[derive(Debug)]
struct PublishRateLimitCounter {
    limit: PublishRateLimit,
    publishes: usize,
}

impl PublishRateLimitCounter {
    fn new(limit: PublishRateLimit) -> Self {
        Self {
            limit,
            publishes: 0,
        }
    }

    async fn satisfy(&mut self, package: &str, action: &str) {
        if self.publishes >= self.limit.burst {
            let delay = self.limit.wait_interval();
            println!("Waiting {delay:?} before publishing {package} ({action} rate limit) ...");
            time::sleep(delay).await;
        }

        self.publishes += 1;
    }
}

#[derive(Debug)]
struct CratesIoRateLimiter {
    new_crates: PublishRateLimitCounter,
    new_versions: PublishRateLimitCounter,
}

impl CratesIoRateLimiter {
    const NEW_CRATES_RATE_LIMIT: PublishRateLimit =
        PublishRateLimit::new(5, Duration::from_mins(10));
    const NEW_VERSIONS_RATE_LIMIT: PublishRateLimit =
        PublishRateLimit::new(30, Duration::from_mins(1));

    fn new() -> Self {
        Self {
            new_crates: PublishRateLimitCounter::new(Self::NEW_CRATES_RATE_LIMIT),
            new_versions: PublishRateLimitCounter::new(Self::NEW_VERSIONS_RATE_LIMIT),
        }
    }

    async fn satisfy(&mut self, package: &str, is_published: bool) {
        if is_published {
            self.new_versions.satisfy(package, "new version").await;
        } else {
            self.new_crates.satisfy(package, "new crate").await;
        }
    }
}

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
    pub async fn new() -> Result<Self> {
        Self::with_simulation(false, None).await
    }

    /// Create a new publisher with simulation.
    pub async fn with_simulation(simulate: bool, registry_path: Option<PathBuf>) -> Result<Self> {
        Ok(Self {
            metadata: MetadataCommand::new().no_deps().exec_async().await?,
            graph: vec![],
            index: [
                GEAR_SUBSTRATE_DEPENDENCIES,
                SAFE_DEPENDENCIES,
                STACKED_DEPENDENCIES,
                PACKAGES,
            ]
            .concat(),
            workspace: None,
            simulator: if simulate {
                Some(Simulator::new(registry_path).await?)
            } else {
                None
            },
        })
    }

    /// Build package graphs
    ///
    /// 1. Replace git dependencies to crates-io dependencies.
    /// 2. Rename version of all local packages
    /// 3. Patch dependencies if needed
    pub async fn build(mut self, verify: bool, version: Option<String>) -> Result<Self> {
        let mut workspace = Workspace::lookup(version).await?;
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
                if !CRATES_IO_ALLOWED_CATEGORIES.contains(&category.as_str()) {
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

            let crates_io_name = handler::crates_io_name(name);

            if verify {
                match crate::verify_owners(crates_io_name).await? {
                    PackageStatus::InvalidOwners => {
                        bail!("Package {crates_io_name} has invalid owners!")
                    }
                    PackageStatus::NotPublished => is_published = false,
                    PackageStatus::ValidOwners => is_published = true,
                }
            }

            if verify && crate::verify(name, &package_version, self.simulator.as_ref()).await? {
                println!("Package {name}@{package_version} already published!");
                is_actualized = true;
            }

            let manifest = Manifest::new(pkg, is_published, is_actualized).await?;
            self.graph.push(manifest);
        }

        workspace.complete(
            self.index.clone(),
            &package_versions,
            self.simulator.is_some(),
        )?;

        self.workspace = Some(workspace);

        self.patch().await?;

        Ok(self)
    }

    /// Restore local files
    pub async fn restore(&self) -> Result<()> {
        for manifest in self.manifests() {
            manifest.restore().await?;
        }

        if let Some(workspace) = self.workspace.as_ref() {
            workspace.lock_file().restore().await?;
        }

        if let Some(simulator) = self.simulator.as_ref() {
            simulator.restore().await?;
        }

        Ok(())
    }

    /// Patch local files
    async fn patch(&self) -> Result<()> {
        for manifest in self.manifests() {
            manifest.patch().await?;
        }

        if let Some(simulator) = self.simulator.as_ref() {
            simulator.patch().await?;
        }

        Ok(())
    }

    /// Returns an iterator of manifests that have been potentially patched
    fn manifests(&self) -> impl Iterator<Item = &Manifest> {
        self.graph.iter().chain(self.workspace.as_deref())
    }

    /// Check the to-be-published packages
    pub async fn check(&self) -> Result<()> {
        // Post tests for gtest
        for (pkg, test) in [
            ("demo-syscall-error", "program_can_be_initialized"),
            ("gsdk", "timeout"),
        ] {
            let status = crate::test(pkg, test).await?;
            if !status.success() {
                bail!("{pkg}:{test} failed to pass the test ...");
            }
        }

        Ok(())
    }

    /// Apply publish-only workspace dependency rewrites.
    pub async fn prepare_publish(&mut self) -> Result<()> {
        for manifest in self.graph.iter_mut() {
            handler::patch_publish(&manifest.name, &mut manifest.mutable_manifest);
        }

        if let Some(workspace) = self.workspace.as_mut() {
            workspace.rename()?;
        }

        self.patch().await
    }

    /// Publish packages
    pub async fn publish(&self) -> Result<()> {
        let mut rate_limiter = CratesIoRateLimiter::new();

        for Manifest {
            name,
            path,
            is_published,
            ..
        } in self.graph.iter().filter(|m| !m.is_actualized)
        {
            println!("Publishing {path:?}");
            if let Some(simulator) = self.simulator.as_ref() {
                simulator.clear_cache().await?;
            }

            if self.simulator.is_none() {
                rate_limiter.satisfy(name, *is_published).await;
            }

            let status = crate::publish(&path.to_string_lossy()).await?;
            if !status.success() {
                bail!("Failed to publish package {path:?} ...");
            }

            if self.simulator.is_none() && !is_published {
                let crates_io_name = handler::crates_io_name(name);
                for owner in EXPECTED_OWNERS {
                    let status = crate::add_owner(crates_io_name, owner).await?;
                    if !status.success() {
                        bail!("Failed to add owner {owner} to package {crates_io_name} ...");
                    }
                }
            }
        }

        Ok(())
    }
}
