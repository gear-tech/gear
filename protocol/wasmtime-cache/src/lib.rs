// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Wasmtime module cache.
//!
//! The cache uses a per-code "single flight" protocol. The first thread that
//! misses the LRU for a code hash records that hash in `compiling`, drops the
//! lock, and compiles the module. Threads requesting the same hash wait on a
//! condition variable, while threads requesting other hashes can reserve their
//! own compile slots and proceed independently.
//!
//! A `CompilePermit` represents ownership of one in-progress compile. Dropping
//! it always removes the hash from `compiling` and wakes waiters, so both
//! successful compilation and early errors unblock the next thread.

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
    // Hashes currently being compiled outside the mutex. A hash is present here
    // only while its owner holds a `CompilePermit`.
    compiling: HashSet<u64>,
}

impl Cache {
    fn global() -> &'static Self {
        static CACHE: OnceLock<Cache> = OnceLock::new();
        CACHE.get_or_init(Cache::new)
    }

    fn new() -> Self {
        Self {
            state: Mutex::new(CacheState {
                modules: LruCache::new(MODULES_CACHE_CAPACITY),
                compiling: HashSet::new(),
            }),
            module_ready: Condvar::new(),
        }
    }

    fn hash(code: &[u8]) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(code);
        hasher.finish()
    }

    fn get_impl(&'static self, engine: &Engine, code: &[u8]) -> wasmtime::Result<ModuleFrom> {
        let hash = Self::hash(code);

        let compile_permit = match self.reserve_compile(hash, engine)? {
            Ok(compile_permit) => compile_permit,
            Err(module) => return Ok(module),
        };

        tracing::trace!("create wasmtime module because of missed LRU cache");
        let module = Module::new(engine, code).context("failed to create module")?;
        self.insert_module(hash, module.clone());

        drop(compile_permit);
        Ok(ModuleFrom::New(module))
    }

    fn reserve_compile(
        &'static self,
        hash: u64,
        engine: &Engine,
    ) -> wasmtime::Result<Result<CompilePermit, ModuleFrom>> {
        let mut state = self.state.lock().unwrap();

        loop {
            // Re-check after every wake-up: another thread may have inserted
            // the module while we slept, or the condvar may wake spuriously.
            if let Some(module) = self.cached_module(&mut state, engine, hash)? {
                return Ok(Err(module));
            }

            // Inserting the hash makes this thread the only compiler for this
            // code. Different hashes do not block each other.
            if state.compiling.insert(hash) {
                return Ok(Ok(CompilePermit { cache: self, hash }));
            }

            state = self.module_ready.wait(state).unwrap();
        }
    }

    fn cached_module(
        &self,
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

    fn insert_module(&self, hash: u64, module: Module) {
        let mut state = self.state.lock().unwrap();
        let old_module = state.modules.put(hash, module);
        debug_assert!(old_module.is_none());
    }

    fn finish_compile(&self, hash: u64) {
        {
            let mut state = self.state.lock().unwrap();
            debug_assert!(state.compiling.remove(&hash));
        }

        self.module_ready.notify_all();
    }
}

/// RAII marker for one in-progress compile.
///
/// The permit is created while holding `Cache::state`, then compilation happens
/// without the mutex. Its `Drop` implementation clears `compiling` and notifies
/// waiters, including when `Module::new` returns an error.
struct CompilePermit {
    cache: &'static Cache,
    hash: u64,
}

impl Drop for CompilePermit {
    fn drop(&mut self) {
        self.cache.finish_compile(self.hash);
    }
}

enum ModuleFrom {
    Lru(Module),
    EngineChanged(Module),
    New(Module),
}

/// Returns a compiled Wasmtime module, using an in-memory LRU cache on hits.
pub fn get(engine: &Engine, code: &[u8]) -> wasmtime::Result<Module> {
    match Cache::global().get_impl(engine, code)? {
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

        let cache = Cache::global();

        let module = cache
            .get_impl(&engine, EMPTY_WASM)
            .expect("module compiles");
        assert!(matches!(module, ModuleFrom::New(_)));

        let module = cache
            .get_impl(&engine, EMPTY_WASM)
            .expect("module loads from cache");
        assert!(matches!(module, ModuleFrom::Lru(_)));

        let module = cache
            .get_impl(&Engine::default(), EMPTY_WASM)
            .expect("module loads from cache");
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

                    match Cache::global()
                        .get_impl(&engine, CUSTOM_WASM)
                        .expect("module loads")
                    {
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

                    let source = match Cache::global()
                        .get_impl(&engine, code)
                        .expect("module loads")
                    {
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
