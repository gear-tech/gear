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

use bytes::Bytes;
use std::{
    fs::File,
    io,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use uluru::LRUCache;
use wasmer::{CompileError, Engine, Module, SerializeError};
use wasmer_cache::Hash;

#[cfg(all(loom, test))]
use loom::sync::Mutex;
#[cfg(not(all(loom, test)))]
use std::sync::Mutex;

type CachedModules = Mutex<LRUCache<CachedModule, 1024>>;

struct CachedModule {
    hash: Hash,
    serialized_module: Bytes,
}

impl CachedModule {
    fn with_static_modules<F, R>(f: F) -> R
    where
        F: FnOnce(&mut LRUCache<CachedModule, 1024>) -> R,
    {
        #[cfg(all(loom, test))]
        let modules = {
            loom::lazy_static! {
                static ref MODULES: CachedModules = CachedModules::default();
            }
            &*MODULES
        };

        #[cfg(not(all(loom, test)))]
        let modules = {
            static MODULES: std::sync::OnceLock<CachedModules> = std::sync::OnceLock::new();
            MODULES.get_or_init(CachedModules::default)
        };

        let mut modules = modules.lock().expect("failed to lock modules");
        f(&mut modules)
    }
}

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
    #[display("Compilation error: {_0}")]
    Compile(CompileError),
    #[display("IO error: {_0}")]
    Io(io::Error),
    #[display("Serialization error: {_0}")]
    Serialize(SerializeError),
}

fn compile_and_write_module(
    engine: &Engine,
    code: &[u8],
    file: &mut File,
) -> Result<(Bytes, Module), Error> {
    let module = Module::new(engine, code)?;
    let serialized_module = module.serialize()?;

    file.write_all(&serialized_module)?;
    file.flush()?;

    Ok((serialized_module, module))
}

enum ModuleFrom {
    Lru(Module),
    Fs(Module),
    Recompilation(Module),
    CacheMiss(Module),
}

fn get_impl(
    engine: &Engine,
    code: &[u8],
    base_path: impl AsRef<Path>,
) -> Result<ModuleFrom, Error> {
    let hash = Hash::generate(code);
    let serialized_module = CachedModule::with_static_modules(|modules| {
        modules
            .find(|x| x.hash == hash)
            .map(|module| module.serialized_module.clone())
    });

    let module = if let Some(serialized_module) = serialized_module {
        log::trace!("load module from LRU cache");

        // SAFETY: we deserialize module we serialized earlier in the same code
        unsafe {
            ModuleFrom::Lru(
                Module::deserialize_unchecked(engine, &*serialized_module)
                    .expect("corrupted in-memory cache"),
            )
        }
    } else {
        let path = base_path.as_ref().join(hash.to_string());
        // open file with all options to lock the file and
        // retrieve metadata without concurrency issues
        let mut file = File::options()
            .read(true)
            .append(true)
            .create(true)
            .open(path)?;
        file.lock()?;

        let mut f = || -> Result<_, Error> {
            let metadata = file.metadata()?;

            // if length of the file is not zero, it means the module was cached before
            if metadata.len() != 0 {
                log::trace!("load module from file cache");

                let mut serialized_module = Vec::new();
                file.read_to_end(&mut serialized_module)?;

                // SAFETY: we deserialize module we serialized earlier in the same code
                // but use `deserialize` instead of `deserialize_unchecked` to prevent issues
                // if wasmer changes its format
                unsafe {
                    match Module::deserialize(engine, &serialized_module) {
                        Ok(module) => Ok((serialized_module.into(), ModuleFrom::Fs(module))),
                        Err(e) => {
                            log::trace!("recompile module because file cache corrupted: {e}");
                            file.seek(SeekFrom::Start(0))?;
                            file.set_len(0)?;
                            let (serialized_module, module) =
                                compile_and_write_module(engine, code, &mut file)?;
                            Ok((serialized_module, ModuleFrom::Recompilation(module)))
                        }
                    }
                }
            } else {
                log::trace!("compile module because of missed cache");
                let (serialized_module, module) =
                    compile_and_write_module(engine, code, &mut file)?;
                Ok((serialized_module, ModuleFrom::CacheMiss(module)))
            }
        };

        let res = f();

        // explicitly drop the lock even on error to
        // allow other threads & processes to read the file
        // because some OS only unlock on process exit
        file.unlock()?;

        let (serialized_module, module) = res?;

        CachedModule::with_static_modules(|modules| {
            modules.insert(CachedModule {
                hash,
                serialized_module,
            })
        });

        module
    };

    Ok(module)
}

pub fn get(engine: &Engine, code: &[u8], base_path: impl AsRef<Path>) -> Result<Module, Error> {
    match get_impl(engine, code, base_path)? {
        ModuleFrom::Lru(module) => Ok(module),
        ModuleFrom::Fs(module) => Ok(module),
        ModuleFrom::Recompilation(module) => Ok(module),
        ModuleFrom::CacheMiss(module) => Ok(module),
    }
}

#[cfg(not(loom))]
#[cfg(test)]
mod tests {
    use super::*;
    use demo_constructor::WASM_BINARY;
    use std::fs;

    #[test]
    fn different_cases() {
        let engine = Engine::default();
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_dir = temp_dir.path();

        // first time caching
        let module = crate::get_impl(&engine, WASM_BINARY, temp_dir).unwrap();
        assert!(matches!(module, ModuleFrom::CacheMiss(_)));

        let module = crate::get_impl(&engine, WASM_BINARY, temp_dir).unwrap();
        assert!(matches!(module, ModuleFrom::Lru(_)));

        let module = crate::get_impl(&engine, WASM_BINARY, temp_dir).unwrap();
        assert!(matches!(module, ModuleFrom::Lru(_)));

        let saved_module = temp_dir.read_dir().unwrap().next().unwrap().unwrap().path();

        // LRU cache miss
        CachedModule::with_static_modules(|modules| {
            modules.clear();
        });

        let module = crate::get_impl(&engine, WASM_BINARY, temp_dir).unwrap();
        assert!(matches!(module, ModuleFrom::Fs(_)));

        let module = crate::get_impl(&engine, WASM_BINARY, temp_dir).unwrap();
        assert!(matches!(module, ModuleFrom::Lru(_)));

        // total cache miss
        CachedModule::with_static_modules(|modules| {
            modules.clear();
        });
        fs::remove_file(&saved_module).unwrap();

        let module = crate::get_impl(&engine, WASM_BINARY, temp_dir).unwrap();
        assert!(matches!(module, ModuleFrom::CacheMiss(_)));

        // corrupted file cache
        CachedModule::with_static_modules(|modules| {
            modules.clear();
        });
        fs::write(&saved_module, "invalid module").unwrap();

        let module = crate::get_impl(&engine, WASM_BINARY, temp_dir).unwrap();
        assert!(matches!(module, ModuleFrom::Recompilation(_)));

        let module = crate::get_impl(&engine, WASM_BINARY, temp_dir).unwrap();
        assert!(matches!(module, ModuleFrom::Lru(_)));

        // check recompiled module is saved
        let serialized_module = fs::read(&saved_module).unwrap();

        CachedModule::with_static_modules(|modules| {
            modules.clear();
        });

        let module = crate::get_impl(&engine, WASM_BINARY, temp_dir).unwrap();
        if let ModuleFrom::Fs(module) = module {
            assert_eq!(serialized_module, module.serialize().unwrap());
        } else {
            unreachable!("module should be loaded from fs cache");
        }
    }
}

#[cfg(loom)]
#[cfg(test)]
mod tests_loom {
    use super::*;
    use demo_constructor::WASM_BINARY;
    use loom::thread;

    #[test]
    fn loom_environment() {
        loom::model(|| {
            let engine = Engine::default();
            let temp_dir = tempfile::tempdir().unwrap();
            let temp_dir = temp_dir.path();
            let mut threads = Vec::new();

            for i in 1..loom::MAX_THREADS {
                let engine = engine.clone();
                let temp_dir = temp_dir.to_path_buf();

                let handle = thread::Builder::new()
                    .stack_size(4 * 1024 * 1024)
                    .name(format!("test-thread-{i}"))
                    .spawn(move || {
                        let _module = crate::get(&engine, WASM_BINARY, &temp_dir).unwrap();
                    })
                    .unwrap();
                threads.push(handle);
            }

            for handle in threads {
                handle.join().unwrap();
            }
        });
    }
}
