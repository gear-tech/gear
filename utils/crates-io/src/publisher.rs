//! Packages publisher

use crate::{rename, ManifestWithPath, PACKAGES, SAFE_DEPENDENCIES, STACKED_DEPENDENCIES};
use anyhow::Result;
use cargo_metadata::{Metadata, MetadataCommand};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
};

/// crates-io packages publisher.
pub struct Publisher {
    metadata: Metadata,
    graph: BTreeMap<Option<usize>, ManifestWithPath>,
    index: HashMap<String, usize>,
}

impl Publisher {
    /// Create a new publisher.
    pub fn new() -> Result<Self> {
        let metadata = MetadataCommand::new().no_deps().exec()?;
        let graph = BTreeMap::new();
        let index = HashMap::<String, usize>::from_iter(
            [
                SAFE_DEPENDENCIES.to_vec(),
                STACKED_DEPENDENCIES.into(),
                PACKAGES.into(),
            ]
            .concat()
            .into_iter()
            .enumerate()
            .map(|(i, p)| (p.into(), i)),
        );

        Ok(Self {
            metadata,
            graph,
            index,
        })
    }

    /// Build package graphs
    pub fn build(mut self) -> Result<Self> {
        let workspace = ManifestWithPath::workspace()?;
        for p in self.metadata.packages.iter() {
            if !self.index.contains_key(&p.name) {
                continue;
            }

            let version = p.version.to_string();
            println!("Verifying {}@{} ...", &p.name, &version);
            if crate::verify(&p.name, &version)? {
                println!("Package {}@{} already published .", &p.name, &version);
                continue;
            }

            let mut manifest = workspace.manifest(&p.manifest_path)?;
            rename::package(p, &mut manifest.manifest)?;
            rename::deps(
                &mut manifest.manifest,
                self.index.keys().collect(),
                version.to_string(),
            )?;

            self.graph
                .insert(self.index.get(&p.name).cloned(), manifest);
        }

        Ok(self)
    }

    /// Check the to-be-published packages
    pub fn check(&self) -> Result<()> {
        self.flush()?;

        for ManifestWithPath { path, name, .. } in self.graph.values() {
            if !PACKAGES.contains(&name.as_str()) {
                continue;
            }

            println!("Checking {path:?}");
            let status = crate::check(&path.to_string_lossy())?;
            if !status.success() {
                panic!("Package {path:?} didn't pass the check .");
            }
        }

        Ok(())
    }

    /// Publish packages
    pub fn publish(&self) -> Result<()> {
        self.flush()?;

        for ManifestWithPath { path, .. } in self.graph.values() {
            println!("Publishing {path:?}");
            let status = crate::publish(&path.to_string_lossy())?;
            if !status.success() {
                panic!("Failed to publish package {path:?} ...");
            }
        }

        Ok(())
    }

    /// Flush new manifests to disk
    fn flush(&self) -> Result<()> {
        for ManifestWithPath { path, manifest, .. } in self.graph.values() {
            fs::write(path, toml::to_string_pretty(&manifest)?)?;
        }

        Ok(())
    }
}
