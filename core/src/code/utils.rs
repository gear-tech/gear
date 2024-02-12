// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

//! Module that contains functions to check code.

use crate::{
    code::errors::*,
    message::{DispatchKind, WasmEntryPoint},
    pages::{PageNumber, PageU32Size, WasmPage},
};
use alloc::{collections::BTreeSet, vec};
use gear_wasm_instrument::{
    parity_wasm::{
        self,
        elements::{
            ExportEntry, External, GlobalEntry, GlobalType, ImportCountType, InitExpr, Instruction,
            Internal, Module, Type, ValueType,
        },
    },
    SyscallName, STACK_END_EXPORT_NAME,
};

/// Defines maximal permitted count of memory pages.
pub const MAX_WASM_PAGE_AMOUNT: u16 = 512;

/// Name of exports allowed on chain.
pub const ALLOWED_EXPORTS: [&str; 6] = [
    "init",
    "handle",
    "handle_reply",
    "handle_signal",
    "state",
    "metahash",
];

/// Name of exports required on chain (only 1 of these is required).
pub const REQUIRED_EXPORTS: [&str; 2] = ["init", "handle"];

pub fn get_static_pages(module: &Module) -> Result<WasmPage, CodeError> {
    // get initial memory size from memory import
    let static_pages = module
        .import_section()
        .ok_or(SectionError::NotFound(SectionName::Import))?
        .entries()
        .iter()
        .find_map(|entry| match entry.external() {
            External::Memory(mem_ty) => Some(mem_ty.limits().initial()),
            _ => None,
        })
        .map(WasmPage::new)
        .ok_or(MemoryError::EntryNotFound)?
        .map_err(|_| MemoryError::InvalidStaticPageCount)?;

    if static_pages.raw() > MAX_WASM_PAGE_AMOUNT as u32 {
        Err(MemoryError::InvalidStaticPageCount)?;
    }

    Ok(static_pages)
}

pub fn get_exports(module: &Module) -> BTreeSet<DispatchKind> {
    let mut entries = BTreeSet::new();

    for entry in module
        .export_section()
        .expect("Exports section has been checked for already")
        .entries()
        .iter()
    {
        if let Internal::Function(_) = entry.internal() {
            if let Some(entry) = DispatchKind::try_from_entry(entry.field()) {
                entries.insert(entry);
            }
        }
    }

    entries
}

pub fn check_exports(module: &Module) -> Result<(), CodeError> {
    let types = module
        .type_section()
        .ok_or(SectionError::NotFound(SectionName::Type))?
        .types();

    let funcs = module
        .function_section()
        .ok_or(SectionError::NotFound(SectionName::Function))?
        .entries();

    let import_count = module.import_count(ImportCountType::Function) as u32;

    let exports = module
        .export_section()
        .ok_or(SectionError::NotFound(SectionName::Export))?
        .entries();

    let mut entry_point_found = false;
    for (export_index, export) in exports.iter().enumerate() {
        let &Internal::Function(func_index) = export.internal() else {
            continue;
        };

        let index =
            func_index
                .checked_sub(import_count)
                .ok_or(ExportError::ExportReferencesToImport(
                    export_index as u32,
                    func_index,
                ))?;

        // Panic is impossible, unless the Module structure is invalid.
        let type_id = funcs
            .get(index as usize)
            .unwrap_or_else(|| unreachable!("Module structure is invalid"))
            .type_ref() as usize;

        // Panic is impossible, unless the Module structure is invalid.
        let Type::Function(func_type) = types
            .get(type_id)
            .unwrap_or_else(|| unreachable!("Module structure is invalid"));

        if !(func_type.params().is_empty() && func_type.results().is_empty()) {
            Err(ExportError::InvalidExportFnSignature(export_index as u32))?;
        }

        if !ALLOWED_EXPORTS.contains(&export.field()) {
            Err(ExportError::ExcessExport(export_index as u32))?;
        }

        if REQUIRED_EXPORTS.contains(&export.field()) {
            entry_point_found = true;
        }
    }

    entry_point_found
        .then_some(())
        .ok_or(ExportError::RequiredExportNotFound)
        .map_err(CodeError::Export)
}

pub fn check_imports(module: &Module) -> Result<(), CodeError> {
    let types = module
        .type_section()
        .ok_or(SectionError::NotFound(SectionName::Type))?
        .types();

    let imports = module
        .import_section()
        .ok_or(SectionError::NotFound(SectionName::Import))?
        .entries();

    let syscalls = SyscallName::instrumentable_map();

    let mut visited_imports = BTreeSet::new();
    for (import_index, import) in imports.iter().enumerate() {
        let External::Function(i) = import.external() else {
            continue;
        };

        // Panic is impossible, unless the Module structure is invalid.
        let Type::Function(func_type) = &types
            .get(*i as usize)
            .unwrap_or_else(|| unreachable!("Module structure is invalid"));

        let syscall = syscalls
            .get(import.field())
            .ok_or(ImportError::UnknownImport(import_index as u32))?;

        if !visited_imports.insert(*syscall) {
            Err(ImportError::DuplicateImport(import_index as u32))?;
        }

        let signature = syscall.signature();

        let params = signature
            .params()
            .iter()
            .copied()
            .map(Into::<ValueType>::into);
        let results = signature.results().unwrap_or(&[]);

        if !(params.eq(func_type.params().iter().copied()) && results == func_type.results()) {
            Err(ImportError::InvalidImportFnSignature(import_index as u32))?;
        }
    }

    Ok(())
}

fn get_export_entry_with_index<'a>(
    module: &'a Module,
    name: &str,
) -> Option<(u32, &'a ExportEntry)> {
    module
        .export_section()?
        .entries()
        .iter()
        .enumerate()
        .find_map(|(export_index, export)| {
            (export.field() == name).then_some((export_index as u32, export))
        })
}

fn get_export_entry_mut<'a>(module: &'a mut Module, name: &str) -> Option<&'a mut ExportEntry> {
    module
        .export_section_mut()?
        .entries_mut()
        .iter_mut()
        .find(|export| export.field() == name)
}

fn get_export_global_with_index(module: &Module, name: &str) -> Option<(u32, u32)> {
    let (export_index, export) = get_export_entry_with_index(module, name)?;
    match export.internal() {
        Internal::Global(index) => Some((export_index, *index)),
        _ => None,
    }
}

fn get_export_global_index_mut<'a>(module: &'a mut Module, name: &str) -> Option<&'a mut u32> {
    match get_export_entry_mut(module, name)?.internal_mut() {
        Internal::Global(index) => Some(index),
        _ => None,
    }
}

fn get_init_expr_const_i32(init_expr: &InitExpr) -> Option<i32> {
    match init_expr.code() {
        [Instruction::I32Const(const_i32), Instruction::End] => Some(*const_i32),
        _ => None,
    }
}

fn get_global_entry(module: &Module, global_index: u32) -> Option<&GlobalEntry> {
    module
        .global_section()?
        .entries()
        .get(global_index as usize)
}

struct StackEndInfo {
    pub offset: i32,
    pub is_mutable: bool,
}

fn get_stack_end_info(module: &Module) -> Result<Option<StackEndInfo>, CodeError> {
    let Some((export_index, global_index)) =
        get_export_global_with_index(module, STACK_END_EXPORT_NAME)
    else {
        return Ok(None);
    };

    let entry = get_global_entry(module, global_index).ok_or(ExportError::IncorrectGlobalIndex(
        global_index,
        export_index,
    ))?;

    Ok(Some(StackEndInfo {
        offset: get_init_expr_const_i32(entry.init_expr()).ok_or(StackEndError::Initialization)?,
        is_mutable: entry.global_type().is_mutable(),
    }))
}

/// Check that data segments are not overlapping with stack and are inside static pages.
pub fn check_data_section(module: &Module, check_stack_end: bool) -> Result<(), CodeError> {
    let Some(data_section) = module.data_section() else {
        // No data section - nothing to check.
        return Ok(());
    };

    let static_pages = get_static_pages(module)?;
    let stack_end_offset = match check_stack_end {
        true => get_stack_end_info(module)?.map(|info| info.offset),
        false => None,
    };

    for data_segment in data_section.entries() {
        let data_segment_offset = data_segment
            .offset()
            .as_ref()
            .and_then(get_init_expr_const_i32)
            .ok_or(DataSectionError::Initialization)? as u32;

        if let Some(stack_end_offset) = stack_end_offset {
            // Checks, that each data segment does not overlap the user stack.
            (data_segment_offset >= stack_end_offset as u32)
                .then_some(())
                .ok_or(DataSectionError::UserStackOverlaps(
                    data_segment_offset,
                    stack_end_offset as u32,
                ))?;
        }

        let Some(size) = u32::try_from(data_segment.value().len())
            .map_err(|_| DataSectionError::EndAddressOverflow(data_segment_offset))?
            .checked_sub(1)
        else {
            // Zero size data segment - strange, but allowed.
            continue;
        };

        let data_segment_last_byte_offset = data_segment_offset
            .checked_add(size)
            .ok_or(DataSectionError::EndAddressOverflow(data_segment_offset))?;

        (data_segment_last_byte_offset < static_pages.offset())
            .then_some(())
            .ok_or(DataSectionError::EndAddressOutOfStaticMemory(
                data_segment_offset,
                data_segment_last_byte_offset,
                static_pages.offset(),
            ))?;
    }

    Ok(())
}

pub fn check_and_canonize_gear_stack_end(module: &mut Module) -> Result<(), CodeError> {
    let Some(StackEndInfo {
        offset: stack_end_offset,
        is_mutable: stack_end_global_is_mutable,
    }) = get_stack_end_info(module)?
    else {
        return Ok(());
    };

    // If [STACK_END_EXPORT_NAME] points to mutable global, then make new const global
    // with the same init expr and change the export internal to point to the new global.
    if stack_end_global_is_mutable {
        // Panic is impossible, because we have checked above, that global section exists.
        let global_section = module
            .global_section_mut()
            .unwrap_or_else(|| unreachable!("Cannot find global section"));
        let new_global_index = u32::try_from(global_section.entries().len())
            .map_err(|_| StackEndError::GlobalIndexOverflow)?;
        global_section.entries_mut().push(GlobalEntry::new(
            GlobalType::new(parity_wasm::elements::ValueType::I32, false),
            InitExpr::new(vec![
                Instruction::I32Const(stack_end_offset),
                Instruction::End,
            ]),
        ));

        // Panic is impossible, because we have checked above,
        // that stack end export exists and it points to global.
        get_export_global_index_mut(module, STACK_END_EXPORT_NAME)
            .map(|global_index| *global_index = new_global_index)
            .unwrap_or_else(|| unreachable!("Cannot find stack end export"))
    }

    Ok(())
}

pub fn check_mut_global_exports(module: &Module) -> Result<(), CodeError> {
    if let (Some(export_section), Some(global_section)) =
        (module.export_section(), module.global_section())
    {
        let global_exports =
            export_section
                .entries()
                .iter()
                .enumerate()
                .filter_map(|(export_index, export)| match export.internal() {
                    Internal::Global(index) => Some((export_index as u32, *index)),
                    _ => None,
                });

        for (export_index, global_index) in global_exports {
            if global_section
                .entries()
                .get(global_index as usize)
                .ok_or(ExportError::IncorrectGlobalIndex(
                    global_index,
                    export_index,
                ))?
                .global_type()
                .is_mutable()
            {
                Err(ExportError::MutableGlobalExport(global_index, export_index))?;
            }
        }
    }

    Ok(())
}

pub fn check_start_section(module: &Module) -> Result<(), CodeError> {
    if module.start_section().is_some() {
        log::debug!("Found start section in program code, which is not allowed");
        Err(SectionError::NotSupported(SectionName::Start))?
    } else {
        Ok(())
    }
}
