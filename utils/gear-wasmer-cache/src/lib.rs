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
use fs4::fs_std::FileExt;
use std::{
    fs::File,
    io,
    io::{Read, Write},
    path::Path,
};
use uluru::LRUCache;
use wasmer::{CompileError, Engine, Module, SerializeError};
use wasmer_cache::Hash;

cfg_if::cfg_if! {
    if #[cfg(all(loom, test))] {
        use loom::sync::Mutex;
    } else {
        use std::sync::Mutex;
    }
}

type CachedModules = Mutex<LRUCache<CachedModule, 1024>>;

struct CachedModule {
    hash: Hash,
    serialized_module: Bytes,
}

impl CachedModule {
    fn static_modules() -> &'static CachedModules {
        cfg_if::cfg_if! {
            if #[cfg(all(loom, test))] {
                loom::lazy_static! {
                    static ref MODULES: CachedModules = CachedModules::default();
                }
                &*MODULES
            } else {
                static MODULES: std::sync::OnceLock<CachedModules> = std::sync::OnceLock::new();
                MODULES.get_or_init(CachedModules::default)
            }
        }
    }
}

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
    #[display(fmt = "Compilation error: {_0}")]
    Compile(CompileError),
    #[display(fmt = "IO error: {_0}")]
    Io(io::Error),
    #[display(fmt = "Serialization error: {_0}")]
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

pub fn get(engine: &Engine, code: &[u8], base_path: impl AsRef<Path>) -> Result<Module, Error> {
    let mut modules = CachedModule::static_modules()
        .lock()
        .expect("failed to lock modules");

    let hash = Hash::generate(code);
    let module = if let Some(module) = modules.find(|x| x.hash == hash) {
        log::trace!("load module from LRU cache");

        // SAFETY: we deserialize module we serialized earlier in the same code
        unsafe {
            Module::deserialize_unchecked(engine, &*module.serialized_module)
                .expect("corrupted in-memory cache")
        }
    } else {
        let path = base_path.as_ref().join(hash.to_string());
        // open file with all options to lock the file and
        // retrieve metadata without concurrency issues
        let mut file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;
        file.lock_exclusive()?;
        let metadata = file.metadata()?;

        // if length of the file is not zero, it means the module was cached before
        let (serialized_module, module) = if metadata.len() != 0 {
            log::trace!("load module from file cache");

            let mut serialized_module = Vec::new();
            file.read_to_end(&mut serialized_module)?;

            // SAFETY: we deserialize module we serialized earlier in the same code
            // but use `deserialize` instead of `deserialize_unchecked` to prevent issues
            // if wasmer changes its format
            unsafe {
                match Module::deserialize(engine, &serialized_module) {
                    Ok(module) => (serialized_module.into(), module),
                    Err(e) => {
                        log::trace!("recompile module because file cache corrupted: {e}");
                        compile_and_write_module(engine, code, &mut file)?
                    }
                }
            }
        } else {
            log::trace!("compile module because of missed cache");
            compile_and_write_module(engine, code, &mut file)?
        };

        // explicitly drop the lock to
        // allow other threads & processes to read the file
        // because some OS only unlock on process exit
        file.unlock()?;

        modules.insert(CachedModule {
            hash,
            serialized_module,
        });

        module
    };

    Ok(module)
}

#[cfg(loom)]
#[cfg(test)]
mod tests {
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
