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
    vec,
    vec::Vec,
};
use core::convert::Infallible;
use wasm_encoder::{
    reencode,
    reencode::{Reencode, RoundtripReencoder},
};
use wasmparser::{
    BinaryReader, BinaryReaderError, Encoding, ExternalKind, FuncType, FunctionBody, GlobalType,
    MemoryType, NameSectionReader, Payload, RefType, TableType, TypeRef, ValType, WasmFeatures,
};

macro_rules! define_for_each_instruction_helper {
    ($_:tt;
        proposals { $($proposals:ident,)+ }
        rewrite_fields { $( $ops:ident { $($args:ident: $argsty:ty),* }, )+ }
        forbidden_instructions { $($forbidden_instructions:ident,)+ }
    ) => {
        macro_rules! define_for_each_instruction {
            ($_ ( @$_ proposal:ident $_ op:ident $_ ({ $_ ($_ arg:ident: $_ argty:ty),* })? => $_ visit:ident ($_ ($_ ann:tt)*) )*) => {
                define_for_each_instruction!(inner $_ ( @$_ proposal $_ op $_ ({ $_ ($_ arg: $_ argty),* })? )* accum @accum2);
            };
            // ACCUM: skip forbidden instructions
            $(
                (
                    inner
                    @$_ proposal:ident $forbidden_instructions $_ ({ $_ ($_ arg:ident: $_ argty:ty),* })?
                    $_ ( @$_ proposals:ident $_ ops:ident $_ ({ $_ ($_ args:ident: $_ argsty:ty),* })? )*
                    accum
                    $_ ( $_ ops_accum:ident $_ ({ $_ ($_ args_accum:ident: $_ argsty_accum:ty),* })? )*
                    @accum2
                ) => {
                    define_for_each_instruction!(
                        inner
                        $_ ( @$_ proposals $_ ops $_ ({ $_ ($_ args: $_ argsty),* })? )*
                        accum
                        $_ ( $_ ops_accum $_ ({ $_ ($_ args_accum: $_ argsty_accum),* })? )*
                        @accum2
                    );
                };
            )+
            // ACCUM: use only specific proposals
            $(
                (
                    inner
                    @$proposals $_ op:ident $_ ({ $_ ($_ arg:ident: $_ argty:ty),* })?
                    $_ ( @$_ proposals:ident $_ ops:ident $_ ({ $_ ($_ args:ident: $_ argsty:ty),* })? )*
                    accum
                    $_ ( $_ ops_accum:ident $_ ({ $_ ($_ args_accum:ident: $_ argsty_accum:ty),* })? )*
                    @accum2
                ) => {
                    define_for_each_instruction!(
                        inner
                        $_ ( @$_ proposals $_ ops $_ ({ $_ ($_ args: $_ argsty),* })? )*
                        accum
                        $_ op $_ ({ $_ ( $_ arg: $_ argty ),* })?
                        $_ ( $_ ops_accum $_ ({ $_ ($_ args_accum: $_ argsty_accum),* })? )*
                        @accum2
                    );
                };
            )+
            // ACCUM: skip rest instructions
            (
                inner
                @$_ proposal:ident $_ op:ident $_ ({ $_ ($_ arg:ident: $_ argty:ty),* })?
                $_ ( @$_ proposals:ident $_ ops:ident $_ ({ $_ ($_ args:ident: $_ argsty:ty),* })? )*
                accum
                $_ ( $_ ops_accum:ident $_ ({ $_ ($_ args_accum:ident: $_ argsty_accum:ty),* })? )*
                @accum2
            ) => {
                define_for_each_instruction!(
                    inner
                    $_ ( @$_ proposals $_ ops $_ ({ $_ ($_ args: $_ argsty),* })? )*
                    accum
                    $_ ( $_ ops_accum $_ ({ $_ ($_ args_accum: $_ argsty_accum),* })? )*
                    @accum2
                );
            };
            // @accum2: rewrite instructions fields
            $(
                (
                    inner
                    accum
                    $_ op:ident { $($args: $_ argty:ty),* }
                    $_ ( $_ ops:ident $_ ({ $_ ($_ args:ident: $_ argsty:ty),* })? )*
                    @accum2
                    $_ ( $_ ops_accum:ident $_ ({ $_ ($_ args_accum:ident: $_ argsty_accum:ty),* })? )*
                ) => {
                    define_for_each_instruction!(
                        inner
                        accum
                        $_ ( $_ ops $_ ({ $_ ($_ args: $_ argsty),* })? )*
                        @accum2
                        $_ op { $($args: $argsty),* }
                        $_ ( $_ ops_accum $_ ({ $_ ($_ args_accum: $_ argsty_accum),* })? )*
                    );
                };
            )+
            // @accum2: accumulate rest instructions from `accum` to `@accum2`
            (
                inner
                accum
                $_ op:ident $_ ({ $_ ($_ arg:ident: $_ argty:ty),* })?
                $_ ( $_ ops:ident $_ ({ $_ ($_ args:ident: $_ argsty:ty),* })? )*
                @accum2
                $_ ( $_ ops_accum:ident $_ ({ $_ ($_ args_accum:ident: $_ argsty_accum:ty),* })? )*
            ) => {
                define_for_each_instruction!(
                    inner
                    accum
                    $_ ( $_ ops $_ ({ $_ ($_ args: $_ argsty),* })? )*
                    @accum2
                    $_ op $_ ({ $_ ( $_ arg: $_ argty ),* })?
                    $_ ( $_ ops_accum $_ ({ $_ ($_ args_accum: $_ argsty_accum),* })? )*
                );
            };
            (
                inner
                accum
                @accum2
                $_ ( $_ op:ident $_ ({ $_ ($_ arg:ident: $_ argty:ty),* })? )*
            ) => {
                #[macro_export]
                macro_rules! for_each_instruction {
                    ($_ mac:ident) => {
                        $_ mac! {
                            $_ ( $_ op $_ ({ $_ ($_ arg: $_ argty),* })? )*
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
        define_instruction!(@convert $( $op $({ $($arg: $argty),* })? )* @accum);
    };
    // convert 2 or more arguments
    (
        @convert
        $op:ident $({ $first_arg:ident: $first_argty:ty, $($arg:ident: $argty:ty),+ })?
        $( $ops:ident $({ $($args:ident: $argsty:ty),* })? )*
        @accum
        $( $accum_ops:ident $({ $( $accum_args_helper:tt => $accum_args:ident: $accum_argsty:ty ),+ })? [ $( $($accum_tt:tt)+ )? ] )*
    ) => {
        define_instruction!(
            @convert
            $( $ops $({ $($args: $argsty),* })? )*
            @accum
            $op $({ $first_arg => $first_arg: $first_argty, $( $arg => $arg: $argty),+ })? [ $( { $first_arg: $first_argty, $($arg: $argty),+ } )? ]
            $( $accum_ops $({ $( $accum_args_helper => $accum_args: $accum_argsty ),+ })? [ $( $($accum_tt)+ )? ] )*
        );
    };
    // convert iff only 1 argument
    (
        @convert
        $op:ident $({ $arg:ident: $argty:ty })?
        $( $ops:ident $({ $($args:ident: $argsty:ty),* })? )*
        @accum
        $( $accum_ops:ident $({ $( $accum_args_helper:tt => $accum_args:ident: $accum_argsty:ty ),+ })? [ $( $($accum_tt:tt)+ )? ] )*
    ) => {
        define_instruction!(
            @convert
            $( $ops $({ $($args: $argsty),* })? )*
            @accum
            $op $({ 0 => $arg: $argty })? [ $( ($argty) )? ]
            $( $accum_ops $({ $( $accum_args_helper => $accum_args: $accum_argsty ),+ })? [ $( $($accum_tt)+ )? ] )*
        );
    };
    (
        @convert
        @accum
        $( $op:ident $({ $( $helper_arg:tt => $arg:ident: $argty:ty ),+ })? [ $( $($accum_tt:tt)+ )? ] )*
    ) => {
        #[derive(Debug, Clone, Eq, PartialEq)]
        pub enum Instruction {
            $(
                $op $( $($accum_tt)+ )?,
            )*
        }

        impl Instruction {
            fn parse(op: wasmparser::Operator) -> Result<Self> {
                match op {
                    $(
                        wasmparser::Operator::$op $({ $($arg),* })? => {
                            Ok(Self::$op $({ $($helper_arg: <_>::try_from($arg)?),* })?)
                        }
                    )*
                    op => Err(ModuleError::UnsupportedInstruction(format!("{op:?}"))),
                }
            }

            fn reencode(&self) -> Result<wasm_encoder::Instruction> {
                Ok(match self {
                    $(
                        Self::$op $( { $($helper_arg: $arg),+ } )? => {
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
    #[display(fmt = "Multiple tables")]
    MultipleTables,
    #[display(fmt = "Multiple memories")]
    MultipleMemories,
    #[display(fmt = "Memory index must be non-zero (actual: {})", _0)]
    NonZeroMemoryIdx(u32),
    #[display(fmt = "Passive data is not supported")]
    PassiveDataKind,
    #[display(fmt = "Element expressions are not supported")]
    ElementExpressions,
    #[display(fmt = "Only active element is supported")]
    NonActiveElementKind,
    #[display(fmt = "Table init expression is not supported")]
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

#[derive(Default, Clone, derive_more::DebugCustom, Eq, PartialEq)]
#[debug(fmt = "ConstExpr {{ .. }}")]
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

    pub fn memory(
        module: impl Into<Cow<'static, str>>,
        name: impl Into<Cow<'static, str>>,
        initial: u32,
        maximum: Option<u32>,
    ) -> Self {
        Self {
            module: module.into(),
            name: name.into(),
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
    Active {
        table_index: Option<u32>,
        offset_expr: ConstExpr,
    },
}

impl ElementKind {
    fn parse(kind: wasmparser::ElementKind) -> Result<Self> {
        match kind {
            wasmparser::ElementKind::Passive => Err(ModuleError::NonActiveElementKind),
            wasmparser::ElementKind::Active {
                table_index,
                offset_expr,
            } => Ok(Self::Active {
                table_index,
                offset_expr: ConstExpr::parse(offset_expr)?,
            }),
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
                table_index: Some(0),
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
            ElementKind::Active {
                table_index,
                offset_expr,
            } => {
                encoder_section.active(*table_index, &offset_expr.reencode()?, items);
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
                    if let Instruction::Call(function_index) = instruction {
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
                }
            }
        }

        if let Some(start_idx) = &mut self.module.start_section {
            if *start_idx >= inserted_index {
                *start_idx += inserted_count
            }
        }

        if let Some(section) = self.module.name_section_mut() {
            for name in section {
                if let Name::Function(map) = name {
                    for naming in map {
                        if naming.index >= inserted_index {
                            naming.index += inserted_count;
                        }
                    }
                }
            }
        }

        self
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

    fn custom_section(&mut self) -> &mut Vec<CustomSection> {
        self.module.custom_section.get_or_insert_with(Vec::new)
    }

    pub fn push_custom_section(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        data: impl Into<Vec<u8>>,
    ) {
        self.custom_section().push((name.into(), data.into()));
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

#[derive(derive_more::DebugCustom, Clone, Default)]
#[debug(fmt = "Module {{ .. }}")]
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
    pub custom_section: Option<Vec<CustomSection>>,
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

        let mut parser = wasmparser::Parser::new(0);
        parser.set_features(
            (WasmFeatures::WASM1 | WasmFeatures::SIGN_EXTENSION) & !WasmFeatures::FLOATS,
        );
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
                Payload::CustomSection(section) => {
                    // we avoid usage of `CustomSectionReader::as_known()`
                    // because compiler is unable to remove branch with `CoreDumpSection`
                    // which reads f32 and f64 values
                    if section.name() == "name" {
                        let section = NameSectionReader::new(BinaryReader::new(section.data(), 0));
                        name_section = Some(
                            section
                                .into_iter()
                                .map(|name| name.map_err(Into::into).and_then(Name::parse))
                                .collect::<Result<Vec<_>>>()?,
                        );
                    }
                }
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
            custom_section: None,
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

        if let Some(table) = self.table_section() {
            let mut encoder_section = wasm_encoder::TableSection::new();
            table.reencode(&mut encoder_section)?;
            module.section(&encoder_section);
        }

        if let Some(memory) = self.memory_section() {
            let mut encoder_section = wasm_encoder::MemorySection::new();
            encoder_section.memory(RoundtripReencoder.memory_type(*memory));
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

        if let Some(crate_section) = self.code_section() {
            let mut encoder_section = wasm_encoder::CodeSection::new();
            for function in crate_section {
                encoder_section.function(&function.reencode()?);
            }
            module.section(&encoder_section);
        }

        if let Some(crate_section) = self.data_section() {
            let mut encoder_section = wasm_encoder::DataSection::new();
            for data in crate_section {
                encoder_section.active(0, &data.offset_expr.reencode()?, data.data.iter().copied());
            }
            module.section(&encoder_section);
        }

        if let Some(name_section) = self.name_section() {
            let mut encoder_section = wasm_encoder::NameSection::new();
            for name in name_section {
                name.reencode(&mut encoder_section);
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

    pub fn table_section(&self) -> Option<&Table> {
        self.table_section.as_ref()
    }

    pub fn table_section_mut(&mut self) -> Option<&mut Table> {
        self.table_section.as_mut()
    }

    pub fn memory_section(&self) -> Option<&MemoryType> {
        self.memory_section.as_ref()
    }

    pub fn memory_section_mut(&mut self) -> Option<&mut MemoryType> {
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
    pub fn code_section(&self) -> Option<&Vec<Function>> {
        self.code_section.as_ref()
    }

    pub fn code_section_mut(&mut self) -> Option<&mut CodeSection> {
        self.code_section.as_mut()
    }

    pub fn data_section(&self) -> Option<&DataSection> {
        self.data_section.as_ref()
    }

    pub fn data_section_mut(&mut self) -> Option<&mut DataSection> {
        self.data_section.as_mut()
    }

    pub fn name_section(&self) -> Option<&Vec<Name>> {
        self.name_section.as_ref()
    }

    pub fn name_section_mut(&mut self) -> Option<&mut Vec<Name>> {
        self.name_section.as_mut()
    }

    pub fn custom_section(&self) -> Option<&Vec<CustomSection>> {
        self.custom_section.as_ref()
    }

    pub fn custom_section_mut(&mut self) -> Option<&mut Vec<CustomSection>> {
        self.custom_section.as_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test_parsing_failed {
        (
            $( $test_name:ident: $wat:literal => $err_msg:literal; )*
        ) => {
            $(
                #[test]
                #[should_panic = $err_msg]
                fn $test_name() {
                    let wasm = wat::parse_str($wat).unwrap();
                    let _module = Module::new(&wasm).unwrap();
                }
            )*
        };
    }

    test_parsing_failed! {
        multiple_tables_denied: r#"
        (module
            (table 10 10 funcref)
            (table 20 20 funcref)
        )"# => "MultipleTables";

        multiple_memories_denied: r#"
        (module
            (memory (export "memory") 1)
            (memory (export "memory2") 2)
        )"# => "MultipleMemories";

        data_non_zero_memory_idx: r#"
        (module
            (data (memory 123) (offset i32.const 0) "")
        )
        "# => "NonZeroMemoryIdx(123)";

        passive_data_kind_denied: r#"
        (module
            (data "")
        )
        "# => "PassiveDataKind";

        passive_element_denied: r#"
        (module
            (elem funcref (item i32.const 0))
        )
        "# => "NonActiveElementKind";

        declared_element_denied: r#"
        (module
            (func $a)
            (elem declare func $a)
        )
        "# => "NonActiveElementKind";

        element_expressions_denied: r#"
        (module
            (elem (i32.const 1) funcref)
        )
        "# => "ElementExpressions";

        table_init_expr_denied: r#"
        (module
            (table 0 0 funcref (i32.const 0))
        )"# => "TableInitExpr";
    }
}
