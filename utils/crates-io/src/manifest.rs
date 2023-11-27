//! Manifest utils for crates-io-manager

use anyhow::Result;
use cargo_toml::{Manifest, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Cargo manifest with path
pub struct ManifestWithPath {
    /// Crate name
    pub name: String,
    /// Cargo manifest
    pub manifest: Manifest,
    /// Path of the manifest
    pub path: PathBuf,
}

impl ManifestWithPath {
    /// Get the worksapce manifest
    pub fn workspace() -> Result<Self> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../Cargo.toml")
            .canonicalize()?;

        Ok(Self {
            name: "__gear_workspace".into(),
            manifest: Manifest::from_path(&path)?,
            path,
        })
    }

    /// Complete the manifest of the specified crate from
    /// the current manifest
    pub fn manifest(&self, path: impl AsRef<Path>) -> Result<Self> {
        let mut manifest = Manifest::<Value>::from_slice_with_metadata(&fs::read(&path)?)?;
        manifest
            .complete_from_path_and_workspace(path.as_ref(), Some((&self.manifest, &self.path)))?;

        Ok(Self {
            name: manifest.package.clone().map(|p| p.name).unwrap_or_default(),
            manifest,
            path: path.as_ref().to_path_buf(),
        })
    }
}
