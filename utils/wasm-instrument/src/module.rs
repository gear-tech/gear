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
use wasm_encoder::{
    reencode,
    reencode::{Reencode, RoundtripReencoder},
    RefType,
};
use wasmparser::{
    BinaryReaderError, Data, ElementKind, Encoding, Export, ExternalKind, FuncType, FunctionBody,
    GlobalType, Import, MemoryType, Operator, Payload, Table, TypeRef, ValType,
};

pub type Result<T, E = Error> = core::result::Result<T, E>;

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
    #[display(fmt = "Binary reader error: {}", _0)]
    BinaryReader(BinaryReaderError),
    #[display(fmt = "Reencode error: {}", _0)]
    Reencode(reencode::Error),
}

#[derive(Default)]
pub struct ConstExpr<'a> {
    pub instructions: Vec<Operator<'a>>,
}

impl<'a> ConstExpr<'a> {
    fn new(expr: wasmparser::ConstExpr<'a>) -> Result<Self> {
        let mut instructions = Vec::new();
        let mut ops = expr.get_operators_reader();
        while !ops.is_end_then_eof() {
            instructions.push(ops.read()?);
        }

        Ok(Self { instructions })
    }
}

pub struct Global<'a> {
    pub ty: GlobalType,
    pub init_expr: ConstExpr<'a>,
}

impl<'a> Global<'a> {
    fn new(global: wasmparser::Global<'a>) -> Result<Self> {
        Ok(Self {
            ty: global.ty,
            init_expr: ConstExpr::new(global.init_expr)?,
        })
    }
}

pub enum ElementItems<'a> {
    Functions(Vec<u32>),
    Expressions(RefType, Vec<ConstExpr<'a>>),
}

impl<'a> ElementItems<'a> {
    fn new(elements: wasmparser::ElementItems<'a>) -> Result<Self> {
        Ok(match elements {
            wasmparser::ElementItems::Functions(f) => {
                let mut funcs = Vec::new();
                for func in f {
                    funcs.push(func?);
                }
                Self::Functions(funcs)
            }
            wasmparser::ElementItems::Expressions(ty, e) => {
                let mut exprs = Vec::new();
                for expr in e {
                    exprs.push(ConstExpr::new(expr?)?);
                }
                Self::Expressions(RoundtripReencoder.ref_type(ty)?, exprs)
            }
        })
    }
}

pub struct Element<'a> {
    pub kind: ElementKind<'a>,
    pub items: ElementItems<'a>,
}

impl<'a> Element<'a> {
    fn new(element: wasmparser::Element<'a>) -> Result<Self> {
        Ok(Self {
            kind: element.kind,
            items: ElementItems::new(element.items)?,
        })
    }
}

#[derive(Debug, Default)]
pub struct Function<'a> {
    pub locals: Vec<(u32, ValType)>,
    pub instructions: Vec<Operator<'a>>,
}

impl<'a> Function<'a> {
    fn from_entry(func: FunctionBody<'a>) -> Result<Self> {
        let mut locals = Vec::new();
        for pair in func.get_locals_reader()? {
            let (cnt, ty) = pair?;
            locals.push((cnt, ty));
        }

        let mut instructions = Vec::new();
        let mut reader = func.get_operators_reader()?;
        while !reader.eof() {
            instructions.push(reader.read()?);
        }

        Ok(Self {
            locals,
            instructions,
        })
    }
}

#[derive(Debug, Default)]
pub struct ModuleBuilder<'a> {
    module: Module<'a>,
}

impl<'a> ModuleBuilder<'a> {
    pub fn from_module(module: Module<'a>) -> Self {
        Self { module }
    }

    pub fn rewrite_sections_after_insertion(
        mut self,
        inserted_index: u32,
        inserted_count: u32,
    ) -> Result<Self, Self> {
        if inserted_count == 0 {
            return Err(self);
        }

        if let Some(section) = self.module.code_section_mut() {
            for func in section {
                for instruction in &mut func.instructions {
                    if let Operator::Call { function_index } = instruction {
                        if *function_index >= inserted_index {
                            *function_index += inserted_count
                        }
                    }
                }
            }
        }

        if let Some(section) = self.module.export_section_mut() {
            for export in section {
                if let ExternalKind::Func = export.kind {
                    if export.index >= inserted_index {
                        export.index += inserted_count
                    }
                }
            }
        }

        if let Some(section) = self.module.element_section_mut() {
            for segment in section {
                // update all indirect call addresses initial values
                match &mut segment.items {
                    ElementItems::Functions(funcs) => {
                        for func_index in funcs.iter_mut() {
                            if *func_index >= inserted_index {
                                *func_index += inserted_count
                            }
                        }
                    }
                    ElementItems::Expressions(_ty, exprs) => {
                        for expr in exprs {
                            for instruction in &mut expr.instructions {
                                if let Operator::Call { function_index } = instruction {
                                    if *function_index >= inserted_index {
                                        *function_index += inserted_count
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(start_idx) = &mut self.module.start_section {
            if *start_idx >= inserted_index {
                *start_idx += inserted_count
            }
        }

        if let Some(section) = self.module.function_section_mut() {
            for func_idx in section {
                if *func_idx >= inserted_index {
                    *func_idx += inserted_count;
                }
            }
        }

        Ok(self)
    }

    pub fn build(self) -> Module<'a> {
        self.module
    }

    pub fn as_module(&self) -> &Module<'a> {
        &self.module
    }

    fn type_section(&mut self) -> &mut TypeSection {
        self.module.type_section.get_or_insert_with(Vec::new)
    }

    fn import_section(&mut self) -> &mut Vec<Import<'a>> {
        self.module.import_section.get_or_insert_with(Vec::new)
    }

    fn global_section(&mut self) -> &mut Vec<Global<'a>> {
        self.module.global_section.get_or_insert_with(Vec::new)
    }

    fn export_section(&mut self) -> &mut Vec<Export<'a>> {
        self.module.export_section.get_or_insert_with(Vec::new)
    }

    fn code_section(&mut self) -> &mut CodeSection<'a> {
        self.module.code_section.get_or_insert_with(Vec::new)
    }

    pub fn push_type(&mut self, ty: FuncType) -> u32 {
        self.type_section().push(ty);
        self.type_section().len() as u32 - 1
    }

    pub fn push_import(&mut self, import: Import<'a>) {
        self.import_section().push(import);
    }

    pub fn push_global(&mut self, global: Global<'a>) -> u32 {
        self.global_section().push(global);
        self.global_section().len() as u32 - 1
    }

    pub fn push_export(&mut self, export: Export<'a>) {
        self.export_section().push(export);
    }

    pub fn push_function(&mut self, function: Function<'a>) {
        self.code_section().push(function);
    }
}

pub type TypeSection = Vec<FuncType>;
pub type FuncSection = Vec<u32>;
pub type CodeSection<'a> = Vec<Function<'a>>;

#[derive(derive_more::DebugCustom, Default)]
#[debug(fmt = "Module {{ .. }}")]
pub struct Module<'a> {
    pub type_section: Option<TypeSection>,
    pub import_section: Option<Vec<Import<'a>>>,
    pub function_section: Option<FuncSection>,
    pub table_section: Option<Vec<Table<'a>>>,
    pub memory_section: Option<Vec<MemoryType>>,
    pub global_section: Option<Vec<Global<'a>>>,
    pub export_section: Option<Vec<Export<'a>>>,
    pub start_section: Option<u32>,
    pub element_section: Option<Vec<Element<'a>>>,
    pub data_section: Option<Vec<Data<'a>>>,
    pub code_section: Option<CodeSection<'a>>,
}

impl<'a> Module<'a> {
    pub fn new(code: &'a [u8]) -> Result<Self> {
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
        let mut code_section = None;

        let payloads = wasmparser::Parser::new(0).parse_all(code);
        for payload in payloads {
            match payload? {
                Payload::Version {
                    num: _,
                    encoding,
                    range: _,
                } => {
                    debug_assert_eq!(encoding, Encoding::Module);
                }
                Payload::TypeSection(section) => {
                    debug_assert!(type_section.is_none());
                    type_section = Some(
                        section
                            .into_iter_err_on_gc_types()
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
                    global_section = Some(
                        section
                            .into_iter()
                            .map(|element| element.map_err(Into::into).and_then(Global::new))
                            .collect::<Result<_, _>>()?,
                    );
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
                    element_section = Some(
                        section
                            .into_iter()
                            .map(|element| element.map_err(Into::into).and_then(Element::new))
                            .collect::<Result<Vec<_>>>()?,
                    );
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
                Payload::CodeSectionStart {
                    count,
                    range: _,
                    size: _,
                } => {
                    code_section = Some(Vec::with_capacity(count as usize));
                }
                Payload::CodeSectionEntry(entry) => {
                    code_section
                        .as_mut()
                        .expect("code section start missing")
                        .push(Function::from_entry(entry)?);
                }
                Payload::CustomSection(_) => {}
                Payload::UnknownSection { .. } => {}
                _ => {}
            }
        }

        Ok(Self {
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
            code_section,
        })
    }

    pub fn import_count(&self, pred: impl Fn(&TypeRef) -> bool) -> usize {
        self.import_section()
            .map(|imports| imports.iter().filter(|import| pred(&import.ty)).count())
            .unwrap_or(0)
    }

    pub fn functions_space(&self) -> usize {
        self.import_count(|ty| matches!(ty, TypeRef::Func(_)))
            + self
                .function_section()
                .map(|section| section.len())
                .unwrap_or(0)
    }

    pub fn globals_space(&self) -> usize {
        self.import_count(|ty| matches!(ty, TypeRef::Global(_)))
            + self
                .global_section()
                .map(|section| section.len())
                .unwrap_or(0)
    }

    // Getters //

    pub fn type_section(&self) -> Option<&TypeSection> {
        self.type_section.as_ref()
    }

    pub fn type_section_mut(&mut self) -> Option<&mut TypeSection> {
        self.type_section.as_mut()
    }

    pub fn import_section(&self) -> Option<&Vec<Import<'a>>> {
        self.import_section.as_ref()
    }

    pub fn import_section_mut(&mut self) -> Option<&mut Vec<Import<'a>>> {
        self.import_section.as_mut()
    }

    pub fn function_section(&self) -> Option<&FuncSection> {
        self.function_section.as_ref()
    }

    pub fn function_section_mut(&mut self) -> Option<&mut FuncSection> {
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

    pub fn start_section_mut(&mut self) -> Option<&mut u32> {
        self.start_section.as_mut()
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

    pub fn code_section(&self) -> Option<&Vec<Function<'a>>> {
        self.code_section.as_ref()
    }

    pub fn code_section_mut(&mut self) -> Option<&mut CodeSection<'a>> {
        self.code_section.as_mut()
    }
}
