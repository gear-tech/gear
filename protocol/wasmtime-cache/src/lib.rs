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

#[cfg(all(loom, test))]
use loom::sync::{Condvar, Mutex};
#[cfg(not(all(loom, test)))]
use std::sync::{Condvar, Mutex};

use gear_core::ids::{CodeId, prelude::CodeIdExt};
use lru::LruCache;
use std::{collections::HashSet, num::NonZeroUsize, sync::OnceLock};
use wasmtime::{Engine, Module, error::Context};

const MODULES_CACHE_CAPACITY: NonZeroUsize = NonZeroUsize::new(1024).unwrap();

struct Cache {
    state: Mutex<CacheState>,
    module_ready: Condvar,
}

struct CacheState {
    modules: LruCache<CodeId, Module>,
    // Codes currently being compiled outside the mutex. A code is present here
    // only while its owner holds a `CompilePermit`.
    compiling: HashSet<CodeId>,
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

    fn get(&self, engine: &Engine, code: &[u8]) -> wasmtime::Result<ModuleFrom> {
        let code_id = CodeId::generate(code);

        let _permit = match self.reserve_compile(code_id, engine)? {
            Ok(permit) => permit,
            Err(module) => return Ok(module),
        };

        tracing::trace!("create wasmtime module because of missed LRU cache");

        let module = Module::new(engine, code).context("failed to create module")?;

        let mut state = self.state.lock().unwrap();
        let old_module = state.modules.put(code_id, module.clone());
        debug_assert!(old_module.is_none());

        Ok(ModuleFrom::New(module))
    }

    fn reserve_compile(
        &self,
        code_id: CodeId,
        engine: &Engine,
    ) -> wasmtime::Result<Result<CompilePermit<'_>, ModuleFrom>> {
        let mut state = self.state.lock().unwrap();

        loop {
            // Re-check after every wake-up: another thread may have inserted
            // the module while we slept, or the condvar may wake spuriously.
            if let Some(module) = Self::cached_module(&mut state, engine, code_id)? {
                return Ok(Err(module));
            }

            // Inserting the code makes this thread the only compiler for this
            // code. Different codes do not block each other.
            if state.compiling.insert(code_id) {
                return Ok(Ok(CompilePermit {
                    cache: self,
                    code_id,
                }));
            }

            state = self.module_ready.wait(state).unwrap();
        }
    }

    fn cached_module(
        state: &mut CacheState,
        engine: &Engine,
        code_id: CodeId,
    ) -> wasmtime::Result<Option<ModuleFrom>> {
        let Some(module) = state.modules.get(&code_id) else {
            return Ok(None);
        };

        tracing::trace!("load wasmtime module from LRU cache");

        if Engine::same(module.engine(), engine) {
            Ok(Some(ModuleFrom::Lru(module.clone())))
        } else {
            tracing::trace!("reserialize module because of changed engine");
            let module = match module
                .serialize()
                .context("failed to serialize module")
                .and_then(|module| unsafe {
                    Module::deserialize(engine, &module).context("failed to deserialize module")
                }) {
                Ok(module) => module,
                Err(error) => {
                    tracing::trace!(
                        "failed to reserialize module for changed engine, recompiling: {error:?}"
                    );
                    state.modules.pop(&code_id);
                    // Treat an engine-incompatible serialized module as a miss:
                    // the caller will reserve a compile slot and run
                    // `Module::new(engine, code)` outside the mutex.
                    return Ok(None);
                }
            };
            let old_module = state.modules.put(code_id, module.clone());
            debug_assert!(old_module.is_some());
            Ok(Some(ModuleFrom::EngineChanged(module)))
        }
    }

    fn finish_compile(&self, code_id: CodeId) {
        {
            let mut state = self.state.lock().unwrap();
            let removed = state.compiling.remove(&code_id);
            debug_assert!(removed);
        }

        self.module_ready.notify_all();
    }
}

/// RAII marker for one in-progress compile.
///
/// The permit is created while holding `Cache::state`, then compilation happens
/// without the mutex. Its `Drop` implementation clears `compiling` and notifies
/// waiters, including when `Module::new` returns an error.
struct CompilePermit<'a> {
    cache: &'a Cache,
    code_id: CodeId,
}

impl Drop for CompilePermit<'_> {
    fn drop(&mut self) {
        self.cache.finish_compile(self.code_id);
    }
}

enum ModuleFrom {
    Lru(Module),
    EngineChanged(Module),
    New(Module),
}

/// Returns a compiled Wasmtime module, using an in-memory LRU cache on hits.
pub fn get(engine: &Engine, code: &[u8]) -> wasmtime::Result<Module> {
    static CACHE: OnceLock<Cache> = OnceLock::new();

    let cache = CACHE.get_or_init(Cache::new);
    match cache.get(engine, code)? {
        ModuleFrom::Lru(module) | ModuleFrom::EngineChanged(module) | ModuleFrom::New(module) => {
            Ok(module)
        }
    }
}

#[cfg(not(loom))]
#[cfg(test)]
mod tests {
    use super::*;
    use wasmtime::{Config, ModuleVersionStrategy};

    const EMPTY_WASM: &[u8] = b"\x00asm\x01\x00\x00\x00";

    fn engine_with_module_version(version: &str) -> Engine {
        let mut config = Config::new();
        config
            .module_version(ModuleVersionStrategy::Custom(version.to_string()))
            .expect("module version is valid");
        Engine::new(&config).expect("engine config is valid")
    }

    #[test]
    fn smoke() {
        let engine = Engine::default();

        let cache = Cache::new();

        let module = cache.get(&engine, EMPTY_WASM).expect("module compiles");
        assert!(matches!(module, ModuleFrom::New(_)));

        let module = cache
            .get(&engine, EMPTY_WASM)
            .expect("module loads from cache");
        assert!(matches!(module, ModuleFrom::Lru(_)));

        let module = cache
            .get(&Engine::default(), EMPTY_WASM)
            .expect("module loads from cache");
        assert!(matches!(module, ModuleFrom::EngineChanged(_)));
    }

    #[test]
    fn compiles_when_cached_module_cannot_be_deserialized_for_engine() {
        let cache = Cache::new();

        let module = cache
            .get(&engine_with_module_version("first"), EMPTY_WASM)
            .expect("module compiles");
        assert!(matches!(module, ModuleFrom::New(_)));

        let module = cache
            .get(&engine_with_module_version("second"), EMPTY_WASM)
            .expect("module compiles after deserialize miss");
        assert!(matches!(module, ModuleFrom::New(_)));
    }
}

#[cfg(loom)]
#[cfg(test)]
mod tests_loom {
    use super::*;
    use loom::{sync::Arc, thread};

    const EMPTY_WASM: &[u8] = b"\x00asm\x01\x00\x00\x00";

    #[test]
    fn loom_environment() {
        loom::model(|| {
            let engine = Engine::default();
            let cache = Arc::new(Cache::new());
            let mut threads = Vec::new();

            for i in 0..2 {
                let cache = cache.clone();
                let engine = engine.clone();

                let handle = thread::Builder::new()
                    .stack_size(4 * 1024 * 1024)
                    .name(format!("test-thread-{i}"))
                    .spawn(move || cache.get(&engine, EMPTY_WASM).expect("module compiles"))
                    .expect("failed to spawn thread");
                threads.push(handle);
            }

            let mut new = 0;
            let mut lru = 0;
            for handle in threads {
                match handle.join().expect("thread panicked") {
                    ModuleFrom::New(_) => new += 1,
                    ModuleFrom::Lru(_) => lru += 1,
                    ModuleFrom::EngineChanged(_) => panic!("engine should not change"),
                }
            }

            assert_eq!((new, lru), (1, 1));
        });
    }
}
