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

use gear_common::STORAGE_PROGRAM_PREFIX;
use gear_core::{
    program::{Program, ProgramId},
    storage::ProgramStorage,
};
use sp_std::prelude::*;

#[derive(Default, Clone)]
pub struct ExtProgramStorage;

impl ProgramStorage for ExtProgramStorage {
    fn get(&self, id: ProgramId) -> Option<Program> {
        gear_common::native::get_program(id)
    }

    fn set(&mut self, program: Program) -> Option<Program> {
        gear_common::native::set_program(program);
        None
    }

    fn exists(&self, id: ProgramId) -> bool {
        gear_common::native::program_exists(id)
    }

    fn remove(&mut self, _id: ProgramId) -> Option<Program> {
        unimplemented!()
    }
}

impl ExtProgramStorage {
    pub fn iter(&self) -> ExtProgramStorageIter {
        ExtProgramStorageIter {
            key: Some(STORAGE_PROGRAM_PREFIX.to_vec()),
        }
    }
}

pub struct ExtProgramStorageIter {
    key: Option<Vec<u8>>,
}

impl Iterator for ExtProgramStorageIter {
    type Item = Program;

    fn next(&mut self) -> Option<Self::Item> {
        self.key = self.key.as_ref().and_then(|key| {
            sp_io::storage::next_key(key).filter(|key| key.starts_with(STORAGE_PROGRAM_PREFIX))
        });

        self.key.as_ref().and_then(|key| {
            gear_common::native::get_program(ProgramId::from_slice(
                &key[STORAGE_PROGRAM_PREFIX.len()..],
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use gear_common::{STORAGE_CODE_PREFIX, STORAGE_MESSAGE_PREFIX, STORAGE_WAITLIST_PREFIX};

    fn new_test_ext() -> sp_io::TestExternalities {
        frame_system::GenesisConfig::default()
            .build_storage::<gear_runtime::Runtime>()
            .unwrap()
            .into()
    }

    fn new_test_storage() -> gear_core::storage::Storage<ExtProgramStorage> {
        sp_io::storage::clear_prefix(STORAGE_CODE_PREFIX, None);
        sp_io::storage::clear_prefix(STORAGE_MESSAGE_PREFIX, None);
        sp_io::storage::clear_prefix(STORAGE_PROGRAM_PREFIX, None);
        sp_io::storage::clear_prefix(STORAGE_WAITLIST_PREFIX, None);
        gear_core::storage::Storage {
            program_storage: ExtProgramStorage,
        }
    }

    fn parse_wat(source: &str) -> Vec<u8> {
        wabt::Wat2Wasm::new()
            .validate(false)
            .convert(source)
            .expect("failed to parse module")
            .as_ref()
            .to_vec()
    }

    #[test]
    fn program_storage_iterator() {
        new_test_ext().execute_with(|| {
            let mut storage = new_test_storage();

            let wat = r#"
            (module
                (import "env" "memory" (memory 1))
            )"#;
            let code = parse_wat(wat);

            for id in 1..=10 {
                let program =
                    Program::new(ProgramId::from(id), code.clone(), Default::default()).unwrap();
                storage.program_storage.set(program);
            }

            let programs_count = storage.program_storage.iter().count();
            assert_eq!(programs_count, 10)
        })
    }
}
