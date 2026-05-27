// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Wasmtime module cache.

use lru::LruCache;
use std::{
    hash::{DefaultHasher, Hasher},
    num::NonZeroUsize,
    sync::{Mutex, OnceLock},
};
use wasmtime::{Engine, Module};

const MODULES_CACHE_CAPACITY: NonZeroUsize = NonZeroUsize::new(1024).unwrap();

type CachedModules = Mutex<LruCache<u64, Vec<u8>>>;

fn modules() -> &'static CachedModules {
    static MODULES: OnceLock<CachedModules> = OnceLock::new();
    MODULES.get_or_init(|| Mutex::new(LruCache::new(MODULES_CACHE_CAPACITY)))
}

fn cache_key(code: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(code);
    hasher.finish()
}

enum ModuleFrom {
    Lru(Module),
    Compilation(Module),
    Recompilation(Module),
}

fn get_impl(engine: &Engine, code: &[u8]) -> wasmtime::Result<ModuleFrom> {
    let key = cache_key(code);
    let mut modules = modules().lock().expect("failed to lock mutex");

    if let Some(serialized_module) = modules.get(&key) {
        tracing::trace!("load wasmtime module from LRU cache");

        // SAFETY: the cache stores only modules serialized by this crate in the
        // same process. Wasmtime validates that serialized artifacts are
        // compatible with the engine before returning a module.
        match unsafe { Module::deserialize(engine, serialized_module) } {
            Ok(module) => return Ok(ModuleFrom::Lru(module)),
            Err(error) => {
                tracing::trace!(
                    "recompile wasmtime module because LRU cache is incompatible: {error}"
                );
            }
        }
    }

    tracing::trace!("compile wasmtime module because of missed LRU cache");
    let module = Module::new(engine, code)?;
    let serialized_module = module.serialize()?;
    let module_from = if modules.put(key, serialized_module).is_some() {
        ModuleFrom::Recompilation(module)
    } else {
        ModuleFrom::Compilation(module)
    };

    Ok(module_from)
}

/// Returns a compiled Wasmtime module, using an in-memory LRU cache on hits.
pub fn get(engine: &Engine, code: &[u8]) -> wasmtime::Result<Module> {
    match get_impl(engine, code)? {
        ModuleFrom::Lru(module)
        | ModuleFrom::Compilation(module)
        | ModuleFrom::Recompilation(module) => Ok(module),
    }
}

/// Clears the process-local module cache.
///
/// Intended for benchmarks and tests that need a deterministic cache state.
#[doc(hidden)]
pub fn clear() {
    modules().lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY_WASM: &[u8] = b"\0asm";

    #[test]
    fn caches_serialized_modules_in_lru() {
        let engine = Engine::default();

        clear();

        let module = get_impl(&engine, EMPTY_WASM).expect("module compiles");
        assert!(matches!(module, ModuleFrom::Compilation(_)));

        let module = get_impl(&engine, EMPTY_WASM).expect("module loads from cache");
        assert!(matches!(module, ModuleFrom::Lru(_)));
    }
}
