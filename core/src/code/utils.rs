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
    pages::{WasmPage, WasmPagesAmount},
};
use alloc::collections::BTreeSet;
use gear_wasm_instrument::{
    parity_wasm::elements::{
        ExportEntry, External, GlobalEntry, ImportCountType, InitExpr, Instruction, Internal,
        Module, Type, ValueType,
    },
    SyscallName, STACK_END_EXPORT_NAME,
};

/// Defines maximal permitted count of memory pages.
pub const MAX_WASM_PAGES_AMOUNT: u16 = 512;

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

pub fn get_static_pages(module: &Module) -> Result<WasmPagesAmount, CodeError> {
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
        .map(WasmPagesAmount::try_from)
        .ok_or(MemoryError::EntryNotFound)?
        .map_err(|_| MemoryError::InvalidStaticPageCount)?;

    if static_pages > WasmPagesAmount::from(MAX_WASM_PAGES_AMOUNT) {
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

        let index = func_index.checked_sub(import_count).ok_or(
            ExportError::ExportReferencesToImportFunction(export_index as u32, func_index),
        )?;

        // Panic is impossible, unless the Module structure is invalid.
        let type_id = funcs
            .get(index as usize)
            .unwrap_or_else(|| unreachable!("Module structure is invalid"))
            .type_ref() as usize;

        // Panic is impossible, unless the Module structure is invalid.
        let Type::Function(func_type) = types
            .get(type_id)
            .unwrap_or_else(|| unreachable!("Module structure is invalid"));

        if !ALLOWED_EXPORTS.contains(&export.field()) {
            Err(ExportError::ExcessExport(export_index as u32))?;
        }

        if !(func_type.params().is_empty() && func_type.results().is_empty()) {
            Err(ExportError::InvalidExportFnSignature(export_index as u32))?;
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

fn get_export_global_with_index(module: &Module, name: &str) -> Option<(u32, u32)> {
    let (export_index, export) = get_export_entry_with_index(module, name)?;
    match export.internal() {
        Internal::Global(index) => Some((export_index, *index)),
        _ => None,
    }
}

fn get_init_expr_const_i32(init_expr: &InitExpr) -> Option<i32> {
    match init_expr.code() {
        [Instruction::I32Const(const_i32), Instruction::End] => Some(*const_i32),
        _ => None,
    }
}

fn get_export_global_entry(
    module: &Module,
    export_index: u32,
    global_index: u32,
) -> Result<&GlobalEntry, CodeError> {
    let index = (global_index as usize)
        .checked_sub(module.import_count(ImportCountType::Global))
        .ok_or(ExportError::ExportReferencesToImportGlobal(
            export_index,
            global_index,
        ))?;

    module
        .global_section()
        .and_then(|s| s.entries().get(index))
        .ok_or(ExportError::IncorrectGlobalIndex(global_index, export_index).into())
}

/// Check that data segments are not overlapping with stack and are inside static pages.
pub fn check_data_section(
    module: &Module,
    static_pages: WasmPagesAmount,
    stack_end: Option<WasmPage>,
) -> Result<(), CodeError> {
    let Some(data_section) = module.data_section() else {
        // No data section - nothing to check.
        return Ok(());
    };

    for data_segment in data_section.entries() {
        let data_segment_offset = data_segment
            .offset()
            .as_ref()
            .and_then(get_init_expr_const_i32)
            .ok_or(DataSectionError::Initialization)? as u32;

        if let Some(stack_end_offset) = stack_end.map(|p| p.offset()) {
            // Checks, that each data segment does not overlap the user stack.
            (data_segment_offset >= stack_end_offset)
                .then_some(())
                .ok_or(DataSectionError::GearStackOverlaps(
                    data_segment_offset,
                    stack_end_offset,
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

        ((data_segment_last_byte_offset as u64) < static_pages.offset())
            .then_some(())
            .ok_or(DataSectionError::EndAddressOutOfStaticMemory(
                data_segment_offset,
                data_segment_last_byte_offset,
                static_pages.offset(),
            ))?;
    }

    Ok(())
}

fn get_stack_end_offset(module: &Module) -> Result<Option<u32>, CodeError> {
    let Some((export_index, global_index)) =
        get_export_global_with_index(module, STACK_END_EXPORT_NAME)
    else {
        return Ok(None);
    };

    Ok(Some(
        get_init_expr_const_i32(
            get_export_global_entry(module, export_index, global_index)?.init_expr(),
        )
        .ok_or(StackEndError::Initialization)? as u32,
    ))
}

pub fn check_and_canonize_gear_stack_end(
    module: &mut Module,
    static_pages: WasmPagesAmount,
) -> Result<Option<WasmPage>, CodeError> {
    let Some(stack_end_offset) = get_stack_end_offset(module)? else {
        return Ok(None);
    };

    // Remove stack end export from module.
    // Panic below is impossible, because we have checked above, that export section exists.
    module
        .export_section_mut()
        .unwrap_or_else(|| unreachable!("Cannot find export section"))
        .entries_mut()
        .retain(|export| export.field() != STACK_END_EXPORT_NAME);

    if stack_end_offset % WasmPage::SIZE != 0 {
        return Err(StackEndError::NotAligned(stack_end_offset).into());
    }

    let stack_end = WasmPage::from_offset(stack_end_offset);
    if stack_end > static_pages {
        return Err(StackEndError::OutOfStatic(stack_end_offset, static_pages.offset()).into());
    }

    Ok(Some(stack_end))
}

/// Checks that module:
/// 1) Does not have exports to mutable globals.
/// 2) Does not have exports to imported globals.
/// 3) Does not have exports with incorrect global index.
pub fn check_mut_global_exports(module: &Module) -> Result<(), CodeError> {
    let Some(export_section) = module.export_section() else {
        return Ok(());
    };

    export_section
        .entries()
        .iter()
        .enumerate()
        .filter_map(|(export_index, export)| match export.internal() {
            Internal::Global(index) => Some((export_index as u32, *index)),
            _ => None,
        })
        .try_for_each(|(export_index, global_index)| {
            let entry = get_export_global_entry(module, export_index, global_index)?;
            if entry.global_type().is_mutable() {
                Err(ExportError::MutableGlobalExport(global_index, export_index).into())
            } else {
                Ok(())
            }
        })
}

pub fn check_start_section(module: &Module) -> Result<(), CodeError> {
    if module.start_section().is_some() {
        log::debug!("Found start section in program code, which is not allowed");
        Err(SectionError::NotSupported(SectionName::Start))?
    } else {
        Ok(())
    }
}
