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

use crate::optimize;
use gear_wasm_instrument::STACK_END_EXPORT_NAME;
use pwasm_utils::parity_wasm::{
    builder,
    elements::{
        ExportEntry, GlobalEntry, ImportCountType, Instruction, Instructions, Internal, Module,
        ValueType,
    },
};

/// Insert the export with the stack end address in `module` if there is
/// the global '__stack_pointer'.
/// By default rust compilation into wasm creates global '__stack_pointer', which
/// initialized by the end of stack address. Unfortunately this global is not an export.
/// This export can be used in runtime to identify the end of stack memory
/// and skip its uploading to the storage.
///
/// Returns error if cannot insert stack end export by some reasons.
pub fn insert_stack_end_export(module: &mut Module) -> Result<(), &'static str> {
    let module_bytes = module
        .clone()
        .to_bytes()
        .map_err(|_| "cannot get code from module")?;

    let stack_pointer_index =
        get_global_index(&module_bytes, |name| name.ends_with("__stack_pointer"))
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

/// If `_start` export function exists, then insert this function call in the beginning of
/// each export function.
///
/// If `_start` function does not exist, then do nothing, and returns Ok.
/// If `_start` export exists, but by some reason we cannot insert its call in export functions,
/// then returns Error.
pub fn insert_start_call_in_export_funcs(module: &mut Module) -> Result<(), &'static str> {
    let start_func_index = if let Some(start) = module
        .export_section()
        .ok_or("Cannot find export section")?
        .entries()
        .iter()
        .find(|export| export.field() == "_start")
    {
        match start.internal() {
            Internal::Function(index) => *index,
            _ => return Err("_start export is not a function"),
        }
    } else {
        return Ok(());
    };

    for export_name in optimize::FUNC_EXPORTS {
        let Some(export) = module
            .export_section()
            .ok_or("Cannot find export section")?
            .entries()
            .iter()
            .find(|export| export.field() == export_name) else
        {
            continue
        };

        let index = match export.internal() {
            Internal::Function(index) => *index,
            _ => return Err("Func export is not a function"),
        };

        let index_in_functions = (index as usize)
            .checked_sub(module.import_count(ImportCountType::Function))
            .ok_or("Cannot process case when export function is import")?;

        module.code_section_mut().unwrap().bodies_mut()[index_in_functions]
            .code_mut()
            .elements_mut()
            .insert(0, Instruction::Call(start_func_index));
    }

    Ok(())
}

/// For each mutable global, except stack pointer, creates buffer in memory and
/// initial constant value in data section.
/// For correct work required stack pointer and data end global names to be
/// in custom names section.
///
/// Returns error if cannot move globals to static memory by some reasons.
pub fn move_mut_globals_to_static(module: &mut Module) -> Result<(), &'static str> {
    let module_bytes = module
        .clone()
        .to_bytes()
        .map_err(|_| "cannot get code from module")?;

    // Identify stack pointer and data end globals
    let stack_pointer_index =
        get_global_index(&module_bytes, |name| name.ends_with("__stack_pointer"))
            .ok_or("Cannot find stack pointer global")?;
    let data_end_index = get_global_index(&module_bytes, |name| name.ends_with("__data_end"))
        .ok_or("Cannot find data end global")?;

    // Identify mutable globals and their initial data
    let mut mut_globals = vec![];
    for (index, global) in module
        .global_section()
        .ok_or("Cannot find globals section")?
        .entries()
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

        let global_initial_data = handle_global_init_data(
            global,
            |c| c.to_le_bytes().to_vec(),
            |c| c.to_le_bytes().to_vec(),
        )?;
        mut_globals.push((index, global_initial_data));
    }

    log::trace!("mutable globals are {:?}", mut_globals);

    let data_end_offset = handle_global_init_data(
        module
            .global_section()
            .expect("Cannot find globals section")
            .entries()
            .get(data_end_index as usize)
            .expect("We have already find data end global earlier"),
        |c| Ok(c as u32),
        |_| Err("Wrong data section initial data instruction"),
    )??;

    log::trace!("data section end offset == {:#x}", data_end_offset);

    let mut own_module = module.clone();
    let mut global_data_offset = data_end_offset;
    let mut new_data_in_section = vec![];
    for (index, data) in mut_globals {
        // Make function to get global
        own_module = append_get_global_function(own_module, global_data_offset, data.len());
        let get_global_function_index = (own_module
            .functions_space()
            .checked_sub(1)
            .expect("Must be already at least one function"))
            as u32;

        // Make function to set global
        own_module = append_set_global_function(own_module, global_data_offset, data.len());
        let set_global_function_index = (own_module
            .functions_space()
            .checked_sub(1)
            .expect("Must be at least one function already"))
            as u32;

        log::trace!(
            "make get/set global functions {} and {} for global {}",
            get_global_function_index,
            set_global_function_index,
            index
        );

        // Bypass all instructions in module and replace global.get and global.set
        // by corresponding functions call.
        for instr in own_module
            .code_section_mut()
            .ok_or("Cannot find code section")?
            .bodies_mut()
            .iter_mut()
            .flat_map(|body| body.code_mut().elements_mut().iter_mut())
        {
            let global_index = u32::try_from(index).expect("Global index bigger than u32");
            if *instr == Instruction::GetGlobal(global_index) {
                *instr = Instruction::Call(get_global_function_index);
            } else if *instr == Instruction::SetGlobal(global_index) {
                *instr = Instruction::Call(set_global_function_index);
            }
        }

        new_data_in_section.extend(data.iter());
        global_data_offset += data.len() as u32;
    }

    // Insert new data section for globals initial values
    own_module = builder::from_module(own_module)
        .data()
        .offset(Instruction::I32Const(data_end_offset as i32))
        .value(new_data_in_section)
        .build()
        .build();

    // Update data end global value
    handle_mut_global_init_data(
        module
            .global_section_mut()
            .expect("Cannot find globals section")
            .entries_mut()
            .get_mut(data_end_index as usize)
            .expect("We have already find data end global earlier"),
        |c| {
            log::debug!(
                "Change data end offset from {:#x} to {:#x}",
                c,
                global_data_offset
            );
            *c = global_data_offset as i32;
        },
        |_| unreachable!("Data end global has i32 value, which has been already checked"),
    )?;

    *module = own_module;

    Ok(())
}

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
        .flatten()
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

fn handle_global_init_data<T>(
    global: &GlobalEntry,
    process_i32: impl FnOnce(i32) -> T,
    process_i64: impl FnOnce(i64) -> T,
) -> Result<T, &'static str> {
    let init_code = global.init_expr().code();
    if init_code.len() != 2 {
        return Err("Global has more than 2 init instructions");
    }
    if init_code[1] != Instruction::End {
        return Err("Last init instruction is not End");
    }
    match init_code[0] {
        Instruction::I32Const(c) => Ok(process_i32(c)),
        Instruction::I64Const(c) => Ok(process_i64(c)),
        _ => Err("Global init instruction is not i32 or i64 const"),
    }
}

fn handle_mut_global_init_data<T>(
    global: &mut GlobalEntry,
    mut process_i32: impl FnMut(&mut i32) -> T,
    mut process_i64: impl FnMut(&mut i64) -> T,
) -> Result<T, &'static str> {
    let init_code = global.init_expr_mut().code_mut();
    if init_code.len() != 2 {
        return Err("Global has more than 2 init instructions");
    }
    if init_code[1] != Instruction::End {
        return Err("Last init instruction is not End");
    }
    match init_code
        .get_mut(0)
        .expect("Unreachable: init code has no instructions")
    {
        Instruction::I32Const(c) => Ok(process_i32(c)),
        Instruction::I64Const(c) => Ok(process_i64(c)),
        _ => Err("Global init instruction is not i32 or i64 const"),
    }
}

fn append_get_global_function(module: Module, offset: u32, data_len: usize) -> Module {
    let builder = builder::from_module(module)
        .function()
        .signature()
        .results();
    let (builder, load_instr) = match data_len {
        4 => (builder.i32(), Instruction::I32Load(2, 0)),
        8 => (builder.i64(), Instruction::I64Load(3, 0)),
        _ => unreachable!("Support only i64 and i32 globals"),
    };
    builder
        .build()
        .body()
        .with_instructions(Instructions::new(vec![
            Instruction::I32Const(offset as i32),
            load_instr,
            Instruction::End,
        ]))
        .build()
        .build()
        .build()
}

fn append_set_global_function(module: Module, offset: u32, data_len: usize) -> Module {
    let builder = builder::from_module(module).function().signature().params();
    let (builder, store_instr) = match data_len {
        4 => (builder.i32(), Instruction::I32Store(2, 0)),
        8 => (builder.i64(), Instruction::I64Store(3, 0)),
        _ => unreachable!("Support only i64 and i32 globals"),
    };
    builder
        .build()
        .build()
        .body()
        .with_instructions(Instructions::new(vec![
            Instruction::I32Const(offset as i32),
            Instruction::GetLocal(0),
            store_instr,
            Instruction::End,
        ]))
        .build()
        .build()
        .build()
}

#[cfg(test)]
mod test {
    use super::{
        insert_stack_end_export, insert_start_call_in_export_funcs, move_mut_globals_to_static,
        STACK_END_EXPORT_NAME,
    };
    use pwasm_utils::parity_wasm;
    use wabt::Wat2Wasm;
    use wasmi::{core::Value, Engine, Linker, Memory, MemoryType, Store};

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

        let binary = Wat2Wasm::new()
            .validate(true)
            .write_debug_names(true)
            .convert(wat)
            .expect("failed to parse module");

        let mut module =
            elements::deserialize_buffer(binary.as_ref()).expect("failed to deserialize binary");
        insert_stack_end_export(&mut module).expect("insert_stack_end_export failed");

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

    #[test]
    fn test_insert_start_call_to_export_funcs() {
        let wat = r#"
        (module
            (global $g (mut i32) (i32.const 10))
            (export "handle" (func $handle))
            (export "_start" (func $_start))
            (func $handle (param i32) (result i32)
                global.get $g
                local.get 0
                i32.add
            )
            (func $_start
                i32.const 11
                global.set $g
            )
        )"#;

        let binary = Wat2Wasm::new()
            .validate(true)
            .write_debug_names(true)
            .convert(wat)
            .expect("failed to parse module");

        let check = |binary, expected| {
            let mut store: Store<()> = Store::new(&Engine::default(), ());
            let mut linker: Linker<()> = Linker::new();
            let module = wasmi::Module::new(store.engine(), binary).unwrap();
            let mut outputs = [Value::I32(-1)];
            linker
                .instantiate(&mut store, &module)
                .unwrap()
                .ensure_no_start(&mut store)
                .unwrap()
                .get_export(&store, "handle")
                .unwrap()
                .into_func()
                .unwrap()
                .call(&mut store, &[Value::I32(1)], &mut outputs)
                .unwrap();
            assert_eq!(outputs[0], Value::I32(expected));
        };

        // Check that works without changes
        check(binary.as_ref(), 11);

        // Insert `_start` call in `handle` code and check that it works as expected.
        let mut module = parity_wasm::deserialize_buffer(binary.as_ref()).unwrap();
        insert_start_call_in_export_funcs(&mut module).unwrap();
        check(&module.to_bytes().unwrap(), 12);
    }

    #[test]
    fn test_move_mut_globals_to_static_memory() {
        let wat = r#"
        (module
            (import "env" "memory" (memory 1))
            (global $__data_end i32 (i32.const 2380))
            (global $__stack_pointer (mut i32) (i32.const 10000))
            (global $g1 (mut i32) (i32.const 10))
            (global $g2 (mut i32) (i32.const 100))
            (export "handle" (func $handle))
            (func $handle (param i32) (result i32)
                global.get $g1
                global.get $g2
                i32.add
                local.get 0
                i32.add
                global.set $g1
                local.get 0
                global.set $g2
                global.get $g1
            )
        )"#;

        let binary = Wat2Wasm::new()
            .validate(true)
            .write_debug_names(true)
            .convert(wat)
            .expect("failed to parse module");

        let check = |binary, expected1, expected2| {
            let mut store: Store<()> = Store::new(&Engine::default(), ());
            let module = wasmi::Module::new(store.engine(), binary).unwrap();
            let memory = Memory::new(&mut store, MemoryType::new(1, None)).unwrap();

            let mut linker: Linker<()> = Linker::new();
            linker.define("env", "memory", memory).unwrap();

            let mut outputs = [Value::I32(-1)];
            linker
                .instantiate(&mut store, &module)
                .unwrap()
                .ensure_no_start(&mut store)
                .unwrap()
                .get_export(&store, "handle")
                .unwrap()
                .into_func()
                .unwrap()
                .call(&mut store, &[Value::I32(1)], &mut outputs)
                .unwrap();
            assert_eq!(outputs[0], Value::I32(expected1));

            let mut data = vec![0u8; 0x10000];
            memory.read(&store, 0, data.as_mut_slice()).unwrap();
            let instance = linker
                .instantiate(&mut store, &module)
                .unwrap()
                .ensure_no_start(&mut store)
                .unwrap();
            memory.write(&mut store, 0, &data).unwrap();
            instance
                .get_export(&store, "handle")
                .unwrap()
                .into_func()
                .unwrap()
                .call(&mut store, &[Value::I32(1)], &mut outputs)
                .unwrap();
            assert_eq!(outputs[0], Value::I32(expected2));
        };

        // First check that it works correct without changes.
        check(binary.as_ref(), 111, 111);

        // Then check that after moving globals to static memory, globals will changed
        // their values after first execution, and second execution will return another result.
        let mut module = parity_wasm::deserialize_buffer(binary.as_ref()).unwrap();
        move_mut_globals_to_static(&mut module).unwrap();
        check(&module.to_bytes().unwrap(), 111, 113);
    }
}
