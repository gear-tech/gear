// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use gear_wasm_instrument::STACK_END_EXPORT_NAME;
use pwasm_utils::parity_wasm::elements::{ExportEntry, Instruction, Internal, Module, ValueType};

/// Insert stack end addr export in `module` if there is global '__stack_pointer'.
/// By default rust compilation into wasm creates global '__stack_pointer', which
/// initialized by end of stack address. Unfortunately this global is not export.
/// By default '__stack_pointer' has number 0 in globals, so if there is '__stack_pointer' in
/// a name section, then we suppose that 0 global contains stack end addr, and insert an export
/// for this global. This export can be used in runtime to identify end of stack memory
/// and skip its uploading to storage.
pub fn insert_stack_end_export(module: &mut Module) -> Result<(), &str> {
    let name_section = module
        .custom_sections()
        .find(|x| x.name() == "name")
        .ok_or("Cannot find name section")?;
    let payload = unsafe { std::str::from_utf8_unchecked(name_section.payload()) };

    // Unfortunately `parity-wasm` cannot work with global names subsection in custom names section.
    // So, we just check, whether names section contains '__stack_pointer' as name.
    // TODO: make parsing of global names and identify that global 0 has name '__stack_pointer'
    if !payload.contains("__stack_pointer") {
        return Err("has no stack pointer global");
    }

    let glob_section = module
        .global_section()
        .ok_or("Cannot find globals section")?;
    let zero_global = glob_section
        .entries()
        .iter()
        .next()
        .ok_or("there is no globals")?;
    if zero_global.global_type().content_type() != ValueType::I32 {
        return Err("has no i32 global 0");
    }

    let init_code = zero_global.init_expr().code();
    if init_code.len() != 2 {
        return Err("num of init instructions != 2 for glob 0");
    }

    if init_code[1] != Instruction::End {
        return Err("second init instruction is not end");
    }

    if let Instruction::I32Const(literal) = init_code[0] {
        log::debug!("stack pointer init == {:#x}", literal);
        let export_section = module
            .export_section_mut()
            .ok_or("Cannot find export section")?;
        let x = export_section.entries_mut();
        x.push(ExportEntry::new(
            STACK_END_EXPORT_NAME.to_string(),
            Internal::Global(0),
        ));
        Ok(())
    } else {
        Err("has unexpected instr for init")
    }
}
