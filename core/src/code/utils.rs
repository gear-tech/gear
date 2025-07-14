// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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
    code::{GENERIC_OS_PAGE_SIZE, errors::*},
    env::WasmEntryPoint,
    message::DispatchKind,
    pages::{WasmPage, WasmPagesAmount},
};
use alloc::collections::BTreeSet;
use gear_wasm_instrument::{
    ConstExpr, ElementItems, Export, Global, Instruction, Module, STACK_END_EXPORT_NAME,
    SyscallName,
};
use wasmparser::{ExternalKind, Payload, TypeRef, ValType};

/// Defines maximal permitted count of memory pages.
pub const MAX_WASM_PAGES_AMOUNT: u16 = u16::MAX / 2 + 1; // 2GB
/// Reference type size in bytes.
pub(crate) const REF_TYPE_SIZE: u32 = 4;

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
        .import_section
        .as_ref()
        .ok_or(SectionError::NotFound(SectionName::Import))?
        .iter()
        .find_map(|entry| match entry.ty {
            TypeRef::Memory(mem_ty) => Some(mem_ty.initial as u32),
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
        .export_section
        .as_ref()
        .expect("Exports section has been checked for already")
    {
        if let ExternalKind::Func = entry.kind
            && let Some(entry) = DispatchKind::try_from_entry(&entry.name)
        {
            entries.insert(entry);
        }
    }

    entries
}

pub fn check_exports(module: &Module) -> Result<(), CodeError> {
    let types = module
        .type_section
        .as_ref()
        .ok_or(SectionError::NotFound(SectionName::Type))?;

    let funcs = module
        .function_section
        .as_ref()
        .ok_or(SectionError::NotFound(SectionName::Function))?;

    let import_count = module.import_count(|ty| matches!(ty, TypeRef::Func(_)));

    let exports = module
        .export_section
        .as_ref()
        .ok_or(SectionError::NotFound(SectionName::Export))?;

    let mut entry_point_found = false;
    for (export_index, export) in exports.iter().enumerate() {
        let ExternalKind::Func = export.kind else {
            continue;
        };

        let index = export.index.checked_sub(import_count as u32).ok_or(
            ExportError::ExportReferencesToImportFunction(export_index as u32, export.index),
        )?;

        // Panic is impossible, unless the Module structure is invalid.
        let type_id = funcs
            .get(index as usize)
            .copied()
            .unwrap_or_else(|| unreachable!("Module structure is invalid"))
            as usize;

        // Panic is impossible, unless the Module structure is invalid.
        let func_type = types
            .get(type_id)
            .unwrap_or_else(|| unreachable!("Module structure is invalid"));

        if !ALLOWED_EXPORTS.contains(&&*export.name) {
            Err(ExportError::ExcessExport(export_index as u32))?;
        }

        if !(func_type.params().is_empty() && func_type.results().is_empty()) {
            Err(ExportError::InvalidExportFnSignature(export_index as u32))?;
        }

        if REQUIRED_EXPORTS.contains(&&*export.name) {
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
        .type_section
        .as_ref()
        .ok_or(SectionError::NotFound(SectionName::Type))?;

    let imports = module
        .import_section
        .as_ref()
        .ok_or(SectionError::NotFound(SectionName::Import))?;

    let syscalls = SyscallName::instrumentable_map();

    let mut visited_imports = BTreeSet::new();

    for (import_index, import) in imports.iter().enumerate() {
        let import_index: u32 = import_index
            .try_into()
            .unwrap_or_else(|_| unreachable!("Import index should fit in u32"));

        match import.ty {
            TypeRef::Func(i) => {
                // Panic is impossible, unless the Module structure is invalid.
                let &func_type = &types
                    .get(i as usize)
                    .unwrap_or_else(|| unreachable!("Module structure is invalid"));

                let syscall = syscalls
                    .get(import.name.as_ref())
                    .ok_or(ImportError::UnknownImport(import_index))?;

                if !visited_imports.insert(*syscall) {
                    Err(ImportError::DuplicateImport(import_index))?;
                }

                let signature = syscall.signature();
                let signature_func_type = signature.func_type();

                if signature_func_type != *func_type {
                    Err(ImportError::InvalidImportFnSignature(import_index))?;
                }
            }
            TypeRef::Global(_) => Err(ImportError::UnexpectedImportKind {
                kind: &"Global",
                index: import_index,
            })?,
            TypeRef::Table(_) => Err(ImportError::UnexpectedImportKind {
                kind: &"Table",
                index: import_index,
            })?,
            _ => continue,
        }
    }

    Ok(())
}

fn get_export_entry_with_index<'a>(module: &'a Module, name: &str) -> Option<(u32, &'a Export)> {
    module
        .export_section
        .as_ref()?
        .iter()
        .enumerate()
        .find_map(|(export_index, export)| {
            (export.name == name).then_some((export_index as u32, export))
        })
}

fn get_export_global_with_index(module: &Module, name: &str) -> Option<(u32, u32)> {
    let (export_index, export) = get_export_entry_with_index(module, name)?;
    match export.kind {
        ExternalKind::Global => Some((export_index, export.index)),
        _ => None,
    }
}

fn get_init_expr_const_i32(init_expr: &ConstExpr) -> Option<i32> {
    match init_expr.instructions.as_slice() {
        [Instruction::I32Const(value)] => Some(*value),
        _ => None,
    }
}

fn get_export_global_entry(
    module: &Module,
    export_index: u32,
    global_index: u32,
) -> Result<&Global, CodeError> {
    let index = global_index
        .checked_sub(module.import_count(|ty| matches!(ty, TypeRef::Global(_))) as u32)
        .ok_or(ExportError::ExportReferencesToImportGlobal(
            export_index,
            global_index,
        ))? as usize;

    module
        .global_section
        .as_ref()
        .and_then(|s| s.get(index))
        .ok_or(ExportError::IncorrectGlobalIndex(global_index, export_index).into())
}

/// Check that data segments are not overlapping with stack and are inside static pages.
pub fn check_data_section(
    module: &Module,
    static_pages: WasmPagesAmount,
    stack_end: Option<WasmPage>,
    data_section_amount_limit: Option<u32>,
) -> Result<(), CodeError> {
    let Some(data_section) = &module.data_section else {
        // No data section - nothing to check.
        return Ok(());
    };

    // Check that data segments amount does not exceed the limit.
    if let Some(data_segments_amount_limit) = data_section_amount_limit {
        let number_of_data_segments = data_section.len() as u32;
        if number_of_data_segments > data_segments_amount_limit {
            Err(DataSectionError::DataSegmentsAmountLimit {
                limit: data_segments_amount_limit,
                actual: number_of_data_segments,
            })?;
        }
    }

    for data_segment in data_section {
        let data_segment_offset = get_init_expr_const_i32(&data_segment.offset_expr)
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

        let Some(size) = u32::try_from(data_segment.data.len())
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
            &get_export_global_entry(module, export_index, global_index)?.init_expr,
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
        .export_section
        .as_mut()
        .unwrap_or_else(|| unreachable!("Cannot find export section"))
        .retain(|export| export.name != STACK_END_EXPORT_NAME);

    if !stack_end_offset.is_multiple_of(WasmPage::SIZE) {
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
    let Some(export_section) = &module.export_section else {
        return Ok(());
    };

    export_section
        .iter()
        .enumerate()
        .filter_map(|(export_index, export)| match export.kind {
            ExternalKind::Global => Some((export_index as u32, export.index)),
            _ => None,
        })
        .try_for_each(|(export_index, global_index)| {
            let entry = get_export_global_entry(module, export_index, global_index)?;
            if entry.ty.mutable {
                Err(ExportError::MutableGlobalExport(global_index, export_index).into())
            } else {
                Ok(())
            }
        })
}

pub fn check_start_section(module: &Module) -> Result<(), CodeError> {
    if module.start_section.is_some() {
        log::debug!("Found start section in program code, which is not allowed");
        Err(SectionError::NotSupported(SectionName::Start))?
    } else {
        Ok(())
    }
}

/// Calculates the instantiated data section size based on the number of heuristic memory pages (see `GENERIC_OS_PAGE_SIZE`).
/// That is, the size of the instantiated data section is the size of the section after the module is instantiated
/// in the executor's memory. Additionally, the number of heuristic pages used during instantiation is considered,
/// as each page contributes to the total weight during instantiation.
pub fn get_data_section_size(module: &Module) -> Result<u32, CodeError> {
    let Some(data_section) = &module.data_section else {
        // No data section
        return Ok(0);
    };

    let mut used_pages = BTreeSet::new();
    for data_segment in data_section {
        let data_segment_offset = get_init_expr_const_i32(&data_segment.offset_expr)
            .ok_or(DataSectionError::Initialization)? as u32;
        let data_segment_start = data_segment_offset / GENERIC_OS_PAGE_SIZE;

        let data_segment_size = data_segment.data.len() as u32;

        if data_segment_size == 0 {
            // Zero size data segment
            continue;
        }

        let data_segment_end = data_segment_offset // We should use `offset` here and not `start`
                .saturating_add(data_segment_size.saturating_sub(1)) // Round up to the nearest whole number
                / GENERIC_OS_PAGE_SIZE;

        used_pages.extend(data_segment_start..=data_segment_end);
    }

    Ok(used_pages.len() as u32 * GENERIC_OS_PAGE_SIZE)
}

/// Calculates the amount of bytes in the global section will be initialized during module instantiation.
pub fn get_instantiated_global_section_size(module: &Module) -> Result<u32, CodeError> {
    let Some(global_section) = &module.global_section else {
        // No element section
        return Ok(0);
    };

    Ok(global_section.iter().fold(0, |total_bytes, global| {
        let value_size = match global.ty.content_type {
            ValType::I32 => size_of::<i32>(),
            ValType::I64 => size_of::<i64>(),
            ValType::F32 | ValType::F64 | ValType::V128 | ValType::Ref(_) => {
                unreachable!("f32/64, SIMD and reference types are not supported")
            }
        } as u32;
        total_bytes.saturating_add(value_size)
    }))
}

/// Calculates the amount of bytes in the table section that will be allocated during module instantiation.
pub fn get_instantiated_table_section_size(module: &Module) -> u32 {
    let Some(table) = &module.table_section else {
        return 0;
    };

    // Tables may hold only reference types, which are 4 bytes long.
    (table.ty.initial as u32).saturating_mul(REF_TYPE_SIZE)
}

/// Calculates the amount of bytes in the table/element section that will be initialized during module instantiation.
pub fn get_instantiated_element_section_size(module: &Module) -> Result<u32, CodeError> {
    if module.table_section.is_none() {
        return Ok(0);
    }

    let Some(element_section) = &module.element_section else {
        // No element section
        return Ok(0);
    };

    Ok(element_section.iter().fold(0, |total_bytes, segment| {
        let count = match &segment.items {
            ElementItems::Functions(section) => section.len(),
        } as u32;
        // Tables may hold only reference types, which are 4 bytes long.
        total_bytes.saturating_add(count.saturating_mul(REF_TYPE_SIZE))
    }))
}

pub struct CodeTypeSectionSizes {
    pub code_section: u32,
    pub type_section: u32,
}

// Calculate the size of the code and type sections in bytes.
pub fn get_code_type_sections_sizes(code_bytes: &[u8]) -> Result<CodeTypeSectionSizes, CodeError> {
    let mut code_section_size = 0;
    let mut type_section_size = 0;

    let parser = wasmparser::Parser::new(0);

    for item in parser.parse_all(code_bytes) {
        let item = item.map_err(CodeError::Validation)?;
        match item {
            Payload::CodeSectionStart { size, .. } => {
                code_section_size = size;
            }
            Payload::TypeSection(t) => {
                type_section_size += t.range().len() as u32;
            }
            _ => {}
        }
    }

    Ok(CodeTypeSectionSizes {
        code_section: code_section_size,
        type_section: type_section_size,
    })
}
