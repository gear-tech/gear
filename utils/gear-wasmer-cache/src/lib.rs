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
    sync::{Mutex, OnceLock},
};
use uluru::LRUCache;
use wasmer::{CompileError, Engine, Module, SerializeError};
use wasmer_cache::Hash;

type CachedModules = Mutex<LRUCache<CachedModule, 1024>>;

struct CachedModule {
    hash: Hash,
    serialized_module: Bytes,
}

impl CachedModule {
    fn static_modules() -> &'static CachedModules {
        static MODULES: OnceLock<CachedModules> = OnceLock::new();
        MODULES.get_or_init(CachedModules::default)
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

            // downgrade the lock so other threads & processes can read the file
            file.lock_shared()?;
            file.read_to_end(&mut serialized_module)?;

            // SAFETY: we deserialize module we serialized earlier in the same code
            let module = unsafe {
                Module::deserialize_unchecked(engine, &serialized_module)
                    .expect("corrupted file cache")
            };

            (serialized_module.into(), module)
        } else {
            log::trace!("compile module because of missed cache");

            let module = Module::new(engine, code)?;
            let serialized_module = module.serialize()?;

            file.write_all(&serialized_module)?;
            file.flush()?;

            (serialized_module, module)
        };

        // explicitly drop the lock to
        // allow other threads & processes to read the file
        file.unlock()?;

        modules.insert(CachedModule {
            hash,
            serialized_module,
        });

        module
    };

    Ok(module)
}
