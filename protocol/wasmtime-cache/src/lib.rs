// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Wasmtime module cache.

use lru::LruCache;
use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hasher},
    num::NonZeroUsize,
    sync::{Condvar, Mutex, OnceLock},
};
use wasmtime::{Engine, Module, error::Context};

const MODULES_CACHE_CAPACITY: NonZeroUsize = NonZeroUsize::new(1024).unwrap();

struct Cache {
    state: Mutex<CacheState>,
    module_ready: Condvar,
}

struct CacheState {
    modules: LruCache<u64, Module>,
    compiling: HashSet<u64>,
}

impl Cache {
    fn new() -> Self {
        Self {
            state: Mutex::new(CacheState {
                modules: LruCache::new(MODULES_CACHE_CAPACITY),
                compiling: HashSet::new(),
            }),
            module_ready: Condvar::new(),
        }
    }
}

struct CompilePermit {
    hash: u64,
}

impl Drop for CompilePermit {
    fn drop(&mut self) {
        let cache = cache();

        {
            let mut state = cache.state.lock().unwrap();
            debug_assert!(state.compiling.remove(&self.hash));
        }

        cache.module_ready.notify_all();
    }
}

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(Cache::new)
}

fn hash(code: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(code);
    hasher.finish()
}

enum ModuleFrom {
    Lru(Module),
    EngineChanged(Module),
    New(Module),
}

fn cached_module(
    state: &mut CacheState,
    engine: &Engine,
    hash: u64,
) -> wasmtime::Result<Option<ModuleFrom>> {
    if let Some(module) = state.modules.get(&hash) {
        tracing::trace!("load wasmtime module from LRU cache");

        if Engine::same(module.engine(), engine) {
            Ok(Some(ModuleFrom::Lru(module.clone())))
        } else {
            tracing::trace!("reserialize module because of changed engine");
            let module = module.serialize().context("failed to serialize module")?;
            let module = unsafe {
                Module::deserialize(engine, &module).context("failed to deserialize module")?
            };
            let old_module = state.modules.put(hash, module.clone());
            debug_assert!(old_module.is_some());
            Ok(Some(ModuleFrom::EngineChanged(module)))
        }
    } else {
        Ok(None)
    }
}

fn reserve_compile(
    hash: u64,
    engine: &Engine,
) -> wasmtime::Result<Result<CompilePermit, ModuleFrom>> {
    let cache = cache();
    let mut state = cache.state.lock().unwrap();

    loop {
        if let Some(module) = cached_module(&mut state, engine, hash)? {
            return Ok(Err(module));
        }

        if state.compiling.insert(hash) {
            return Ok(Ok(CompilePermit { hash }));
        }

        state = cache.module_ready.wait(state).unwrap();
    }
}

fn get_impl(engine: &Engine, code: &[u8]) -> wasmtime::Result<ModuleFrom> {
    let hash = hash(code);

    let compile_permit = match reserve_compile(hash, engine)? {
        Ok(compile_permit) => compile_permit,
        Err(module) => return Ok(module),
    };

    tracing::trace!("create wasmtime module because of missed LRU cache");
    let module = Module::new(engine, code).context("failed to create module")?;

    {
        let mut state = cache().state.lock().unwrap();
        let old_module = state.modules.put(hash, module.clone());
        debug_assert!(old_module.is_none());
    }

    drop(compile_permit);
    Ok(ModuleFrom::New(module))
}

/// Returns a compiled Wasmtime module, using an in-memory LRU cache on hits.
pub fn get(engine: &Engine, code: &[u8]) -> wasmtime::Result<Module> {
    match get_impl(engine, code)? {
        ModuleFrom::Lru(module) | ModuleFrom::EngineChanged(module) | ModuleFrom::New(module) => {
            Ok(module)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, Barrier},
        thread,
    };

    const EMPTY_WASM: &[u8] = b"\x00asm\x01\x00\x00\x00";
    const CUSTOM_WASM: &[u8] = b"\x00asm\x01\x00\x00\x00\x00\x04\x03foo";
    const CUSTOM_WASM_A: &[u8] = b"\x00asm\x01\x00\x00\x00\x00\x06\x05codeA";
    const CUSTOM_WASM_B: &[u8] = b"\x00asm\x01\x00\x00\x00\x00\x06\x05codeB";

    enum Source {
        New,
        Lru,
        EngineChanged,
    }

    #[test]
    fn smoke() {
        let engine = Engine::default();

        let module = get_impl(&engine, EMPTY_WASM).expect("module compiles");
        assert!(matches!(module, ModuleFrom::New(_)));

        let module = get_impl(&engine, EMPTY_WASM).expect("module loads from cache");
        assert!(matches!(module, ModuleFrom::Lru(_)));

        let module = get_impl(&Engine::default(), EMPTY_WASM).expect("module loads from cache");
        assert!(matches!(module, ModuleFrom::EngineChanged(_)));
    }

    #[test]
    fn concurrent_miss_compiles_once() {
        const THREADS: usize = 8;

        let engine = Engine::default();
        let barrier = Arc::new(Barrier::new(THREADS));

        let handles = (0..THREADS)
            .map(|_| {
                let engine = engine.clone();
                let barrier = barrier.clone();

                thread::spawn(move || {
                    barrier.wait();

                    match get_impl(&engine, CUSTOM_WASM).expect("module loads") {
                        ModuleFrom::New(_) => Source::New,
                        ModuleFrom::Lru(_) => Source::Lru,
                        ModuleFrom::EngineChanged(_) => Source::EngineChanged,
                    }
                })
            })
            .collect::<Vec<_>>();

        let mut new = 0;
        let mut lru = 0;
        for handle in handles {
            match handle.join().expect("thread does not panic") {
                Source::New => new += 1,
                Source::Lru => lru += 1,
                Source::EngineChanged => panic!("engine should not change"),
            }
        }

        assert_eq!(new, 1);
        assert_eq!(lru, THREADS - 1);
    }

    #[test]
    fn two_concurrent_misses_per_code_compile_once_each() {
        const THREADS: usize = 4;

        let engine = Engine::default();
        let barrier = Arc::new(Barrier::new(THREADS));
        let code_by_thread = [
            (0, CUSTOM_WASM_A),
            (0, CUSTOM_WASM_A),
            (1, CUSTOM_WASM_B),
            (1, CUSTOM_WASM_B),
        ];

        let handles = code_by_thread
            .into_iter()
            .map(|(code_index, code)| {
                let engine = engine.clone();
                let barrier = barrier.clone();

                thread::spawn(move || {
                    barrier.wait();

                    let source = match get_impl(&engine, code).expect("module loads") {
                        ModuleFrom::New(_) => Source::New,
                        ModuleFrom::Lru(_) => Source::Lru,
                        ModuleFrom::EngineChanged(_) => Source::EngineChanged,
                    };

                    (code_index, source)
                })
            })
            .collect::<Vec<_>>();

        let mut counts = [(0, 0), (0, 0)];
        for handle in handles {
            let (code_index, source) = handle.join().expect("thread does not panic");

            match source {
                Source::New => counts[code_index].0 += 1,
                Source::Lru => counts[code_index].1 += 1,
                Source::EngineChanged => panic!("engine should not change"),
            }
        }

        assert_eq!(counts, [(1, 1), (1, 1)]);
    }
}
