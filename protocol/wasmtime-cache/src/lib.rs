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

type CachedModules = Mutex<LruCache<u64, Module>>;

fn modules() -> &'static CachedModules {
    static MODULES: OnceLock<CachedModules> = OnceLock::new();
    MODULES.get_or_init(|| Mutex::new(LruCache::new(MODULES_CACHE_CAPACITY)))
}

fn hash(code: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(code);
    hasher.finish()
}

enum ModuleFrom {
    Lru(Module),
    New(Module),
}

fn get_impl(engine: &Engine, code: &[u8]) -> wasmtime::Result<ModuleFrom> {
    let hash = hash(code);
    let mut modules = modules().lock().expect("failed to lock mutex");

    if let Some(module) = modules.get(&hash) {
        tracing::trace!("load wasmtime module from LRU cache");
        return Ok(ModuleFrom::Lru(module.clone()));
    }

    tracing::trace!("create wasmtime module because of missed LRU cache");
    let module = Module::new(engine, code)?;
    let old_module = modules.put(hash, module.clone());
    debug_assert!(old_module.is_none());

    Ok(ModuleFrom::New(module))
}

/// Returns a compiled Wasmtime module, using an in-memory LRU cache on hits.
pub fn get(engine: &Engine, code: &[u8]) -> wasmtime::Result<Module> {
    match get_impl(engine, code)? {
        ModuleFrom::Lru(module) | ModuleFrom::New(module) => Ok(module),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY_WASM: &[u8] = b"\x00asm\x01\x00\x00\x00";

    #[test]
    fn smoke() {
        let engine = Engine::default();

        let module = get_impl(&engine, EMPTY_WASM).expect("module compiles");
        assert!(matches!(module, ModuleFrom::New(_)));

        let module = get_impl(&engine, EMPTY_WASM).expect("module loads from cache");
        assert!(matches!(module, ModuleFrom::Lru(_)));
    }
}
