//! package publisher

use crate::{rename, ManifestWithPath, PACKAGES};
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
            PACKAGES.into_iter().enumerate().map(|(i, p)| (p.into(), i)),
        );

        Ok(Self {
            metadata,
            graph,
            index,
        })
    }

    /// Build package graphs
    pub fn build(mut self) -> Result<Self> {
        // Build package graph.
        let workspace = ManifestWithPath::workspace()?;
        for p in self.metadata.packages.iter() {
            if !self.index.contains_key(&p.name) {
                continue;
            }

            let version = p.version.to_string();
            if crate::verify(&p.name, &version)? {
                println!("Package {}@{} already published.", &p.name, &version);
                continue;
            }

            let mut manifest = workspace.manifest(&p.manifest_path)?;
            rename::package(&p, &mut manifest.manifest)?;
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

    /// publish packages
    pub fn publish(&self) -> Result<()> {
        for ManifestWithPath { path, manifest } in self.graph.values() {
            println!("Publishing {:?}", path);
            fs::write(path, toml::to_string_pretty(&manifest)?)?;

            let path = path.to_string_lossy();
            let status = crate::publish(&path)?;
            if !status.success() {
                panic!("Failed to publish package {path}...");
            }
        }

        Ok(())
    }
}
