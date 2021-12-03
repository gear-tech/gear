// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

//! Storage backing abstractions.

use alloc::vec::Vec;
use hashbrown::HashMap;

use crate::program::{Program, ProgramId};

/// General trait, which informs what exact storage types are used by a storage manager ("carrier").
///
/// Mainly used for readability in order to keep readable definitions of types that
/// manage different storage domains (for example, the [`Storage`]).
pub trait StorageCarrier: Default + Clone {
    /// Program storage type used by storage manager
    type PS: ProgramStorage;
}

/// Abstraction over program storage.
pub trait ProgramStorage: Default + Clone {
    /// Get the program from the storage.
    fn get(&self, id: ProgramId) -> Option<Program>;

    /// Store program in the storage.
    fn set(&mut self, program: Program) -> Option<Program>;

    /// Check if program exists.
    fn exists(&self, id: ProgramId) -> bool;

    /// Remove the program from the storage.
    fn remove(&mut self, id: ProgramId) -> Option<Program>;
}

/// In-memory program storage (for tests).
#[derive(Default, Clone)]
pub struct InMemoryProgramStorage {
    inner: HashMap<ProgramId, Program>,
}

impl InMemoryProgramStorage {
    /// Create an empty in-memory program storage.
    pub fn new() -> Self {
        Default::default()
    }
}

impl ProgramStorage for InMemoryProgramStorage {
    fn get(&self, id: ProgramId) -> Option<Program> {
        self.inner.get(&id).cloned()
    }

    fn set(&mut self, program: Program) -> Option<Program> {
        self.inner.insert(program.id(), program)
    }

    fn exists(&self, id: ProgramId) -> bool {
        self.inner.contains_key(&id)
    }

    fn remove(&mut self, id: ProgramId) -> Option<Program> {
        self.inner.remove(&id)
    }
}

impl From<Vec<Program>> for InMemoryProgramStorage {
    fn from(programs: Vec<Program>) -> Self {
        Self {
            inner: programs.into_iter().map(|p| (p.id(), p)).collect(),
        }
    }
}

impl From<InMemoryProgramStorage> for Vec<Program> {
    fn from(storage: InMemoryProgramStorage) -> Vec<Program> {
        storage
            .inner
            .into_iter()
            .map(|(_, program)| program)
            .collect()
    }
}

/// Storage.
#[derive(Default, Clone)]
pub struct Storage<PS: ProgramStorage> {
    /// Program storage.
    pub program_storage: PS,
}

impl<PS: ProgramStorage> StorageCarrier for Storage<PS> {
    type PS = PS;
}

impl<PS: ProgramStorage> Storage<PS> {
    /// Create an empty storage.
    pub fn new() -> Self {
        Default::default()
    }
}

/// Fully in-memory storage (for tests).
pub type InMemoryStorage = Storage<InMemoryProgramStorage>;

#[cfg(test)]
/// This module contains tests of parts of InMemoryStorage:
/// of allocation storage, message queue storage and program storage
mod tests {
    extern crate wabt;
    use super::*;
    use alloc::vec;

    fn parse_wat(source: &str) -> Vec<u8> {
        let module_bytes = wabt::Wat2Wasm::new()
            .validate(false)
            .convert(source)
            .expect("failed to parse module")
            .as_ref()
            .to_vec();
        module_bytes
    }

    #[test]
    /// Test that InMemoryProgramStorage works correctly
    fn program_storage_interaction() {
        let wat = r#"
            (module
                (import "env" "gr_reply_to"  (func $gr_reply_to (param i32)))
                (import "env" "memory" (memory 2))
                (export "handle" (func $handle))
                (export "handle_reply" (func $handle))
                (export "init" (func $init))
                (func $handle
                    i32.const 65536
                    call $gr_reply_to
                )
                (func $handle_reply
                    i32.const 65536
                    call $gr_reply_to
                )
                (func $init)
            )"#;

        let binary: Vec<u8> = parse_wat(wat);

        // Initialization of some ProgramIds
        let id1 = ProgramId::from(1);

        let id2 = ProgramId::from(2);

        let id3 = ProgramId::from(3);

        // Initialization of InMemoryProgramStorage with our custom vec<Program>
        let mut program_storage: InMemoryProgramStorage = vec![
            Program::new(id1, binary.clone(), Default::default()).expect("err create program"),
            Program::new(id2, binary.clone(), Default::default()).expect("err create program"),
        ]
        .into();

        // Checking that the Program with id2 exists in the storage
        // and it is the one that we put
        assert!(program_storage.get(id2).is_some());
        assert_eq!(program_storage.get(id2).unwrap().code(), binary);

        // Checking that the Program with id3 does not exist in the storage
        assert!(program_storage.get(id3).is_none());

        // Checking that we are able to correctly remove
        // the Program with id2 from storage
        program_storage.remove(id2);
        assert!(program_storage.get(id2).is_none());

        // Checking that we are able to correctly set
        // the new Program with id3 in storage
        program_storage
            .set(Program::new(id3, binary, Default::default()).expect("err create program"));
        assert!(program_storage.get(id3).is_some());

        // Сhecking that the storage after all our interactions
        // contains two programs with id1 and id3 and returns them on draining
        let remaining_programs: Vec<Program> = program_storage.into();
        assert_eq!(remaining_programs.len(), 2);

        for program in remaining_programs {
            assert!(program.id() == id1 || program.id() == id3);
        }
    }
}
