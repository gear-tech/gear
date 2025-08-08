// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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
    vec,
    vec::Vec,
};
use core::convert::Infallible;
use wasm_encoder::{
    reencode,
    reencode::{Reencode, RoundtripReencoder},
};
use wasmparser::{
    BinaryReaderError, Encoding, ExternalKind, FuncType, FunctionBody, GlobalType, KnownCustom,
    MemoryType, Payload, RefType, TableType, TypeRef, ValType, WasmFeatures,
};

pub const GEAR_SUPPORTED_FEATURES: WasmFeatures = WasmFeatures::WASM1
    .union(WasmFeatures::SIGN_EXTENSION)
    .difference(WasmFeatures::FLOATS);

// based on `wasmparser::_for_each_operator_group` and
// it's recommended to read its documentation to understand the logic
//
// float instructions are removed
macro_rules! for_each_instruction_group {
    ($mac:ident) => {
        $mac! {
            @mvp {
                Unreachable
                Nop
                Block { blockty: $crate::BlockType }
                Loop { blockty: $crate::BlockType }
                If { blockty: $crate::BlockType }
                Else
                End
                Br { relative_depth: u32 }
                BrIf { relative_depth: u32 }
                BrTable { targets: $crate::BrTable }
                Return
                Call { function_index: u32 }
                CallIndirect { type_index: u32, table_index: u32 }
                Drop
                Select
                LocalGet { local_index: u32 }
                LocalSet { local_index: u32 }
                LocalTee { local_index: u32 }
                GlobalGet { global_index: u32 }
                GlobalSet { global_index: u32 }
                I32Load { memarg: $crate::MemArg }
                I64Load { memarg: $crate::MemArg }
                I32Load8S { memarg: $crate::MemArg }
                I32Load8U { memarg: $crate::MemArg }
                I32Load16S { memarg: $crate::MemArg }
                I32Load16U { memarg: $crate::MemArg }
                I64Load8S { memarg: $crate::MemArg }
                I64Load8U { memarg: $crate::MemArg }
                I64Load16S { memarg: $crate::MemArg }
                I64Load16U { memarg: $crate::MemArg }
                I64Load32S { memarg: $crate::MemArg }
                I64Load32U { memarg: $crate::MemArg }
                I32Store { memarg: $crate::MemArg }
                I64Store { memarg: $crate::MemArg }
                I32Store8 { memarg: $crate::MemArg }
                I32Store16 { memarg: $crate::MemArg }
                I64Store8 { memarg: $crate::MemArg }
                I64Store16 { memarg: $crate::MemArg }
                I64Store32 { memarg: $crate::MemArg }
                MemorySize { mem: u32 }
                MemoryGrow { mem: u32 }
                I32Const { value: i32 }
                I64Const { value: i64 }
                I32Eqz
                I32Eq
                I32Ne
                I32LtS
                I32LtU
                I32GtS
                I32GtU
                I32LeS
                I32LeU
                I32GeS
                I32GeU
                I64Eqz
                I64Eq
                I64Ne
                I64LtS
                I64LtU
                I64GtS
                I64GtU
                I64LeS
                I64LeU
                I64GeS
                I64GeU
                I32Clz
                I32Ctz
                I32Popcnt
                I32Add
                I32Sub
                I32Mul
                I32DivS
                I32DivU
                I32RemS
                I32RemU
                I32And
                I32Or
                I32Xor
                I32Shl
                I32ShrS
                I32ShrU
                I32Rotl
                I32Rotr
                I64Clz
                I64Ctz
                I64Popcnt
                I64Add
                I64Sub
                I64Mul
                I64DivS
                I64DivU
                I64RemS
                I64RemU
                I64And
                I64Or
                I64Xor
                I64Shl
                I64ShrS
                I64ShrU
                I64Rotl
                I64Rotr
                I32WrapI64
                I64ExtendI32S
                I64ExtendI32U
            }

            @sign_extension {
                I32Extend8S
                I32Extend16S
                I64Extend8S
                I64Extend16S
                I64Extend32S
            }
        }
    };
}

// exactly the same as `for_each_instruction_group` but without proposals info
macro_rules! define_for_each_instruction {
    ($(
        @$proposal:ident {
            $($op:ident $( { $( $arg:ident: $argty:ty ),+ } )?)+
        }
    )+) => {
        macro_rules! for_each_instruction {
            ($mac:ident) => {
                $mac! {
                    $(
                        $(
                            $op $( { $( $arg: $argty ),+ } )?
                        )+
                    )+
                }
            };
        }
    };
}

// `for_each_instruction` is now defined
for_each_instruction_group!(define_for_each_instruction);

macro_rules! define_instruction {
    ($( $op:ident $( { $( $arg:ident: $argty:ty ),+ } )? )+) => {
        define_instruction!(@convert $( $op $( { $( $arg: $argty ),+ } )? )+ @accum);
    };
    // omit `table_index` field of `call_indirect` instruction because it's always zero
    // but we still save original fields to use them for `wasmparser` and `wasm-encoder` types
    // during parsing and reencoding
    (
        @convert
        CallIndirect { $type_index_arg:ident: $type_index_argty:ty, $table_index_arg:ident: $table_index_argty:ty }
        $( $ops:ident $( { $($args:ident: $argtys:ty),+ } )? )*
        @accum
        $( $accum_op:ident $( { $($original_arg:ident: $original_argty:ty),+ } => { $($accum_arg:ident: $accum_argty:ty),+ } )? )*
    ) => {
        define_instruction!(
            @convert
            $( $ops $( { $($args: $argtys),+ } )? )*
            @accum
            CallIndirect { $type_index_arg: $type_index_argty, $table_index_arg: $table_index_argty } => { $type_index_arg: $type_index_argty }
            $( $accum_op $( { $($original_arg: $original_argty),+ } => { $($accum_arg: $accum_argty),+ } )? )*
        );
    };
    // do nothing to the rest instructions and collect them
    (
        @convert
        $op:ident $( { $($arg:ident: $argty:ty),+ } )?
        $( $ops:ident $( { $($args:ident: $argtys:ty),+ } )? )*
        @accum
        $( $accum_op:ident $( { $($original_arg:ident: $original_argty:ty),+ } => { $($accum_arg:ident: $accum_argty:ty),+ } )? )*
    ) => {
        define_instruction!(
            @convert
            $( $ops $( { $($args: $argtys),+ } )? )*
            @accum
            $op $( { $($arg: $argty),+ } => { $($arg: $argty),+ } )?
            $( $accum_op $( { $($original_arg: $original_argty),+ } => { $($accum_arg: $accum_argty),+ } )? )*
        );
    };
    // collection is done so we define `Instruction` itself now
    (
        @convert
        @accum
        $( $op:ident $( { $( $original_arg:ident: $original_argty:ty ),+ } => { $( $arg:ident: $argty:ty ),+ } )? )+
    ) => {
        #[derive(Debug, Clone, Eq, PartialEq)]
        pub enum Instruction {
            $(
                $op $(( $( $argty ),+ ))?,
            )+
        }

        impl Instruction {
            fn parse(op: wasmparser::Operator) -> Result<Self> {
                match op {
                    $(
                        wasmparser::Operator::$op $({ $($original_arg),* })? => {
                            define_instruction!(@parse $op $(( $($original_arg $original_arg),* ))?)
                        }
                    )*
                    op => Err(ModuleError::UnsupportedInstruction(format!("{op:?}"))),
                }
            }

            fn reencode(&self) -> Result<wasm_encoder::Instruction<'_>> {
                Ok(match self {
                    $(
                        Self::$op $(( $($arg),+ ))? => {
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

    // further macro branches are based on `wasm_encoder::reencode` module

    (@parse CallIndirect($type_index:ident type_index, $table_index:ident table_index)) => {{
        // already verified by wasmparser
        debug_assert_eq!($table_index, 0);

        Ok(Self::CallIndirect(<_>::try_from($type_index)?))
    }};
    (@parse $op:ident $(( $($arg:ident $_arg:ident),* ))?) => {
        Ok(Self::$op $(( $(<_>::try_from($arg)?),* ))?)
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
    (@build CallIndirect $arg:ident) => (wasm_encoder::Instruction::CallIndirect { type_index: $arg, table_index: 0 });
    (@build $op:ident $arg:ident) => (wasm_encoder::Instruction::$op($arg));
    (@build $op:ident $($arg:ident)*) => (wasm_encoder::Instruction::$op { $($arg),* });
}

for_each_instruction!(define_instruction);

impl Instruction {
    /// Returns `true` if instruction is forbidden to be used by user
    /// but allowed to be used by instrumentation stage.
    pub fn is_user_forbidden(&self) -> bool {
        matches!(self, Self::MemoryGrow { .. })
    }
}

pub type Result<T, E = ModuleError> = core::result::Result<T, E>;

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum ModuleError {
    #[display("Binary reader error: {_0}")]
    BinaryReader(BinaryReaderError),
    #[display("Reencode error: {_0}")]
    Reencode(reencode::Error),
    #[display("Int conversion error: {_0}")]
    TryFromInt(core::num::TryFromIntError),
    #[display("Unsupported instruction: {_0}")]
    UnsupportedInstruction(String),
    #[display("Multiple tables")]
    MultipleTables,
    #[display("Multiple memories")]
    MultipleMemories,
    #[from(skip)]
    #[display("Memory index must be zero (actual: {_0})")]
    NonZeroMemoryIdx(u32),
    #[from(skip)]
    #[display("Optional table index of element segment is not supported (index: {_0})")]
    ElementTableIdx(u32),
    #[display("Passive data is not supported")]
    PassiveDataKind,
    #[display("Element expressions are not supported")]
    ElementExpressions,
    #[display("Only active element is supported")]
    NonActiveElementKind,
    #[display("Table init expression is not supported")]
    TableInitExpr,
}

impl core::error::Error for ModuleError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            ModuleError::BinaryReader(e) => Some(e),
            ModuleError::Reencode(e) => Some(e),
            ModuleError::TryFromInt(e) => Some(e),
            ModuleError::UnsupportedInstruction(_) => None,
            ModuleError::MultipleTables => None,
            ModuleError::MultipleMemories => None,
            ModuleError::NonZeroMemoryIdx(_) => None,
            ModuleError::ElementTableIdx(_) => None,
            ModuleError::PassiveDataKind => None,
            ModuleError::ElementExpressions => None,
            ModuleError::NonActiveElementKind => None,
            ModuleError::TableInitExpr => None,
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
        // always zero if multi-memory is not enabled
        debug_assert_eq!(memory, 0);
        Ok(Self {
            align,
            offset: offset.try_into()?,
        })
    }
}

impl MemArg {
    pub fn zero() -> Self {
        Self {
            align: 0,
            offset: 0,
        }
    }

    pub fn i32() -> Self {
        Self::i32_offset(0)
    }

    pub fn i64() -> Self {
        Self::i64_offset(0)
    }

    pub fn i32_offset(offset: u32) -> Self {
        Self { align: 2, offset }
    }

    pub fn i64_offset(offset: u32) -> Self {
        Self { align: 3, offset }
    }

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

#[derive(Default, Clone, derive_more::Debug, Eq, PartialEq)]
#[debug("ConstExpr {{ .. }}")]
pub struct ConstExpr {
    pub instructions: Vec<Instruction>,
}

impl ConstExpr {
    pub fn empty() -> Self {
        Self {
            instructions: Vec::new(),
        }
    }

    pub fn i32_value(value: i32) -> Self {
        Self {
            instructions: vec![Instruction::I32Const(value)],
        }
    }

    pub fn i64_value(value: i64) -> Self {
        Self {
            instructions: vec![Instruction::I64Const(value)],
        }
    }

    fn parse(expr: wasmparser::ConstExpr) -> Result<Self> {
        let mut instructions = Vec::new();
        let mut ops = expr.get_operators_reader();
        while !ops.is_end_then_eof() {
            instructions.push(Instruction::parse(ops.read()?)?);
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
    pub fn func(
        module: impl Into<Cow<'static, str>>,
        name: impl Into<Cow<'static, str>>,
        index: u32,
    ) -> Self {
        Self {
            module: module.into(),
            name: name.into(),
            ty: TypeRef::Func(index),
        }
    }

    pub fn memory(initial: u32, maximum: Option<u32>) -> Self {
        Self {
            module: "env".into(),
            name: "memory".into(),
            ty: TypeRef::Memory(MemoryType {
                memory64: false,
                shared: false,
                initial: initial as u64,
                maximum: maximum.map(|v| v as u64),
                page_size_log2: None,
            }),
        }
    }

    fn parse(import: wasmparser::Import) -> Self {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableInit {
    RefNull,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Table {
    pub ty: TableType,
    pub init: TableInit,
}

impl Table {
    pub fn funcref(initial: u32, maximum: Option<u32>) -> Self {
        Table {
            ty: TableType {
                element_type: RefType::FUNCREF,
                table64: false,
                initial: initial as u64,
                maximum: maximum.map(|v| v as u64),
                shared: false,
            },
            init: TableInit::RefNull,
        }
    }

    fn parse(table: wasmparser::Table) -> Result<Self> {
        Ok(Self {
            ty: table.ty,
            init: match table.init {
                wasmparser::TableInit::RefNull => TableInit::RefNull,
                wasmparser::TableInit::Expr(_expr) => return Err(ModuleError::TableInitExpr),
            },
        })
    }

    fn reencode(&self, tables: &mut wasm_encoder::TableSection) -> Result<()> {
        let ty = RoundtripReencoder.table_type(self.ty)?;
        match &self.init {
            TableInit::RefNull => {
                tables.table(ty);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Global {
    pub ty: GlobalType,
    pub init_expr: ConstExpr,
}

impl Global {
    pub fn i32_value(value: i32) -> Self {
        Self {
            ty: GlobalType {
                content_type: ValType::I32,
                mutable: false,
                shared: false,
            },
            init_expr: ConstExpr::i32_value(value),
        }
    }

    pub fn i64_value(value: i64) -> Self {
        Self {
            ty: GlobalType {
                content_type: ValType::I64,
                mutable: false,
                shared: false,
            },
            init_expr: ConstExpr::i64_value(value),
        }
    }

    pub fn i64_value_mut(value: i64) -> Self {
        Self {
            ty: GlobalType {
                content_type: ValType::I64,
                mutable: true,
                shared: false,
            },
            init_expr: ConstExpr::i64_value(value),
        }
    }

    fn parse(global: wasmparser::Global) -> Result<Self> {
        Ok(Self {
            ty: global.ty,
            init_expr: ConstExpr::parse(global.init_expr)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Export {
    pub name: Cow<'static, str>,
    pub kind: ExternalKind,
    pub index: u32,
}

impl Export {
    pub fn func(name: impl Into<Cow<'static, str>>, index: u32) -> Self {
        Self {
            name: name.into(),
            kind: ExternalKind::Func,
            index,
        }
    }

    pub fn global(name: impl Into<Cow<'static, str>>, index: u32) -> Self {
        Self {
            name: name.into(),
            kind: ExternalKind::Global,
            index,
        }
    }

    fn parse(export: wasmparser::Export) -> Self {
        Self {
            name: export.name.to_string().into(),
            kind: export.kind,
            index: export.index,
        }
    }
}

#[derive(Clone)]
pub enum ElementKind {
    Active { offset_expr: ConstExpr },
}

impl ElementKind {
    fn parse(kind: wasmparser::ElementKind) -> Result<Self> {
        match kind {
            wasmparser::ElementKind::Passive => Err(ModuleError::NonActiveElementKind),
            wasmparser::ElementKind::Active {
                table_index,
                offset_expr,
            } => {
                if let Some(table_index) = table_index {
                    return Err(ModuleError::ElementTableIdx(table_index));
                }

                Ok(Self::Active {
                    offset_expr: ConstExpr::parse(offset_expr)?,
                })
            }
            wasmparser::ElementKind::Declared => Err(ModuleError::NonActiveElementKind),
        }
    }
}

#[derive(Clone)]
pub enum ElementItems {
    Functions(Vec<u32>),
}

impl ElementItems {
    fn parse(elements: wasmparser::ElementItems) -> Result<Self> {
        match elements {
            wasmparser::ElementItems::Functions(f) => {
                let mut funcs = Vec::new();
                for func in f {
                    funcs.push(func?);
                }
                Ok(Self::Functions(funcs))
            }
            wasmparser::ElementItems::Expressions(_ty, _e) => Err(ModuleError::ElementExpressions),
        }
    }
}

#[derive(Clone)]
pub struct Element {
    pub kind: ElementKind,
    pub items: ElementItems,
}

impl Element {
    pub fn functions(funcs: Vec<u32>) -> Self {
        Self {
            kind: ElementKind::Active {
                offset_expr: ConstExpr::i32_value(0),
            },
            items: ElementItems::Functions(funcs),
        }
    }

    fn parse(element: wasmparser::Element) -> Result<Self> {
        Ok(Self {
            kind: ElementKind::parse(element.kind)?,
            items: ElementItems::parse(element.items)?,
        })
    }

    fn reencode(&self, encoder_section: &mut wasm_encoder::ElementSection) -> Result<()> {
        let items = match &self.items {
            ElementItems::Functions(funcs) => {
                wasm_encoder::Elements::Functions(funcs.clone().into())
            }
        };

        match &self.kind {
            ElementKind::Active { offset_expr } => {
                encoder_section.active(None, &offset_expr.reencode()?, items);
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Data {
    pub offset_expr: ConstExpr,
    pub data: Cow<'static, [u8]>,
}

impl Data {
    pub fn with_offset(data: impl Into<Cow<'static, [u8]>>, offset: u32) -> Self {
        Self {
            offset_expr: ConstExpr::i32_value(offset as i32),
            data: data.into(),
        }
    }

    fn parse(data: wasmparser::Data) -> Result<Self> {
        Ok(Self {
            offset_expr: match data.kind {
                wasmparser::DataKind::Passive => return Err(ModuleError::PassiveDataKind),
                wasmparser::DataKind::Active {
                    memory_index,
                    offset_expr,
                } => {
                    if memory_index != 0 {
                        return Err(ModuleError::NonZeroMemoryIdx(memory_index));
                    }

                    ConstExpr::parse(offset_expr)?
                }
            },
            data: data.data.to_vec().into(),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Function {
    pub locals: Vec<(u32, ValType)>,
    pub instructions: Vec<Instruction>,
}

impl Function {
    pub fn from_instructions(instructions: impl Into<Vec<Instruction>>) -> Self {
        Self {
            locals: Vec::new(),
            instructions: instructions.into(),
        }
    }

    fn from_entry(func: FunctionBody) -> Result<Self> {
        let mut locals = Vec::new();
        for pair in func.get_locals_reader()? {
            let (cnt, ty) = pair?;
            locals.push((cnt, ty));
        }

        let mut instructions = Vec::new();
        let mut reader = func.get_operators_reader()?;
        while !reader.eof() {
            instructions.push(Instruction::parse(reader.read()?)?);
        }

        Ok(Self {
            locals,
            instructions,
        })
    }

    fn reencode(&self) -> Result<wasm_encoder::Function> {
        let mut encoder_func = wasm_encoder::Function::new(
            self.locals
                .iter()
                .map(|&(cnt, ty)| Ok((cnt, RoundtripReencoder.val_type(ty)?)))
                .collect::<Result<Vec<_>, reencode::Error>>()?,
        );

        for op in &self.instructions {
            encoder_func.instruction(&op.reencode()?);
        }

        if self.instructions.is_empty() {
            encoder_func.instruction(&wasm_encoder::Instruction::End);
        }

        Ok(encoder_func)
    }
}

pub type NameMap = Vec<Naming>;

/// Represents a name for an index from the names section.
#[derive(Debug, Clone)]
pub struct Naming {
    /// The index being named.
    pub index: u32,
    /// The name for the index.
    pub name: Cow<'static, str>,
}

pub type IndirectNameMap = Vec<IndirectNaming>;

/// Represents an indirect name in the names custom section.
#[derive(Debug, Clone)]
pub struct IndirectNaming {
    /// The indirect index of the name.
    pub index: u32,
    /// The map of names within the `index` prior.
    pub names: NameMap,
}

#[derive(Debug, Clone)]
pub enum Name {
    /// The name is for the module.
    Module(Cow<'static, str>),
    /// The name is for the functions.
    Function(NameMap),
    /// The name is for the function locals.
    Local(IndirectNameMap),
    /// The name is for the function labels.
    Label(IndirectNameMap),
    /// The name is for the types.
    Type(NameMap),
    /// The name is for the tables.
    Table(NameMap),
    /// The name is for the memories.
    Memory(NameMap),
    /// The name is for the globals.
    Global(NameMap),
    /// The name is for the element segments.
    Element(NameMap),
    /// The name is for the data segments.
    Data(NameMap),
    /// The name is for fields.
    Field(IndirectNameMap),
    /// The name is for tags.
    Tag(NameMap),
    /// An unknown [name subsection](https://webassembly.github.io/spec/core/appendix/custom.html#subsections).
    Unknown {
        /// The identifier for this subsection.
        ty: u8,
        /// The contents of this subsection.
        data: Cow<'static, [u8]>,
    },
}

impl Name {
    fn parse(name: wasmparser::Name) -> Result<Self> {
        let name_map = |map: wasmparser::NameMap| {
            map.into_iter()
                .map(|n| {
                    n.map(|n| Naming {
                        index: n.index,
                        name: n.name.to_string().into(),
                    })
                })
                .collect::<Result<Vec<_>, BinaryReaderError>>()
        };

        let indirect_name_map = |map: wasmparser::IndirectNameMap| {
            map.into_iter()
                .map(|n| {
                    n.and_then(|n| {
                        Ok(IndirectNaming {
                            index: n.index,
                            names: name_map(n.names)?,
                        })
                    })
                })
                .collect::<Result<Vec<_>, BinaryReaderError>>()
        };

        Ok(match name {
            wasmparser::Name::Module {
                name,
                name_range: _,
            } => Self::Module(name.to_string().into()),
            wasmparser::Name::Function(map) => Self::Function(name_map(map)?),
            wasmparser::Name::Local(map) => Self::Local(indirect_name_map(map)?),
            wasmparser::Name::Label(map) => Self::Label(indirect_name_map(map)?),
            wasmparser::Name::Type(map) => Self::Type(name_map(map)?),
            wasmparser::Name::Table(map) => Self::Table(name_map(map)?),
            wasmparser::Name::Memory(map) => Self::Memory(name_map(map)?),
            wasmparser::Name::Global(map) => Self::Global(name_map(map)?),
            wasmparser::Name::Element(map) => Self::Element(name_map(map)?),
            wasmparser::Name::Data(map) => Self::Data(name_map(map)?),
            wasmparser::Name::Field(map) => Self::Field(indirect_name_map(map)?),
            wasmparser::Name::Tag(map) => Self::Tag(name_map(map)?),
            wasmparser::Name::Unknown { ty, data, range: _ } => Self::Unknown {
                ty,
                data: data.to_vec().into(),
            },
        })
    }

    fn reencode(&self, section: &mut wasm_encoder::NameSection) {
        let name_map = |map: &NameMap| {
            map.iter()
                .fold(wasm_encoder::NameMap::new(), |mut map, naming| {
                    map.append(naming.index, &naming.name);
                    map
                })
        };

        let indirect_name_map = |map: &IndirectNameMap| {
            map.iter()
                .fold(wasm_encoder::IndirectNameMap::new(), |mut map, naming| {
                    map.append(naming.index, &name_map(&naming.names));
                    map
                })
        };

        match self {
            Name::Module(name) => {
                section.module(name);
            }
            Name::Function(map) => section.functions(&name_map(map)),
            Name::Local(map) => section.locals(&indirect_name_map(map)),
            Name::Label(map) => section.labels(&indirect_name_map(map)),
            Name::Type(map) => section.types(&name_map(map)),
            Name::Table(map) => section.tables(&name_map(map)),
            Name::Memory(map) => section.memories(&name_map(map)),
            Name::Global(map) => section.globals(&name_map(map)),
            Name::Element(map) => section.elements(&name_map(map)),
            Name::Data(map) => section.data(&name_map(map)),
            Name::Field(map) => section.fields(&indirect_name_map(map)),
            Name::Tag(map) => section.tags(&name_map(map)),
            Name::Unknown { ty, data } => section.raw(*ty, data),
        }
    }
}

pub struct ModuleFuncIndexShifter {
    builder: ModuleBuilder,
    inserted_at: u32,
    code_section: bool,
    export_section: bool,
    element_section: bool,
    start_section: bool,
    name_section: bool,
}

impl ModuleFuncIndexShifter {
    pub fn with_code_section(mut self) -> Self {
        self.code_section = true;
        self
    }

    pub fn with_export_section(mut self) -> Self {
        self.export_section = true;
        self
    }

    pub fn with_element_section(mut self) -> Self {
        self.element_section = true;
        self
    }

    pub fn with_start_section(mut self) -> Self {
        self.start_section = true;
        self
    }

    pub fn with_name_section(mut self) -> Self {
        self.name_section = true;
        self
    }

    /// Shift function indices in every section
    pub fn with_all(self) -> Self {
        self.with_code_section()
            .with_export_section()
            .with_element_section()
            .with_start_section()
            .with_name_section()
    }

    pub fn shift_all(self) -> ModuleBuilder {
        self.with_all().shift()
    }

    /// Do actual shifting
    pub fn shift(mut self) -> ModuleBuilder {
        if let Some(section) = self
            .builder
            .module
            .code_section
            .as_mut()
            .filter(|_| self.code_section)
        {
            for func in section {
                for instruction in &mut func.instructions {
                    if let Instruction::Call(function_index) = instruction
                        && *function_index >= self.inserted_at
                    {
                        *function_index += 1
                    }
                }
            }
        }

        if let Some(section) = self
            .builder
            .module
            .export_section
            .as_mut()
            .filter(|_| self.export_section)
        {
            for export in section {
                if let ExternalKind::Func = export.kind
                    && export.index >= self.inserted_at
                {
                    export.index += 1
                }
            }
        }

        if let Some(section) = self
            .builder
            .module
            .element_section
            .as_mut()
            .filter(|_| self.element_section)
        {
            for segment in section {
                // update all indirect call addresses initial values
                match &mut segment.items {
                    ElementItems::Functions(funcs) => {
                        for func_index in funcs.iter_mut() {
                            if *func_index >= self.inserted_at {
                                *func_index += 1
                            }
                        }
                    }
                }
            }
        }

        if let Some(start_idx) = self
            .builder
            .module
            .start_section
            .as_mut()
            .filter(|_| self.start_section)
            && *start_idx >= self.inserted_at
        {
            *start_idx += 1
        }

        if let Some(section) = self
            .builder
            .module
            .name_section
            .as_mut()
            .filter(|_| self.name_section)
        {
            for name in section {
                if let Name::Function(map) = name {
                    for naming in map {
                        if naming.index >= self.inserted_at {
                            naming.index += 1;
                        }
                    }
                }
            }
        }

        self.builder
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

    pub fn shift_func_index(self, inserted_at: u32) -> ModuleFuncIndexShifter {
        ModuleFuncIndexShifter {
            builder: self,
            inserted_at,
            code_section: false,
            export_section: false,
            element_section: false,
            start_section: false,
            name_section: false,
        }
    }

    pub fn build(self) -> Module {
        self.module
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

    fn element_section(&mut self) -> &mut Vec<Element> {
        self.module.element_section.get_or_insert_with(Vec::new)
    }

    fn code_section(&mut self) -> &mut CodeSection {
        self.module.code_section.get_or_insert_with(Vec::new)
    }

    fn data_section(&mut self) -> &mut DataSection {
        self.module.data_section.get_or_insert_with(Vec::new)
    }

    fn custom_sections(&mut self) -> &mut Vec<CustomSection> {
        self.module.custom_sections.get_or_insert_with(Vec::new)
    }

    pub fn push_custom_section(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        data: impl Into<Vec<u8>>,
    ) {
        self.custom_sections().push((name.into(), data.into()));
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

    pub fn push_import(&mut self, import: Import) -> u32 {
        self.import_section().push(import);
        self.import_section().len() as u32 - 1
    }

    pub fn set_table(&mut self, table: Table) {
        debug_assert_eq!(self.module.table_section, None);
        self.module.table_section = Some(table);
    }

    pub fn push_global(&mut self, global: Global) -> u32 {
        self.global_section().push(global);
        self.global_section().len() as u32 - 1
    }

    pub fn push_export(&mut self, export: Export) {
        self.export_section().push(export);
    }

    pub fn push_element(&mut self, element: Element) {
        self.element_section().push(element);
    }

    pub fn push_data(&mut self, data: Data) {
        self.data_section().push(data);
    }
}

pub type TypeSection = Vec<FuncType>;
pub type FuncSection = Vec<u32>;
pub type CodeSection = Vec<Function>;
pub type DataSection = Vec<Data>;
pub type CustomSection = (Cow<'static, str>, Vec<u8>);

#[derive(derive_more::Debug, Clone, Default)]
#[debug("Module {{ .. }}")]
pub struct Module {
    pub type_section: Option<TypeSection>,
    pub import_section: Option<Vec<Import>>,
    pub function_section: Option<FuncSection>,
    pub table_section: Option<Table>,
    pub memory_section: Option<MemoryType>,
    pub global_section: Option<Vec<Global>>,
    pub export_section: Option<Vec<Export>>,
    pub start_section: Option<u32>,
    pub element_section: Option<Vec<Element>>,
    pub code_section: Option<CodeSection>,
    pub data_section: Option<DataSection>,
    pub name_section: Option<Vec<Name>>,
    pub custom_sections: Option<Vec<CustomSection>>,
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
        let mut code_section = None;
        let mut data_section = None;
        let mut name_section = None;
        let mut custom_sections = None;

        let mut parser = wasmparser::Parser::new(0);
        parser.set_features(GEAR_SUPPORTED_FEATURES);
        for payload in parser.parse_all(code) {
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
                            .map(|import| import.map(Import::parse))
                            .collect::<Result<_, _>>()?,
                    );
                }
                Payload::FunctionSection(section) => {
                    debug_assert!(function_section.is_none());
                    function_section = Some(section.into_iter().collect::<Result<_, _>>()?);
                }
                Payload::TableSection(section) => {
                    debug_assert!(table_section.is_none());
                    let mut section = section.into_iter();

                    table_section = section
                        .next()
                        .map(|table| table.map_err(Into::into).and_then(Table::parse))
                        .transpose()?;

                    if section.next().is_some() {
                        return Err(ModuleError::MultipleTables);
                    }
                }
                Payload::MemorySection(section) => {
                    debug_assert!(memory_section.is_none());
                    let mut section = section.into_iter();

                    memory_section = section.next().transpose()?;

                    if section.next().is_some() {
                        return Err(ModuleError::MultipleMemories);
                    }
                }
                Payload::TagSection(_) => {}
                Payload::GlobalSection(section) => {
                    debug_assert!(global_section.is_none());
                    global_section = Some(
                        section
                            .into_iter()
                            .map(|element| element.map_err(Into::into).and_then(Global::parse))
                            .collect::<Result<_, _>>()?,
                    );
                }
                Payload::ExportSection(section) => {
                    debug_assert!(export_section.is_none());
                    export_section = Some(
                        section
                            .into_iter()
                            .map(|e| e.map(Export::parse))
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
                            .map(|element| element.map_err(Into::into).and_then(Element::parse))
                            .collect::<Result<Vec<_>>>()?,
                    );
                }
                // note: the section is not present in WASM 1.0
                Payload::DataCountSection { count, range: _ } => {
                    data_section = Some(Vec::with_capacity(count as usize));
                }
                Payload::DataSection(section) => {
                    let data_section = data_section.get_or_insert_with(Vec::new);
                    for data in section {
                        let data = data?;
                        data_section.push(Data::parse(data)?);
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
                Payload::CustomSection(section) => match section.as_known() {
                    KnownCustom::Name(name_section_reader) => {
                        name_section = Some(
                            name_section_reader
                                .into_iter()
                                .map(|name| name.map_err(Into::into).and_then(Name::parse))
                                .collect::<Result<Vec<_>>>()?,
                        );
                    }
                    _ => {
                        let custom_sections = custom_sections.get_or_insert_with(Vec::new);
                        let name = section.name().to_string().into();
                        let data = section.data().to_vec();
                        custom_sections.push((name, data));
                    }
                },
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
            code_section,
            data_section,
            name_section,
            custom_sections,
        })
    }

    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut module = wasm_encoder::Module::new();

        if let Some(crate_section) = &self.type_section {
            let mut encoder_section = wasm_encoder::TypeSection::new();
            for func_type in crate_section.clone() {
                encoder_section
                    .ty()
                    .func_type(&RoundtripReencoder.func_type(func_type)?);
            }
            module.section(&encoder_section);
        }

        if let Some(crate_section) = &self.import_section {
            let mut encoder_section = wasm_encoder::ImportSection::new();
            for import in crate_section.clone() {
                import.reencode(&mut encoder_section)?;
            }
            module.section(&encoder_section);
        }

        if let Some(crate_section) = &self.function_section {
            let mut encoder_section = wasm_encoder::FunctionSection::new();
            for &function in crate_section {
                encoder_section.function(function);
            }
            module.section(&encoder_section);
        }

        if let Some(table) = &self.table_section {
            let mut encoder_section = wasm_encoder::TableSection::new();
            table.reencode(&mut encoder_section)?;
            module.section(&encoder_section);
        }

        if let Some(memory) = &self.memory_section {
            let mut encoder_section = wasm_encoder::MemorySection::new();
            encoder_section.memory(RoundtripReencoder.memory_type(*memory));
            module.section(&encoder_section);
        }

        if let Some(crate_section) = &self.global_section {
            let mut encoder_section = wasm_encoder::GlobalSection::new();
            for global in crate_section {
                encoder_section.global(
                    RoundtripReencoder.global_type(global.ty)?,
                    &global.init_expr.reencode()?,
                );
            }
            module.section(&encoder_section);
        }

        if let Some(crate_section) = &self.export_section {
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

        if let Some(crate_section) = &self.element_section {
            let mut encoder_section = wasm_encoder::ElementSection::new();
            for element in crate_section {
                element.reencode(&mut encoder_section)?;
            }
            module.section(&encoder_section);
        }

        if let Some(crate_section) = &self.code_section {
            let mut encoder_section = wasm_encoder::CodeSection::new();
            for function in crate_section {
                encoder_section.function(&function.reencode()?);
            }
            module.section(&encoder_section);
        }

        if let Some(crate_section) = &self.data_section {
            let mut encoder_section = wasm_encoder::DataSection::new();
            for data in crate_section {
                encoder_section.active(0, &data.offset_expr.reencode()?, data.data.iter().copied());
            }
            module.section(&encoder_section);
        }

        if let Some(name_section) = &self.name_section {
            let mut encoder_section = wasm_encoder::NameSection::new();
            for name in name_section {
                name.reencode(&mut encoder_section);
            }
            module.section(&encoder_section);
        }

        if let Some(custom_sections) = &self.custom_sections {
            for (name, data) in custom_sections {
                let encoder_section = wasm_encoder::CustomSection {
                    name: Cow::Borrowed(name),
                    data: Cow::Borrowed(data),
                };
                module.section(&encoder_section);
            }
        }

        Ok(module.finish())
    }

    pub fn fix_unsupported_features(code: &[u8]) -> Result<Vec<u8>> {
        let mut import_section = None;
        let mut start_section = None;
        let mut code_section = None;

        let mut parser = wasmparser::Parser::new(0);
        parser.set_features(WasmFeatures::WASM2);

        for payload in parser.parse_all(code) {
            match payload? {
                Payload::ImportSection(section) => {
                    debug_assert!(import_section.is_none());
                    import_section = Some(section.into_iter().collect::<Result<Vec<_>, _>>()?);
                }
                Payload::StartSection { func, range: _ } => {
                    start_section = Some(func);
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
                        .push(entry);
                }
                _ => {}
            }
        }

        let import_count = import_section
            .as_ref()
            .map(|imports| {
                imports
                    .iter()
                    .filter(|import| matches!(import.ty, TypeRef::Func(_)))
                    .count()
            })
            .unwrap_or(0) as u32;

        if let Some(func) = start_section
            && let Some(func) = func.checked_sub(import_count)
            && let Some(code_section) = code_section
            && let Some(entry) = code_section.get(func as usize)
        {
            let mut instructions = Vec::new();
            let mut reader = entry.get_operators_reader()?;
            let start = reader.original_position();
            while !reader.eof() {
                instructions.push(reader.read()?);
            }
            let end = reader.original_position() - 1;

            let mut code_copy = code.to_vec();
            code_copy[start..end].fill(0x01); // NOP opcode

            let mut module = Self::new(&code_copy)?;
            module.start_section.take();

            Ok(module.serialize()?)
        } else {
            Ok(code.to_vec())
        }
    }

    pub fn import_count(&self, pred: impl Fn(&TypeRef) -> bool) -> usize {
        self.import_section
            .as_ref()
            .map(|imports| imports.iter().filter(|import| pred(&import.ty)).count())
            .unwrap_or(0)
    }

    pub fn functions_space(&self) -> usize {
        self.import_count(|ty| matches!(ty, TypeRef::Func(_)))
            + self
                .function_section
                .as_ref()
                .map(|section| section.len())
                .unwrap_or(0)
    }

    pub fn globals_space(&self) -> usize {
        self.import_count(|ty| matches!(ty, TypeRef::Global(_)))
            + self
                .global_section
                .as_ref()
                .map(|section| section.len())
                .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test_parsing_failed {
        (
            $( $test_name:ident: $wat:literal => $err:expr; )*
        ) => {
            $(
                #[test]
                fn $test_name() {
                    let wasm = wat::parse_str($wat).unwrap();
                    let lhs = Module::new(&wasm).unwrap_err();
                    let rhs: ModuleError = $err;
                    // we cannot compare errors directly because `BinaryReaderError` does not implement `PartialEq`
                    assert_eq!(format!("{lhs:?}"), format!("{rhs:?}"));
                }
            )*
        };
    }

    test_parsing_failed! {
        multiple_tables_denied: r#"
        (module
            (table 10 10 funcref)
            (table 20 20 funcref)
        )"# => ModuleError::MultipleTables;

        multiple_memories_denied: r#"
        (module
            (memory (export "memory") 1)
            (memory (export "memory2") 2)
        )"# => ModuleError::MultipleMemories;

        data_non_zero_memory_idx_denied: r#"
        (module
            (data (memory 123) (offset i32.const 0) "")
        )
        "# => ModuleError::NonZeroMemoryIdx(123);

        element_table_idx_denied: r#"
        (module
            (elem 123 (offset i32.const 0) 0 0 0 0)
        )"# => ModuleError::ElementTableIdx(123);

        passive_data_kind_denied: r#"
        (module
            (data "")
        )
        "# => ModuleError::PassiveDataKind;

        passive_element_denied: r#"
        (module
            (elem funcref (item i32.const 0))
        )
        "# => ModuleError::NonActiveElementKind;

        declared_element_denied: r#"
        (module
            (func $a)
            (elem declare func $a)
        )
        "# => ModuleError::NonActiveElementKind;

        element_expressions_denied: r#"
        (module
            (elem (i32.const 1) funcref)
        )
        "# => ModuleError::ElementExpressions;

        table_init_expr_denied: r#"
        (module
            (table 0 0 funcref (i32.const 0))
        )"# => ModuleError::TableInitExpr;
    }

    #[test]
    fn call_indirect_non_zero_table_idx_denied() {
        let wasm = wat::parse_str(
            r#"
            (module
                (func
                    call_indirect 123 (type 333)
                )
            )
            "#,
        )
        .unwrap();
        let err = Module::new(&wasm).unwrap_err();
        if let ModuleError::BinaryReader(err) = err {
            assert_eq!(err.offset(), 26);
            assert_eq!(err.message(), "zero byte expected");
        } else {
            panic!("{err}");
        }
    }

    #[test]
    fn custom_section_kept() {
        let mut builder = ModuleBuilder::default();
        builder.push_custom_section("dummy", [1, 2, 3]);
        let module = builder.build();
        let module_bytes = module.serialize().unwrap();
        let wat = wasmprinter::print_bytes(&module_bytes).unwrap();

        let parsed_module_bytes = Module::new(&module_bytes).unwrap().serialize().unwrap();
        let parsed_wat = wasmprinter::print_bytes(&parsed_module_bytes).unwrap();
        assert_eq!(wat, parsed_wat);
    }
}
