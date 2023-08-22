// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use super::*;

pub fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

pub fn parse_wat(source: &str) -> Vec<u8> {
    wabt::Wat2Wasm::new()
        .validate(true)
        .convert(source)
        .expect("failed to parse module")
        .as_ref()
        .to_vec()
}

pub fn h256_code_hash(code: &[u8]) -> H256 {
    CodeId::generate(code).into_origin()
}

#[track_caller]
pub fn get_active_program(program_id: ProgramId) -> ActiveProgram<BlockNumber> {
    ProgramStorageOf::<Test>::get_program(program_id)
        .and_then(|p| ActiveProgram::try_from(p).ok())
        .expect("program should exist")
}
