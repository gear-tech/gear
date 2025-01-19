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

use alloc::{
    borrow::Cow,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::convert::Infallible;
use wasm_encoder::{
    reencode,
    reencode::{Reencode, RoundtripReencoder},
};
use wasmparser::{
    BinaryReaderError, Encoding, ExternalKind, FuncType, FunctionBody, GlobalType, MemoryType,
    Payload, RefType, TableType, TypeRef, ValType,
};

macro_rules! define_for_each_instruction_helper {
    ($dollar:tt;
        proposals { $($proposals:ident,)+ }
        rewrite_fields { $( $ops:ident { $($args:ident: $argsty:ty),* }, )+ }
        forbidden_instructions { $($forbidden_instructions:ident,)+ }
    ) => {
        macro_rules! define_for_each_instruction {
            ($dollar ( @$dollar proposal:ident $dollar op:ident $dollar ({ $dollar ($dollar arg:ident: $dollar argty:ty),* })? => $dollar visit:ident ($dollar ($dollar ann:tt)*) )*) => {
                define_for_each_instruction!(inner $dollar ( @$dollar proposal $dollar op $dollar ({ $dollar ($dollar arg: $dollar argty),* })? )* accum @accum2);
            };
            // ACCUM: skip forbidden instructions
            $(
                (
                    inner
                    @$dollar proposal:ident $forbidden_instructions $dollar ({ $dollar ($dollar arg:ident: $dollar argty:ty),* })?
                    $dollar ( @$dollar proposals:ident $dollar ops:ident $dollar ({ $dollar ($dollar args:ident: $dollar argsty:ty),* })? )*
                    accum
                    $dollar ( $dollar ops_accum:ident $dollar ({ $dollar ($dollar args_accum:ident: $dollar argsty_accum:ty),* })? )*
                    @accum2
                ) => {
                    define_for_each_instruction!(
                        inner
                        $dollar ( @$dollar proposals $dollar ops $dollar ({ $dollar ($dollar args: $dollar argsty),* })? )*
                        accum
                        $dollar ( $dollar ops_accum $dollar ({ $dollar ($dollar args_accum: $dollar argsty_accum),* })? )*
                        @accum2
                    );
                };
            )+
            // ACCUM: use only specific proposals
            $(
                (
                    inner
                    @$proposals $dollar op:ident $dollar ({ $dollar ($dollar arg:ident: $dollar argty:ty),* })?
                    $dollar ( @$dollar proposals:ident $dollar ops:ident $dollar ({ $dollar ($dollar args:ident: $dollar argsty:ty),* })? )*
                    accum
                    $dollar ( $dollar ops_accum:ident $dollar ({ $dollar ($dollar args_accum:ident: $dollar argsty_accum:ty),* })? )*
                    @accum2
                ) => {
                    define_for_each_instruction!(
                        inner
                        $dollar ( @$dollar proposals $dollar ops $dollar ({ $dollar ($dollar args: $dollar argsty),* })? )*
                        accum
                        $dollar op $dollar ({ $dollar ( $dollar arg: $dollar argty ),* })?
                        $dollar ( $dollar ops_accum $dollar ({ $dollar ($dollar args_accum: $dollar argsty_accum),* })? )*
                        @accum2
                    );
                };
            )+
            // ACCUM: skip rest instructions
            (
                inner
                @$dollar proposal:ident $dollar op:ident $dollar ({ $dollar ($dollar arg:ident: $dollar argty:ty),* })?
                $dollar ( @$dollar proposals:ident $dollar ops:ident $dollar ({ $dollar ($dollar args:ident: $dollar argsty:ty),* })? )*
                accum
                $dollar ( $dollar ops_accum:ident $dollar ({ $dollar ($dollar args_accum:ident: $dollar argsty_accum:ty),* })? )*
                @accum2
            ) => {
                define_for_each_instruction!(
                    inner
                    $dollar ( @$dollar proposals $dollar ops $dollar ({ $dollar ($dollar args: $dollar argsty),* })? )*
                    accum
                    $dollar ( $dollar ops_accum $dollar ({ $dollar ($dollar args_accum: $dollar argsty_accum),* })? )*
                    @accum2
                );
            };
            // @accum2: rewrite instructions fields
            $(
                (
                    inner
                    accum
                    $dollar op:ident { $($args: $dollar argty:ty),* }
                    $dollar ( $dollar ops:ident $dollar ({ $dollar ($dollar args:ident: $dollar argsty:ty),* })? )*
                    @accum2
                    $dollar ( $dollar ops_accum:ident $dollar ({ $dollar ($dollar args_accum:ident: $dollar argsty_accum:ty),* })? )*
                ) => {
                    define_for_each_instruction!(
                        inner
                        accum
                        $dollar ( $dollar ops $dollar ({ $dollar ($dollar args: $dollar argsty),* })? )*
                        @accum2
                        $dollar op { $($args: $argsty),* }
                        $dollar ( $dollar ops_accum $dollar ({ $dollar ($dollar args_accum: $dollar argsty_accum),* })? )*
                    );
                };
            )+
            // @accum2: accumulate rest instructions from `accum` to `@accum2`
            (
                inner
                accum
                $dollar op:ident $dollar ({ $dollar ($dollar arg:ident: $dollar argty:ty),* })?
                $dollar ( $dollar ops:ident $dollar ({ $dollar ($dollar args:ident: $dollar argsty:ty),* })? )*
                @accum2
                $dollar ( $dollar ops_accum:ident $dollar ({ $dollar ($dollar args_accum:ident: $dollar argsty_accum:ty),* })? )*
            ) => {
                define_for_each_instruction!(
                    inner
                    accum
                    $dollar ( $dollar ops $dollar ({ $dollar ($dollar args: $dollar argsty),* })? )*
                    @accum2
                    $dollar op $dollar ({ $dollar ( $dollar arg: $dollar argty ),* })?
                    $dollar ( $dollar ops_accum $dollar ({ $dollar ($dollar args_accum: $dollar argsty_accum),* })? )*
                );
            };
            (
                inner
                accum
                @accum2
                $dollar ( $dollar op:ident $dollar ({ $dollar ($dollar arg:ident: $dollar argty:ty),* })? )*
            ) => {
                #[macro_export]
                macro_rules! for_each_instruction {
                    ($dollar mac:ident) => {
                        $dollar mac! {
                            $dollar ( $dollar op $dollar ({ $dollar ($dollar arg: $dollar argty),* })? )*
                        }
                    };
                }
            };
        }
    };
}

define_for_each_instruction_helper!($;
    proposals {
        mvp,
        sign_extension,
    }
    rewrite_fields {
        BrTable { targets: BrTable },
        AnyMemArgInstruction { memarg: MemArg },
    }
    forbidden_instructions {
        F64ReinterpretI64,
        F32ReinterpretI32,
        I64ReinterpretF64,
        I32ReinterpretF32,
        F64PromoteF32,
        F64ConvertI64U,
        F64ConvertI64S,
        F64ConvertI32U,
        F64ConvertI32S,
        F32DemoteF64,
        F32ConvertI64U,
        F32ConvertI64S,
        F32ConvertI32U,
        F32ConvertI32S,
        I64TruncF64U,
        I64TruncF64S,
        I64TruncF32U,
        I64TruncF32S,
        I32TruncF64U,
        I32TruncF64S,
        I32TruncF32U,
        I32TruncF32S,
        F64Copysign,
        F64Max,
        F64Min,
        F64Div,
        F64Mul,
        F64Sub,
        F64Add,
        F64Sqrt,
        F64Nearest,
        F64Trunc,
        F64Floor,
        F64Ceil,
        F64Neg,
        F64Abs,
        F32Copysign,
        F32Max,
        F32Min,
        F32Div,
        F32Mul,
        F32Sub,
        F32Add,
        F32Sqrt,
        F32Nearest,
        F32Trunc,
        F32Floor,
        F32Ceil,
        F32Neg,
        F32Abs,
        F64Ge,
        F64Le,
        F64Gt,
        F64Lt,
        F64Ne,
        F64Eq,
        F32Ge,
        F32Le,
        F32Gt,
        F32Lt,
        F32Ne,
        F32Eq,
        F64Const,
        F32Const,
        F64Store,
        F32Store,
        F64Load,
        F32Load,
    }
);

wasmparser::for_each_operator!(define_for_each_instruction);

macro_rules! define_instruction {
    ($( $op:ident $({ $($arg:ident: $argty:ty),* })? )*) => {
        #[derive(Debug, Clone, Eq, PartialEq)]
        pub enum Instruction {
            $( $op $({ $($arg: $argty),* })? ),*
        }

        impl Instruction {
            fn new(op: wasmparser::Operator) -> Result<Self> {
                match op {
                    $(
                        wasmparser::Operator::$op $({ $($arg),* })? => {
                            Ok(Self::$op $({ $($arg: <_>::try_from($arg)?),* })?)
                        }
                    )*
                    op => Err(ModuleError::UnsupportedInstruction(format!("{op:?}"))),
                }
            }

            fn reencode(&self) -> Result<wasm_encoder::Instruction> {
                Ok(match self {
                    $(
                        Self::$op $({ $($arg),* })? => {
                            $(
                                $(let $arg = define_instruction!(@arg $arg $arg);)*
                            )?
                            define_instruction!(@build $op $($($arg)*)?)
                        }
                    )*
                })
            }
        }
    };
    (@arg $arg:ident blockty) => (RoundtripReencoder.block_type(*$arg)?);
    (@arg $arg:ident targets) => ((
        ($arg).targets.clone().into(),
        ($arg).default,
    ));
    (@arg $arg:ident memarg) => ((*$arg).reencode());
    (@arg $arg:ident $_arg:ident) => (*$arg);

    (@build $op:ident) => (wasm_encoder::Instruction::$op);
    (@build BrTable $arg:ident) => (wasm_encoder::Instruction::BrTable($arg.0, $arg.1));
    (@build I32Const $arg:ident) => (wasm_encoder::Instruction::I32Const($arg));
    (@build I64Const $arg:ident) => (wasm_encoder::Instruction::I64Const($arg));
    (@build F32Const $arg:ident) => (wasm_encoder::Instruction::F32Const(f32::from_bits($arg.bits())));
    (@build F64Const $arg:ident) => (wasm_encoder::Instruction::F64Const(f64::from_bits($arg.bits())));
    (@build $op:ident $arg:ident) => (wasm_encoder::Instruction::$op($arg));
    (@build $op:ident $($arg:ident)*) => (wasm_encoder::Instruction::$op { $($arg),* });
}

for_each_instruction!(define_instruction);

impl Instruction {
    /// Is instruction forbidden to be used by user but allowed to be used by instrumentation stage.
    pub fn is_user_forbidden(&self) -> bool {
        matches!(self, Self::MemoryGrow { .. })
    }
}

pub type Result<T, E = ModuleError> = core::result::Result<T, E>;

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum ModuleError {
    #[from]
    #[display(fmt = "Binary reader error: {}", _0)]
    BinaryReader(BinaryReaderError),
    #[from]
    #[display(fmt = "Reencode error: {}", _0)]
    Reencode(reencode::Error),
    #[from]
    #[display(fmt = "Int conversion error: {}", _0)]
    TryFromInt(core::num::TryFromIntError),
    #[display(fmt = "Unsupported instruction: {}", _0)]
    UnsupportedInstruction(String),
}

impl core::error::Error for ModuleError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            ModuleError::BinaryReader(e) => Some(e),
            ModuleError::Reencode(e) => Some(e),
            ModuleError::TryFromInt(e) => Some(e),
            ModuleError::UnsupportedInstruction(_) => None,
        }
    }
}

impl From<Infallible> for ModuleError {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MemArg {
    /// The expected alignment of the instruction's dynamic address operand
    /// (expressed the exponent of a power of two).
    pub align: u8,
    /// A static offset to add to the instruction's dynamic address operand.
    pub offset: u32,
}

impl TryFrom<wasmparser::MemArg> for MemArg {
    type Error = ModuleError;

    fn try_from(
        wasmparser::MemArg {
            align,
            max_align: _,
            offset,
            memory,
        }: wasmparser::MemArg,
    ) -> Result<Self, Self::Error> {
        debug_assert_eq!(memory, 0);
        Ok(Self {
            align,
            offset: offset.try_into()?,
        })
    }
}

impl MemArg {
    fn reencode(self) -> wasm_encoder::MemArg {
        wasm_encoder::MemArg {
            offset: self.offset as u64,
            align: self.align as u32,
            memory_index: 0,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BrTable {
    pub default: u32,
    pub targets: Vec<u32>,
}

impl BrTable {
    /// Returns the number of `br_table` entries, not including the default label.
    pub fn len(&self) -> u32 {
        self.targets.len() as u32
    }

    /// Returns whether `BrTable` doesn’t have any labels apart from the default one.
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }
}

impl TryFrom<wasmparser::BrTable<'_>> for BrTable {
    type Error = ModuleError;

    fn try_from(targets: wasmparser::BrTable) -> Result<Self> {
        Ok(Self {
            default: targets.default(),
            targets: targets
                .targets()
                .collect::<Result<Vec<_>, BinaryReaderError>>()?,
        })
    }
}

#[derive(Default, Clone, derive_more::DebugCustom)]
#[debug(fmt = "ConstExpr {{ .. }}")]
pub struct ConstExpr {
    pub instructions: Vec<Instruction>,
}

impl ConstExpr {
    fn new(expr: wasmparser::ConstExpr) -> Result<Self> {
        let mut instructions = Vec::new();
        let mut ops = expr.get_operators_reader();
        while !ops.is_end_then_eof() {
            instructions.push(Instruction::new(ops.read()?)?);
        }

        Ok(Self { instructions })
    }

    fn reencode(&self) -> Result<wasm_encoder::ConstExpr> {
        Ok(wasm_encoder::ConstExpr::extended(
            self.instructions
                .iter()
                .map(Instruction::reencode)
                .collect::<Result<Vec<_>>>()?,
        ))
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Import {
    pub module: Cow<'static, str>,
    pub name: Cow<'static, str>,
    pub ty: TypeRef,
}

impl Import {
    fn new(import: wasmparser::Import) -> Self {
        Self {
            module: import.module.to_string().into(),
            name: import.name.to_string().into(),
            ty: import.ty,
        }
    }

    pub fn reencode(&self, imports: &mut wasm_encoder::ImportSection) -> Result<()> {
        imports.import(
            &self.module,
            &self.name,
            RoundtripReencoder.entity_type(self.ty)?,
        );
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum TableInit {
    RefNull,
    Expr(ConstExpr),
}

#[derive(Clone, Debug)]
pub struct Table {
    pub ty: TableType,
    pub init: TableInit,
}

impl Table {
    fn new(table: wasmparser::Table) -> Result<Self> {
        Ok(Self {
            ty: table.ty,
            init: match table.init {
                wasmparser::TableInit::RefNull => TableInit::RefNull,
                wasmparser::TableInit::Expr(expr) => TableInit::Expr(ConstExpr::new(expr)?),
            },
        })
    }

    fn reencode(&self, tables: &mut wasm_encoder::TableSection) -> Result<()> {
        let ty = RoundtripReencoder.table_type(self.ty)?;
        match &self.init {
            TableInit::RefNull => {
                tables.table(ty);
            }
            TableInit::Expr(e) => {
                tables.table_with_init(ty, &e.reencode()?);
            }
        }
        Ok(())
    }
}

pub struct Global {
    pub ty: GlobalType,
    pub init_expr: ConstExpr,
}

impl Global {
    fn new(global: wasmparser::Global) -> Result<Self> {
        Ok(Self {
            ty: global.ty,
            init_expr: ConstExpr::new(global.init_expr)?,
        })
    }
}

pub struct Export {
    pub name: String,
    pub kind: ExternalKind,
    pub index: u32,
}

impl Export {
    fn new(export: wasmparser::Export) -> Self {
        Self {
            name: export.name.to_string(),
            kind: export.kind,
            index: export.index,
        }
    }
}

#[derive(Clone)]
pub enum ElementKind {
    Passive,
    Active {
        table_index: Option<u32>,
        offset_expr: ConstExpr,
    },
    Declared,
}

impl ElementKind {
    fn new(kind: wasmparser::ElementKind) -> Result<Self> {
        Ok(match kind {
            wasmparser::ElementKind::Passive => Self::Passive,
            wasmparser::ElementKind::Active {
                table_index,
                offset_expr,
            } => Self::Active {
                table_index,
                offset_expr: ConstExpr::new(offset_expr)?,
            },
            wasmparser::ElementKind::Declared => Self::Declared,
        })
    }
}

#[derive(Clone)]
pub enum ElementItems {
    Functions(Vec<u32>),
    Expressions(RefType, Vec<ConstExpr>),
}

impl ElementItems {
    fn new(elements: wasmparser::ElementItems) -> Result<Self> {
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
                Self::Expressions(ty, exprs)
            }
        })
    }
}

#[derive(Clone)]
pub struct Element {
    pub kind: ElementKind,
    pub items: ElementItems,
}

impl Element {
    fn new(element: wasmparser::Element) -> Result<Self> {
        Ok(Self {
            kind: ElementKind::new(element.kind)?,
            items: ElementItems::new(element.items)?,
        })
    }

    fn reencode(&self, encoder_section: &mut wasm_encoder::ElementSection) -> Result<()> {
        let items = match &self.items {
            ElementItems::Functions(funcs) => {
                wasm_encoder::Elements::Functions(funcs.clone().into())
            }
            ElementItems::Expressions(ty, exprs) => wasm_encoder::Elements::Expressions(
                RoundtripReencoder.ref_type(*ty)?,
                exprs
                    .iter()
                    .map(ConstExpr::reencode)
                    .collect::<Result<Vec<_>>>()?
                    .into(),
            ),
        };

        match &self.kind {
            ElementKind::Passive => {
                encoder_section.passive(items);
            }
            ElementKind::Active {
                table_index,
                offset_expr,
            } => {
                encoder_section.active(*table_index, &offset_expr.reencode()?, items);
            }
            ElementKind::Declared => {
                encoder_section.declared(items);
            }
        }

        Ok(())
    }
}

pub enum DataKind {
    Passive,
    Active {
        memory_index: u32,
        offset_expr: ConstExpr,
    },
}

pub struct Data {
    pub kind: DataKind,
    pub data: Cow<'static, [u8]>,
}

impl Data {
    fn new(data: wasmparser::Data) -> Result<Self> {
        Ok(Self {
            kind: match data.kind {
                wasmparser::DataKind::Passive => DataKind::Passive,
                wasmparser::DataKind::Active {
                    memory_index,
                    offset_expr,
                } => DataKind::Active {
                    memory_index,
                    offset_expr: ConstExpr::new(offset_expr)?,
                },
            },
            data: data.data.to_vec().into(),
        })
    }
}

#[derive(Debug, Default)]
pub struct Function {
    pub locals: Vec<(u32, ValType)>,
    pub instructions: Vec<Instruction>,
}

impl Function {
    fn from_entry(func: FunctionBody) -> Result<Self> {
        let mut locals = Vec::new();
        for pair in func.get_locals_reader()? {
            let (cnt, ty) = pair?;
            locals.push((cnt, ty));
        }

        let mut instructions = Vec::new();
        let mut reader = func.get_operators_reader()?;
        while !reader.eof() {
            instructions.push(Instruction::new(reader.read()?)?);
        }

        Ok(Self {
            locals,
            instructions,
        })
    }
}

#[derive(Debug, Default)]
pub struct ModuleBuilder {
    module: Module,
}

impl ModuleBuilder {
    pub fn from_module(module: Module) -> Self {
        Self { module }
    }

    pub fn rewrite_sections_after_insertion(
        mut self,
        inserted_index: u32,
        inserted_count: u32,
    ) -> Self {
        if inserted_count == 0 {
            panic!("inserted count is zero");
        }

        if let Some(section) = self.module.code_section_mut() {
            for func in section {
                for instruction in &mut func.instructions {
                    if let Instruction::Call { function_index } = instruction {
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
                                if let Instruction::Call { function_index } = instruction {
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

        // TODO: decode name section and rewrite

        self
    }

    pub fn build(self) -> Module {
        self.module
    }

    pub fn as_module(&self) -> &Module {
        &self.module
    }

    pub fn as_module_mut(&mut self) -> &mut Module {
        &mut self.module
    }

    fn type_section(&mut self) -> &mut TypeSection {
        self.module
            .type_section
            .get_or_insert_with(Default::default)
    }

    fn import_section(&mut self) -> &mut Vec<Import> {
        self.module.import_section.get_or_insert_with(Vec::new)
    }

    fn func_section(&mut self) -> &mut Vec<u32> {
        self.module.function_section.get_or_insert_with(Vec::new)
    }

    fn global_section(&mut self) -> &mut Vec<Global> {
        self.module.global_section.get_or_insert_with(Vec::new)
    }

    fn export_section(&mut self) -> &mut Vec<Export> {
        self.module.export_section.get_or_insert_with(Vec::new)
    }

    fn data_section(&mut self) -> &mut DataSection {
        self.module.data_section.get_or_insert_with(Vec::new)
    }

    fn code_section(&mut self) -> &mut CodeSection {
        self.module.code_section.get_or_insert_with(Vec::new)
    }

    /// Adds a new function to the module.
    ///
    /// Returns index from function section
    pub fn add_func(&mut self, ty: FuncType, function: Function) -> u32 {
        let type_idx = self.push_type(ty);
        self.func_section().push(type_idx);
        let func_idx = self.func_section().len() as u32 - 1;
        self.code_section().push(function);
        func_idx
    }

    pub fn push_type(&mut self, ty: FuncType) -> u32 {
        let idx = self.type_section().iter().position(|vec_ty| *vec_ty == ty);
        idx.map(|pos| pos as u32).unwrap_or_else(|| {
            self.type_section().push(ty);
            self.type_section().len() as u32 - 1
        })
    }

    pub fn push_import(&mut self, import: Import) {
        self.import_section().push(import);
    }

    pub fn push_global(&mut self, global: Global) -> u32 {
        self.global_section().push(global);
        self.global_section().len() as u32 - 1
    }

    pub fn push_export(&mut self, export: Export) {
        self.export_section().push(export);
    }

    pub fn push_data(&mut self, data: Data) {
        self.data_section().push(data);
    }
}

pub type TypeSection = Vec<FuncType>;
pub type FuncSection = Vec<u32>;
pub type DataSection = Vec<Data>;
pub type CodeSection = Vec<Function>;

#[derive(derive_more::DebugCustom, Default)]
#[debug(fmt = "Module {{ .. }}")]
pub struct Module {
    pub type_section: Option<TypeSection>,
    pub import_section: Option<Vec<Import>>,
    pub function_section: Option<FuncSection>,
    pub table_section: Option<Vec<Table>>,
    pub memory_section: Option<Vec<MemoryType>>,
    pub global_section: Option<Vec<Global>>,
    pub export_section: Option<Vec<Export>>,
    pub start_section: Option<u32>,
    pub element_section: Option<Vec<Element>>,
    pub data_section: Option<DataSection>,
    pub code_section: Option<CodeSection>,
}

impl Module {
    pub fn new(code: &[u8]) -> Result<Self> {
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
                    import_section = Some(
                        section
                            .into_iter()
                            .map(|import| import.map(Import::new))
                            .collect::<Result<_, _>>()?,
                    );
                }
                Payload::FunctionSection(section) => {
                    debug_assert!(function_section.is_none());
                    function_section = Some(section.into_iter().collect::<Result<_, _>>()?);
                }
                Payload::TableSection(section) => {
                    debug_assert!(table_section.is_none());
                    table_section = Some(
                        section
                            .into_iter()
                            .map(|table| table.map_err(Into::into).and_then(Table::new))
                            .collect::<Result<_, _>>()?,
                    );
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
                    export_section = Some(
                        section
                            .into_iter()
                            .map(|e| e.map(Export::new))
                            .collect::<Result<_, _>>()?,
                    );
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
                // note: the section is not present in WASM MVP
                Payload::DataCountSection { count, range: _ } => {
                    data_section = Some(Vec::with_capacity(count as usize));
                }
                Payload::DataSection(section) => {
                    let data_section = data_section.get_or_insert_with(Vec::new);
                    for data in section {
                        let data = data?;
                        data_section.push(Data::new(data)?);
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

    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut module = wasm_encoder::Module::new();

        if let Some(crate_section) = self.type_section() {
            let mut encoder_section = wasm_encoder::TypeSection::new();
            for func_type in crate_section.clone() {
                encoder_section
                    .ty()
                    .func_type(&RoundtripReencoder.func_type(func_type)?);
            }
            module.section(&encoder_section);
        }

        if let Some(crate_section) = self.import_section() {
            let mut encoder_section = wasm_encoder::ImportSection::new();
            for import in crate_section.clone() {
                import.reencode(&mut encoder_section)?;
            }
            module.section(&encoder_section);
        }

        if let Some(crate_section) = self.function_section() {
            let mut encoder_section = wasm_encoder::FunctionSection::new();
            for &function in crate_section {
                encoder_section.function(function);
            }
            module.section(&encoder_section);
        }

        if let Some(crate_section) = self.table_section() {
            let mut encoder_section = wasm_encoder::TableSection::new();
            for table in crate_section.clone() {
                table.reencode(&mut encoder_section)?;
            }
            module.section(&encoder_section);
        }

        if let Some(crate_section) = self.memory_section() {
            let mut encoder_section = wasm_encoder::MemorySection::new();
            for &memory_type in crate_section {
                encoder_section.memory(RoundtripReencoder.memory_type(memory_type));
            }
            module.section(&encoder_section);
        }

        if let Some(crate_section) = self.global_section() {
            let mut encoder_section = wasm_encoder::GlobalSection::new();
            for global in crate_section {
                encoder_section.global(
                    RoundtripReencoder.global_type(global.ty)?,
                    &global.init_expr.reencode()?,
                );
            }
            module.section(&encoder_section);
        }

        if let Some(crate_section) = self.export_section() {
            let mut encoder_section = wasm_encoder::ExportSection::new();
            for export in crate_section {
                encoder_section.export(
                    &export.name,
                    RoundtripReencoder.export_kind(export.kind),
                    export.index,
                );
            }
            module.section(&encoder_section);
        }

        if let Some(function_index) = self.start_section {
            module.section(&wasm_encoder::StartSection { function_index });
        }

        if let Some(crate_section) = self.element_section() {
            let mut encoder_section = wasm_encoder::ElementSection::new();
            for element in crate_section {
                element.reencode(&mut encoder_section)?;
            }
            module.section(&encoder_section);
        }

        if let Some(crate_section) = self.data_section() {
            let mut encoder_section = wasm_encoder::DataSection::new();
            for data in crate_section {
                match &data.kind {
                    DataKind::Passive => {
                        encoder_section.passive(data.data.iter().copied());
                    }
                    DataKind::Active {
                        memory_index,
                        offset_expr,
                    } => {
                        encoder_section.active(
                            *memory_index,
                            &offset_expr.reencode()?,
                            data.data.iter().copied(),
                        );
                    }
                }
            }
            module.section(&encoder_section);
        }

        if let Some(crate_section) = self.code_section() {
            let mut encoder_section = wasm_encoder::CodeSection::new();
            for function in crate_section {
                let mut encoder_func = wasm_encoder::Function::new(
                    function
                        .locals
                        .iter()
                        .map(|&(cnt, ty)| Ok((cnt, RoundtripReencoder.val_type(ty)?)))
                        .collect::<Result<Vec<_>, reencode::Error>>()?,
                );
                for op in function.instructions.clone() {
                    encoder_func.instruction(&op.reencode()?);
                }

                encoder_section.function(&encoder_func);
            }
            module.section(&encoder_section);
        }

        Ok(module.finish())
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

    pub fn import_section(&self) -> Option<&Vec<Import>> {
        self.import_section.as_ref()
    }

    pub fn import_section_mut(&mut self) -> Option<&mut Vec<Import>> {
        self.import_section.as_mut()
    }

    pub fn function_section(&self) -> Option<&FuncSection> {
        self.function_section.as_ref()
    }

    pub fn function_section_mut(&mut self) -> Option<&mut FuncSection> {
        self.function_section.as_mut()
    }

    pub fn table_section(&self) -> Option<&Vec<Table>> {
        self.table_section.as_ref()
    }

    pub fn table_section_mut(&mut self) -> Option<&mut Vec<Table>> {
        self.table_section.as_mut()
    }

    pub fn memory_section(&self) -> Option<&Vec<MemoryType>> {
        self.memory_section.as_ref()
    }

    pub fn memory_section_mut(&mut self) -> Option<&mut Vec<MemoryType>> {
        self.memory_section.as_mut()
    }

    pub fn global_section(&self) -> Option<&Vec<Global>> {
        self.global_section.as_ref()
    }

    pub fn global_section_mut(&mut self) -> Option<&mut Vec<Global>> {
        self.global_section.as_mut()
    }

    pub fn export_section(&self) -> Option<&Vec<Export>> {
        self.export_section.as_ref()
    }

    pub fn export_section_mut(&mut self) -> Option<&mut Vec<Export>> {
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

    pub fn element_section_mut(&mut self) -> Option<&mut Vec<Element>> {
        self.element_section.as_mut()
    }

    pub fn data_section(&self) -> Option<&DataSection> {
        self.data_section.as_ref()
    }

    pub fn data_section_mut(&mut self) -> Option<&mut DataSection> {
        self.data_section.as_mut()
    }

    pub fn code_section(&self) -> Option<&Vec<Function>> {
        self.code_section.as_ref()
    }

    pub fn code_section_mut(&mut self) -> Option<&mut CodeSection> {
        self.code_section.as_mut()
    }
}
