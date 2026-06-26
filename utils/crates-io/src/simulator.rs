// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Packages publishing simulator.

use crate::{CARGO_REGISTRY_NAME, Workspace};
use anyhow::Result;
use std::{
    env,
    net::SocketAddr,
    ops::Deref,
    path::{Path, PathBuf},
};
use tempfile::TempDir;
use tokio::{
    fs,
    task::{self, JoinHandle},
};
use toml_edit::DocumentMut;

enum RegistryPath {
    Dir(PathBuf),
    TempDir(TempDir),
}

impl Deref for RegistryPath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        match self {
            RegistryPath::Dir(path) => path,
            RegistryPath::TempDir(temp_dir) => temp_dir.path(),
        }
    }
}

/// crates-io packages publishing simulator.
#[allow(dead_code)]
pub struct Simulator {
    path: RegistryPath,
    addr: SocketAddr,
    handle: JoinHandle<()>,
    config_path: PathBuf,
    original_config: DocumentMut,
    mutable_config: DocumentMut,
}

impl Simulator {
    /// Create a new simulator.
    pub async fn new(registry_path: Option<PathBuf>) -> Result<Self> {
        let path = match registry_path {
            Some(path) => RegistryPath::Dir(path),
            None => RegistryPath::TempDir(TempDir::new()?),
        };
        let (future, addr) = cargo_http_registry::serve(&path, "127.0.0.1:35503".parse()?)?;
        let handle = task::spawn(future);

        let config_path = Workspace::resolve_path(".cargo/config.toml").await?;
        let original_config: DocumentMut = fs::read_to_string(&config_path).await?.parse()?;
        let mut mutable_config = original_config.clone();

        // Patch `.cargo/config.toml` according to https://github.com/d-e-s-o/cargo-http-registry/blob/main/README.md#usage
        mutable_config["registry"]["default"] = toml_edit::value(CARGO_REGISTRY_NAME);
        mutable_config["registries"][CARGO_REGISTRY_NAME]["index"] =
            toml_edit::value(format!("http://{addr}/git"));
        mutable_config["registries"][CARGO_REGISTRY_NAME]["token"] = toml_edit::value("token");
        mutable_config["net"]["git-fetch-with-cli"] = toml_edit::value(true);

        Ok(Self {
            path,
            addr,
            handle,
            config_path,
            original_config,
            mutable_config,
        })
    }

    /// Returns socket addr
    pub fn addr(&self) -> &SocketAddr {
        &self.addr
    }

    /// Restore cargo config
    pub async fn restore(&self) -> Result<()> {
        fs::write(&self.config_path, self.original_config.to_string())
            .await
            .map_err(Into::into)
    }

    /// Patch cargo config
    pub async fn patch(&self) -> Result<()> {
        self.clear_cache().await?;
        fs::write(&self.config_path, self.mutable_config.to_string())
            .await
            .map_err(Into::into)
    }

    /// Clear Cargo cache entries that can retain stale simulated packages.
    pub async fn clear_cache(&self) -> Result<()> {
        clear_local_registry_cache().await?;
        clear_target_package_dir().await
    }
}

async fn clear_local_registry_cache() -> Result<()> {
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")));
    let Some(cargo_home) = cargo_home else {
        return Ok(());
    };

    let registry = cargo_home.join("registry");
    clear_registry_dir(&registry.join("src")).await?;
    clear_registry_dir(&registry.join("cache")).await
}

async fn clear_registry_dir(path: &Path) -> Result<()> {
    let Ok(mut entries) = fs::read_dir(path).await else {
        return Ok(());
    };

    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        if file_name.to_string_lossy().starts_with("127.0.0.1-") {
            let path = entry.path();
            if entry.file_type().await?.is_dir() {
                fs::remove_dir_all(path).await?;
            } else {
                fs::remove_file(path).await?;
            }
        }
    }

    Ok(())
}

async fn clear_target_package_dir() -> Result<()> {
    let manifest = Workspace::resolve_path("Cargo.toml").await?;
    let Some(workspace_dir) = manifest.parent() else {
        return Ok(());
    };

    let package_dir = workspace_dir.join("target").join("package");
    if fs::try_exists(&package_dir).await? {
        fs::remove_dir_all(package_dir).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn clear_registry_dir_removes_local_registry_dirs_and_files() {
        let temp_dir = TempDir::new().unwrap();
        let local_dir = temp_dir.path().join("127.0.0.1-abc");
        let local_file = temp_dir.path().join("127.0.0.1-def");
        let upstream_dir = temp_dir.path().join("github.com-abc");

        fs::create_dir(&local_dir).await.unwrap();
        fs::write(&local_file, b"crate").await.unwrap();
        fs::create_dir(&upstream_dir).await.unwrap();

        clear_registry_dir(temp_dir.path()).await.unwrap();

        assert!(!fs::try_exists(local_dir).await.unwrap());
        assert!(!fs::try_exists(local_file).await.unwrap());
        assert!(fs::try_exists(upstream_dir).await.unwrap());
    }
}
