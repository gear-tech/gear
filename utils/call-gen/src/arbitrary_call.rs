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

//! `Arbitrary` trait implementation for [`GearCall`].

// TODO
// 1. Need a more divirse config for valid wasms.
// 2. Existing programs add
// 3. gas_limit
// 4. value?
// 5. Changing execution path - with ratio
// 6. size of bytes for payloads

pub const MAX_PAYLOAD_SIZE: usize = 8 * 1024 * 1024;

use arbitrary::{Arbitrary, Unstructured, Result};
use gear_wasm_gen::{ValidGearWasmConfigsBundle, EntryPointsSet, SysCallsInjectionAmounts, SysCallName};
use crate::{GearCall, generate_gear_program};

#[derive(Debug, Clone)]
pub struct GearCalls(pub Vec<GearCall>);

impl<'a> Arbitrary<'a> for GearCalls {
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
        println!("ARB LEN {:?}", u.len());
        // Generate 10 `UploadProgram` calls.
        let mut injection_amounts = SysCallsInjectionAmounts::all_once();
        injection_amounts.set(SysCallName::Leave, 0, 0);
        injection_amounts.set(SysCallName::Panic, 0, 0);
        injection_amounts.set(SysCallName::OomPanic, 0, 0);
        injection_amounts.set(SysCallName::Exit, 0, 1);
        injection_amounts.set(SysCallName::Wait, 0, 0);
        injection_amounts.set(SysCallName::WaitFor, 0, 0);
        injection_amounts.set(SysCallName::WaitUpTo, 0, 0);
        injection_amounts.set(SysCallName::Wake, 0, 0);

        let config: ValidGearWasmConfigsBundle = ValidGearWasmConfigsBundle {
            remove_recursion: true,
            call_indirect_enabled: false,
            entry_points_set: EntryPointsSet::InitHandleHandleReply,
            injection_amounts,
            ..Default::default()
        };

        let mut calls = Vec::with_capacity(10);
        while calls.len() != 10 {
            println!("NEW PROG GEN - {:?}", u.len());
            let code = gear_wasm_gen::generate_gear_program_code(u, config.clone())?;
            let salt = bytes(u)?;
            let payload = bytes(u)?;

            let gas = 245_000_000_000 as u64;
            let value = 0;

            println!("END PROG GEN - {:?}", u.len());
            
            calls.push(GearCall::UploadProgram(crate::UploadProgramArgs((code, salt, payload, gas, value))));
        }

        println!("ENDED!");

        Ok(GearCalls(calls))
    }
}

fn bytes<'a>(u: &mut Unstructured<'a>) -> Result<Vec<u8>> {
    let mut bytes = vec![0; 4096];
    u.fill_buffer(&mut bytes)?;

    Ok(bytes)
}