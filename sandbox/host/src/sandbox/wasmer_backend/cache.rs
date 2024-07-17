// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Wasmer's module caches

use std::{
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use sandbox_wasmer::Module;
use tempfile::TempDir;
use uluru::LRUCache;
use wasmer_cache::Hash;

use CachedModuleErr::*;

pub enum CachedModuleErr {
    FileSystemErr,
    CacheMissErr(FileSystemCache, Hash),
}

// CachedModules holds a mutex-protected LRU cache of compiled wasm modules.
// This allows for efficient reuse of modules across invocations.
type CachedModules = Mutex<LRUCache<CachedModule, 1024>>;

struct CachedModule {
    wasm: Vec<u8>,
    // Serialized module (Wasmer's custom binary format)
    serialized_module: Vec<u8>,
}

// The cached_modules function provides thread-safe access to the CACHED_MODULES static.
fn lru_cache() -> &'static CachedModules {
    static CACHED_MODULES: OnceLock<CachedModules> = OnceLock::new();
    CACHED_MODULES.get_or_init(|| Mutex::new(LRUCache::default()))
}

fn fs_cache() -> Result<FileSystemCache, CachedModuleErr> {
    static CACHE_DIR: OnceLock<TempDir> = OnceLock::new();

    // Try to load from tempfile cache
    let cache_path = CACHE_DIR
        .get_or_init(|| {
            tempfile::tempdir().expect("Cannot create temporary directory for wasmer caches")
        })
        .path();
    log::trace!("Wasmer sandbox cache dir is: {cache_path:?}");

    FileSystemCache::new(cache_path).map_err(|_| FileSystemErr)
}

pub fn get_cached_module(
    wasm: &[u8],
    store: &sandbox_wasmer::Store,
) -> Result<Module, CachedModuleErr> {
    let mut lru_lock = lru_cache().lock().expect("CACHED_MODULES lock fail");

    let maybe_module = lru_lock.find(|x| x.wasm == wasm);

    // Try to load from LRU cache first
    if let Some(CachedModule {
        serialized_module, ..
    }) = maybe_module
    {
        // SAFETY: Module inside LRU cache cannot be corrupted.
        let module = unsafe {
            Module::deserialize_unchecked(store, serialized_module.as_slice())
                .expect("module in LRU cache is valid")
        };
        Ok(module)
    } else {
        let code_hash = Hash::generate(wasm);

        let fs_cache = fs_cache()?;
        let serialized_module = fs_cache
            .load(code_hash)
            .map_err(|_| CacheMissErr(fs_cache.clone(), code_hash))?;

        lru_lock.insert(CachedModule {
            wasm: wasm.to_vec(),
            serialized_module: serialized_module.clone(),
        });

        // SAFETY: We trust the module in FS cache.
        let module = unsafe {
            Module::deserialize(store, serialized_module).map_err(|_| {
                // Module in FS cache is corrupted, remove it.
                fs_cache.remove_key(code_hash);
                CacheMissErr(fs_cache, code_hash)
            })?
        };

        Ok(module)
    }
}

pub fn try_to_store_module_in_cache(
    mut fs_cache: FileSystemCache,
    code_hash: Hash,
    wasm: &[u8],
    module: &Module,
) {
    // NOTE: `From<Bytes> to Vec<u8>` is zero cost.
    let serialized_module: Vec<_> = module
        .serialize()
        .expect("module should be serializable")
        .into();

    // Store module in LRU cache
    let _ = lru_cache()
        .lock()
        .expect("CACHED_MODULES lock fail")
        .insert(CachedModule {
            wasm: wasm.to_vec(),
            serialized_module: serialized_module.clone(),
        });
    log::trace!("Store module in LRU cache");

    let res = fs_cache.store(code_hash, &serialized_module);
    log::trace!("Store module in FS cache with result: {:?}", res);
}

/// Altered copy of the `FileSystemCache` struct from `wasmer_cache` crate.
#[derive(Debug, Clone)]
pub struct FileSystemCache {
    path: PathBuf,
    ext: Option<String>,
}

impl FileSystemCache {
    /// Construct a new `FileSystemCache` around the specified directory.
    fn new<P: Into<PathBuf>>(path: P) -> io::Result<Self> {
        let path: PathBuf = path.into();
        if path.exists() {
            let metadata = path.metadata()?;
            if metadata.is_dir() {
                if !metadata.permissions().readonly() {
                    Ok(Self { path, ext: None })
                } else {
                    // This directory is readonly.
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        format!("the supplied path is readonly: {}", path.display()),
                    ))
                }
            } else {
                // This path points to a file.
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "the supplied path already points to a file: {}",
                        path.display()
                    ),
                ))
            }
        } else {
            // Create the directory and any parent directories if they don't yet exist.
            let res = fs::create_dir_all(&path);
            if res.is_err() {
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("failed to create cache directory: {}", path.display()),
                ))
            } else {
                Ok(Self { path, ext: None })
            }
        }
    }

    /// Load the serialized module from the cache.
    fn load(&self, key: Hash) -> Result<Vec<u8>, io::Error> {
        let filename = if let Some(ref ext) = self.ext {
            format!("{}.{}", key, ext)
        } else {
            key.to_string()
        };
        let path = self.path.join(filename);

        fs::read(path)
    }

    /// If an error occurs while deserializing then we can not trust it anymore
    /// so delete the cache file
    fn remove_key(&self, key: Hash) {
        let filename = Self::compose_filename(self.ext.as_deref(), key);
        let path = self.path.join(filename);

        let res = fs::remove_file(path);
        log::trace!("Remove module from FS cache with result: {:?}", res);
    }

    /// Store the serialized module in the cache.
    fn store(&mut self, key: Hash, serialized_module: &[u8]) -> Result<(), io::Error> {
        let filename = Self::compose_filename(self.ext.as_deref(), key);
        let path = self.path.join(filename);
        let mut file = File::create(path)?;

        file.write_all(serialized_module)?;

        Ok(())
    }

    fn compose_filename(extension: Option<&str>, key: Hash) -> PathBuf {
        match extension {
            Some(extension) => format!("{key}.{extension}"),
            None => key.to_string(),
        }
        .into()
    }
}
