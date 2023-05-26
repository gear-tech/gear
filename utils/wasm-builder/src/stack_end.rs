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
use pwasm_utils::parity_wasm::{
    builder,
    elements::{
        ExportEntry, GlobalEntry, ImportCountType, Instruction, Instructions, Internal, Module,
        ValueType,
    },
};

fn get_global_index(module_bytes: &[u8], name_predicate: impl Fn(&str) -> bool) -> Option<u32> {
    use wasmparser::{Name, NameSectionReader, Parser, Payload};

    Parser::new(0)
        .parse_all(module_bytes)
        .filter_map(|p| p.ok())
        .filter_map(|section| match section {
            Payload::CustomSection(r) if r.name() == "name" => {
                Some(NameSectionReader::new(r.data(), r.data_offset()))
            }
            _ => None,
        })
        .flat_map(|sub_section| sub_section)
        .filter_map(|res| res.ok())
        .filter_map(|name| match name {
            Name::Global(m) => Some(m),
            _ => None,
        })
        .flat_map(|naming| naming.into_iter())
        .filter_map(|res| res.ok())
        .find(|global| name_predicate(global.name))
        .map(|global| global.index)
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

pub fn move_mut_globals_to_static(
    module_bytes: &[u8],
    module: &mut Module,
) -> Result<(), &'static str> {
    let start_func_index = if let Some(start) = module
        .export_section()
        .ok_or("Cannot find export section")?
        .entries()
        .iter()
        .find(|export| export.field() == "_start")
    {
        match start.internal() {
            Internal::Function(index) => Some(*index),
            _ => return Err("_start export is not a function"),
        }
    } else {
        None
    };

    let init_func_index = if let Some(start) = module
        .export_section()
        .ok_or("Cannot find export section")?
        .entries()
        .iter()
        .find(|export| export.field() == "init")
    {
        match start.internal() {
            Internal::Function(index) => *index,
            _ => return Err("init export is not a function"),
        }
    } else {
        *module = builder::from_module(module.clone())
            .function()
            .signature()
            .build()
            .body()
            .with_instructions(Instructions::new(vec![Instruction::End]))
            .build()
            .build()
            .build();
        let init_func_index = module.functions_space() - 1;
        *module = builder::from_module(module.clone())
            .export()
            .field("init")
            .with_internal(Internal::Function(init_func_index as u32))
            .build()
            .build();
        init_func_index as u32
    };

    if let Some(start_func_index) = start_func_index {
        let init_index_in_functions =
            init_func_index - module.import_count(ImportCountType::Function) as u32;
        module.code_section_mut().unwrap().bodies_mut()[init_index_in_functions as usize]
            .code_mut()
            .elements_mut()
            .insert(0, Instruction::Call(start_func_index));
    }

    let stack_pointer_index =
        get_global_index(module_bytes, |name| name.ends_with("__stack_pointer"))
            .ok_or("Cannot find stack pointer global")?;
    let data_end_index = get_global_index(module_bytes, |name| name.ends_with("__data_end"))
        .ok_or("Cannot find data end global")?;

    fn get_global_init_data(global: &GlobalEntry) -> Option<Vec<u8>> {
        let init_code = global.init_expr().code();
        if init_code.len() != 2 {
            return None;
        }
        if init_code[1] != Instruction::End {
            return None;
        }
        match init_code[0] {
            Instruction::I32Const(c) => Some(c.to_le_bytes().to_vec()),
            Instruction::I64Const(c) => Some(c.to_le_bytes().to_vec()),
            _ => None,
        }
    }

    let mut mut_globals = vec![];
    for (index, global) in module
        .global_section_mut()
        .ok_or("Cannot find globals section")?
        .entries_mut()
        .iter()
        .enumerate()
    {
        if !global.global_type().is_mutable() {
            continue;
        }
        if index == data_end_index as usize {
            continue;
        }
        if index == stack_pointer_index as usize {
            continue;
        }

        mut_globals.push((
            index,
            get_global_init_data(global).ok_or("Cannot get mut global init data")?,
        ));
    }

    log::debug!("mutable globals are {:?}", mut_globals);

    let data_end_offset = {
        let global = module
            .global_section_mut()
            .ok_or("Cannot find globals section (2)")?
            .entries_mut()
            .get(data_end_index as usize)
            .expect("We have already find this global earlier");
        let init_code = global.init_expr().code();
        if init_code.len() != 2 {
            return Err("Wrong data section initial instructions");
        }
        if init_code[1] != Instruction::End {
            return Err("Wrong data section initial instructions");
        }
        match init_code[0] {
            Instruction::I32Const(c) => c as u32,
            _ => return Err("Wrong data section initial instructions"),
        }
    };

    log::debug!("data section end offset == {:#x}", data_end_offset);

    let mut own_module = module.clone();
    let mut global_data_offset = data_end_offset;
    let mut new_data_in_section = vec![];
    for (index, data) in mut_globals {
        new_data_in_section.extend(data.iter());

        // Make function to set global
        own_module = match data.len() {
            4 => builder::from_module(own_module)
                .function()
                .signature()
                .params()
                .i32()
                .build()
                .build()
                .body()
                .with_instructions(Instructions::new(vec![
                    Instruction::I32Const(global_data_offset as i32),
                    Instruction::GetLocal(0),
                    Instruction::I32Store(2, 0),
                    Instruction::End,
                ]))
                .build()
                .build()
                .build(),
            8 => builder::from_module(own_module)
                .function()
                .signature()
                .params()
                .i64()
                .build()
                .build()
                .body()
                .with_instructions(Instructions::new(vec![
                    Instruction::I32Const(global_data_offset as i32),
                    Instruction::GetLocal(0),
                    Instruction::I64Store(3, 0),
                    Instruction::End,
                ]))
                .build()
                .build()
                .build(),
            _ => unreachable!("LOL"),
        };

        let set_global_function_index = (own_module.functions_space() - 1) as u32;
        log::debug!(
            "make set global function, index == {}",
            set_global_function_index
        );

        for body in own_module
            .code_section_mut()
            .ok_or("Cannot find code section")?
            .bodies_mut()
            .iter_mut()
            .map(|body| body.code_mut().elements_mut())
        {
            let mut get_positions = vec![];
            for (index_instr, instr) in body.iter_mut().enumerate() {
                let global_index = u32::try_from(index).expect("KEK");
                if *instr == Instruction::GetGlobal(global_index) {
                    match data.len() {
                        4 => *instr = Instruction::I32Load(2, 0),
                        8 => *instr = Instruction::I64Load(3, 0),
                        _ => unreachable!("LOL"),
                    }
                    get_positions.push((
                        index_instr,
                        Instruction::I32Const(global_data_offset as i32),
                    ));
                } else if *instr == Instruction::SetGlobal(global_index) {
                    match data.len() {
                        4 | 8 => *instr = Instruction::Call(set_global_function_index),
                        _ => unreachable!("LOL"),
                    }
                }
            }
            while let Some((index, init_instruction)) = get_positions.pop() {
                body.insert(index, init_instruction);
            }
        }

        global_data_offset += data.len() as u32;
    }

    own_module = builder::from_module(own_module)
        .data()
        .offset(Instruction::I32Const(data_end_offset as i32))
        .value(new_data_in_section)
        .build()
        .build();

    // Change data end global
    {
        let global = own_module
            .global_section_mut()
            .ok_or("Cannot find globals section (2)")?
            .entries_mut()
            .get_mut(data_end_index as usize)
            .expect("We have already find this global earlier");
        let init_code = global.init_expr_mut().code_mut();
        if init_code.len() != 2 {
            return Err("Wrong data section initial instructions");
        }
        if init_code[1] != Instruction::End {
            return Err("Wrong data section initial instructions");
        }
        match init_code[0] {
            Instruction::I32Const(c) => {
                log::debug!(
                    "Change data end offset from {:#x} to {:#x}",
                    c,
                    global_data_offset
                );
                *init_code.get_mut(0).unwrap() = Instruction::I32Const(global_data_offset as i32)
            }
            _ => return Err("Wrong data section initial instructions"),
        }
    }

    let s = wasmprinter::print_bytes(&own_module.clone().to_bytes().unwrap()).unwrap();
    log::debug!("{}", s);

    *module = own_module;

    Ok(())
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
