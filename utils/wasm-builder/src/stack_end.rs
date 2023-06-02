// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

fn get_global_index(module_bytes: &[u8], name_predicate: impl Fn(&str) -> bool) -> Option<u32> {
    use wasmparser::{Name, NameSectionReader, Parser, Payload::*};

    let parser = Parser::new(0);
    let mut reader = parser.parse_all(module_bytes).find_map(|p| {
        p.ok().and_then(|section| match section {
            CustomSection(r) if r.name() == "name" => {
                Some(NameSectionReader::new(r.data(), r.data_offset()))
            }
            _ => None,
        })
    })?;

    let global_map = reader.find_map(|name| match name {
        Ok(Name::Global(m)) => Some(m),
        _ => None,
    })?;

    for global in global_map {
        match global {
            Ok(g) if name_predicate(g.name) => return Some(g.index),
            _ => (),
        }
    }

    None
}

/// Insert the export with the stack end address in `module` if there is
/// the global '__stack_pointer'.
/// By default rust compilation into wasm creates global '__stack_pointer', which
/// initialized by the end of stack address. Unfortunately this global is not an export.
///
/// This export can be used in runtime to identify the end of stack memory
/// and skip its uploading to the storage.
pub fn insert_stack_end_export(
    module_bytes: &[u8],
    module: &mut Module,
) -> Result<(), &'static str> {
    let stack_pointer_index =
        get_global_index(module_bytes, |name| name.ends_with("__stack_pointer"))
            .ok_or("has no stack pointer global")?;

    let glob_section = module
        .global_section()
        .ok_or("Cannot find globals section")?;
    let global = glob_section
        .entries()
        .iter()
        .nth(stack_pointer_index as usize)
        .ok_or("there is no globals")?;
    if global.global_type().content_type() != ValueType::I32 {
        return Err("has no i32 global 0");
    }

    let init_code = global.init_expr().code();
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
            Internal::Global(stack_pointer_index),
        ));
        Ok(())
    } else {
        Err("has unexpected instr for init")
    }
}

#[test]
fn assembly_script_stack_pointer() {
    use pwasm_utils::parity_wasm::elements;

    let wat = r#"
        (module
            (import "env" "memory" (memory 1))
            (global $~lib/memory/__data_end i32 (i32.const 2380))
            (global $~lib/memory/__stack_pointer (mut i32) (i32.const 1050956))
            (export "handle" (func $handle))
            (export "init" (func $init))
            (func $handle)
            (func $init)
        )"#;

    let binary = wabt::Wat2Wasm::new()
        .validate(true)
        .write_debug_names(true)
        .convert(wat)
        .expect("failed to parse module")
        .as_ref()
        .to_vec();

    let mut module = elements::deserialize_buffer(&binary).expect("failed to deserialize binary");
    insert_stack_end_export(&binary, &mut module).expect("insert_stack_end_export failed");

    let gear_stack_end = module
        .export_section()
        .expect("export section should exist")
        .entries()
        .iter()
        .find(|e| e.field() == STACK_END_EXPORT_NAME)
        .expect("export entry should exist");

    assert!(matches!(
        gear_stack_end.internal(),
        elements::Internal::Global(1)
    ));
}
