// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use alloc::vec::Vec;
use wasmparser::{
    Data, Element, Encoding, Export, FuncType, Global, Import, MemoryType, Payload, Table, TypeRef,
};

pub struct Module<'a> {
    pub version: u16,
    pub type_section: Option<Vec<FuncType>>,
    pub import_section: Option<Vec<Import<'a>>>,
    pub function_section: Option<Vec<u32>>,
    pub table_section: Option<Vec<Table<'a>>>,
    pub memory_section: Option<Vec<MemoryType>>,
    pub global_section: Option<Vec<Global<'a>>>,
    pub export_section: Option<Vec<Export<'a>>>,
    pub start_section: Option<u32>,
    pub element_section: Option<Vec<Element<'a>>>,
    pub data_section: Option<Vec<Data<'a>>>,
}

impl<'a> Module<'a> {
    pub fn new(code: &'a [u8]) -> Result<Self, wasmparser::BinaryReaderError> {
        let mut version = None;
        let mut type_section = None;
        let mut import_section = None;
        let mut function_section = None;
        let mut table_section = None;
        let mut memory_section = None;
        let mut global_section = None;
        let mut export_section = None;
        let mut start_section = None;
        let mut element_section = None;
        let mut data_section = None;

        let payloads = wasmparser::Parser::new(0).parse_all(code);
        for payload in payloads {
            match payload? {
                Payload::Version {
                    num,
                    encoding,
                    range: _,
                } => {
                    debug_assert_eq!(encoding, Encoding::Module);
                    version = Some(num);
                }
                Payload::TypeSection(section) => {
                    debug_assert!(type_section.is_none());
                    type_section = Some(
                        section
                            .into_iter_err_on_gc_types()
                            .into_iter()
                            .collect::<Result<_, _>>()?,
                    );
                }
                Payload::ImportSection(section) => {
                    debug_assert!(import_section.is_none());
                    import_section = Some(section.into_iter().collect::<Result<_, _>>()?);
                }
                Payload::FunctionSection(section) => {
                    debug_assert!(function_section.is_none());
                    function_section = Some(section.into_iter().collect::<Result<_, _>>()?);
                }
                Payload::TableSection(section) => {
                    debug_assert!(table_section.is_none());
                    table_section = Some(section.into_iter().collect::<Result<_, _>>()?);
                }
                Payload::MemorySection(section) => {
                    debug_assert!(memory_section.is_none());
                    memory_section = Some(section.into_iter().collect::<Result<_, _>>()?);
                }
                Payload::TagSection(_) => {}
                Payload::GlobalSection(section) => {
                    debug_assert!(global_section.is_none());
                    global_section = Some(section.into_iter().collect::<Result<_, _>>()?);
                }
                Payload::ExportSection(section) => {
                    debug_assert!(export_section.is_none());
                    export_section = Some(section.into_iter().collect::<Result<_, _>>()?);
                }
                Payload::StartSection { func, range: _ } => {
                    start_section = Some(func);
                }
                Payload::ElementSection(section) => {
                    debug_assert!(element_section.is_none());
                    element_section = Some(section.into_iter().collect::<Result<_, _>>()?);
                }
                Payload::DataCountSection { count, range: _ } => {
                    data_section = Some(Vec::with_capacity(count as usize));
                }
                Payload::DataSection(section) => {
                    debug_assert!(
                        matches!(data_section, Some(ref vec) if vec.len() == section.count() as usize)
                    );
                    let data_section = data_section
                        .as_mut()
                        .unwrap_or_else(|| unreachable!("data count section missing"));
                    for data in section {
                        let data = data?;
                        data_section.push(data);
                    }
                }
                Payload::CodeSectionStart { .. } => {}
                Payload::CodeSectionEntry(_) => {}
                Payload::CustomSection(_) => {}
                Payload::UnknownSection { .. } => {}
                _ => {}
            }
        }

        Ok(Self {
            version: version.expect("parser error expected because it starts from header parsing"),
            type_section,
            import_section,
            function_section,
            table_section,
            memory_section,
            global_section,
            export_section,
            start_section,
            element_section,
            data_section,
        })
    }

    pub fn import_count(&self, pred: impl Fn(&TypeRef) -> bool) -> u32 {
        self.import_section()
            .map(|imports| imports.iter().filter(|import| pred(&import.ty)).count())
            .unwrap_or(0) as u32
    }

    pub fn type_section(&self) -> Option<&Vec<FuncType>> {
        self.type_section.as_ref()
    }

    pub fn type_section_mut(&mut self) -> Option<&mut Vec<FuncType>> {
        self.type_section.as_mut()
    }

    pub fn import_section(&self) -> Option<&Vec<Import<'a>>> {
        self.import_section.as_ref()
    }

    pub fn import_section_mut(&mut self) -> Option<&mut Vec<Import<'a>>> {
        self.import_section.as_mut()
    }

    pub fn function_section(&self) -> Option<&Vec<u32>> {
        self.function_section.as_ref()
    }

    pub fn function_section_mut(&mut self) -> Option<&mut Vec<u32>> {
        self.function_section.as_mut()
    }

    pub fn table_section(&self) -> Option<&Vec<Table<'a>>> {
        self.table_section.as_ref()
    }

    pub fn table_section_mut(&mut self) -> Option<&mut Vec<Table<'a>>> {
        self.table_section.as_mut()
    }

    pub fn memory_section(&self) -> Option<&Vec<MemoryType>> {
        self.memory_section.as_ref()
    }

    pub fn memory_section_mut(&mut self) -> Option<&mut Vec<MemoryType>> {
        self.memory_section.as_mut()
    }

    pub fn global_section(&self) -> Option<&Vec<Global<'a>>> {
        self.global_section.as_ref()
    }

    pub fn global_section_mut(&mut self) -> Option<&mut Vec<Global<'a>>> {
        self.global_section.as_mut()
    }

    pub fn export_section(&self) -> Option<&Vec<Export<'a>>> {
        self.export_section.as_ref()
    }

    pub fn export_section_mut(&mut self) -> Option<&mut Vec<Export<'a>>> {
        self.export_section.as_mut()
    }

    pub fn start_section(&self) -> Option<u32> {
        self.start_section
    }

    pub fn element_section(&self) -> Option<&Vec<Element>> {
        self.element_section.as_ref()
    }

    pub fn element_section_mut(&mut self) -> Option<&mut Vec<Element<'a>>> {
        self.element_section.as_mut()
    }

    pub fn data_section(&self) -> Option<&Vec<Data<'a>>> {
        self.data_section.as_ref()
    }

    pub fn data_section_mut(&mut self) -> Option<&mut Vec<Data<'a>>> {
        self.data_section.as_mut()
    }
}
