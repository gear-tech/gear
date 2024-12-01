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
    BlockType, Catch, CodeSection, DataSection, DataSegment, DataSegmentMode, ElementMode,
    ElementSection, ElementSegment, Elements, EntityType, ExportKind, Function, GlobalSection,
    ImportSection, Instruction, MemArg, ValType,
};
use wasmparser::{
    types::TypeIdentifier, AbstractHeapType, BinaryReaderError, DataKind, ElementItems,
    ElementKind, ExternalKind, FunctionBody, Global, Import, Operator, Ordering, UnpackedIndex,
};

pub type Result<T, E = Error> = core::result::Result<T, E>;

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
    #[display(fmt = "Binary reader error: {}", _0)]
    BinaryReader(BinaryReaderError),
}

#[derive(Debug, Hash, Eq, PartialEq, Copy, Clone)]
pub enum Item {
    Function,
    Table,
    Memory,
    Tag,
    Global,
    Type,
    Data,
    Element,
}

#[derive(Debug, Hash, Eq, PartialEq, Copy, Clone)]
pub enum ConstExprKind {
    Global,
    ElementOffset,
    ElementFunction,

    DataOffset,
}

pub fn translate_import_def(ty: Import, s: &mut ImportSection) -> Result<()> {
    let new_ty = match ty.ty {
        wasmparser::TypeRef::Func(v) => EntityType::Function(v),
        wasmparser::TypeRef::Tag(v) => EntityType::Tag(translate_tag_type(&v)?),
        wasmparser::TypeRef::Global(v) => EntityType::Global(translate_global_type(&v)?),
        wasmparser::TypeRef::Table(v) => EntityType::Table(translate_table_type(&v)?),
        wasmparser::TypeRef::Memory(v) => EntityType::Memory(translate_memory_type(&v)?),
    };
    s.import(ty.module, ty.name, new_ty);
    Ok(())
}

pub fn translate_table_type(ty: &wasmparser::TableType) -> Result<wasm_encoder::TableType> {
    Ok(wasm_encoder::TableType {
        element_type: translate_ref_type(&ty.element_type)?,
        table64: ty.table64,
        minimum: ty.initial,
        maximum: ty.maximum,
        shared: ty.shared,
    })
}

pub fn translate_memory_type(ty: &wasmparser::MemoryType) -> Result<wasm_encoder::MemoryType> {
    Ok(wasm_encoder::MemoryType {
        memory64: ty.memory64,
        minimum: ty.initial,
        maximum: ty.maximum,
        shared: ty.shared,
        page_size_log2: None,
    })
}

pub fn translate_global_type(ty: &wasmparser::GlobalType) -> Result<wasm_encoder::GlobalType> {
    Ok(wasm_encoder::GlobalType {
        val_type: translate_val_type(&ty.content_type)?,
        mutable: ty.mutable,
        shared: ty.shared,
    })
}

pub fn translate_tag_type(ty: &wasmparser::TagType) -> Result<wasm_encoder::TagType> {
    debug_assert_eq!(ty.kind, wasmparser::TagKind::Exception);
    Ok(wasm_encoder::TagType {
        kind: wasm_encoder::TagKind::Exception,
        func_type_idx: ty.func_type_idx,
    })
}

pub fn translate_heap_type(ty: &wasmparser::HeapType) -> Result<wasm_encoder::HeapType> {
    Ok(match ty {
        wasmparser::HeapType::Abstract { shared, ty } => wasm_encoder::HeapType::Abstract {
            shared: *shared,
            ty: match ty {
                AbstractHeapType::Func => wasm_encoder::AbstractHeapType::Func,
                AbstractHeapType::Extern => wasm_encoder::AbstractHeapType::Extern,
                AbstractHeapType::Any => wasm_encoder::AbstractHeapType::Any,
                AbstractHeapType::None => wasm_encoder::AbstractHeapType::None,
                AbstractHeapType::NoExtern => wasm_encoder::AbstractHeapType::NoExtern,
                AbstractHeapType::NoFunc => wasm_encoder::AbstractHeapType::NoFunc,
                AbstractHeapType::Eq => wasm_encoder::AbstractHeapType::Eq,
                AbstractHeapType::Struct => wasm_encoder::AbstractHeapType::Struct,
                AbstractHeapType::Array => wasm_encoder::AbstractHeapType::Array,
                AbstractHeapType::I31 => wasm_encoder::AbstractHeapType::I31,
                AbstractHeapType::Exn => wasm_encoder::AbstractHeapType::Exn,
                AbstractHeapType::NoExn => wasm_encoder::AbstractHeapType::NoExn,
                AbstractHeapType::Cont => wasm_encoder::AbstractHeapType::Cont,
                AbstractHeapType::NoCont => wasm_encoder::AbstractHeapType::NoCont,
            },
        },
        wasmparser::HeapType::Concrete(idx) => wasm_encoder::HeapType::Concrete(match idx {
            UnpackedIndex::Module(i) => *i,
            UnpackedIndex::RecGroup(i) => *i,
            UnpackedIndex::Id(i) => i.index() as u32,
        }),
    })
}

pub fn translate_ref_type(ty: &wasmparser::RefType) -> Result<wasm_encoder::RefType> {
    Ok(wasm_encoder::RefType {
        nullable: ty.is_nullable(),
        heap_type: translate_heap_type(&ty.heap_type())?,
    })
}

pub fn translate_val_type(ty: &wasmparser::ValType) -> Result<ValType> {
    match ty {
        wasmparser::ValType::I32 => Ok(ValType::I32),
        wasmparser::ValType::I64 => Ok(ValType::I64),
        wasmparser::ValType::F32 => Ok(ValType::F32),
        wasmparser::ValType::F64 => Ok(ValType::F64),
        wasmparser::ValType::V128 => Ok(ValType::V128),
        wasmparser::ValType::Ref(ty) => Ok(ValType::Ref(translate_ref_type(ty)?)),
    }
}

pub fn translate_global(global: Global, s: &mut GlobalSection) -> Result<()> {
    let ty = translate_global_type(&global.ty)?;
    let expr = translate_const_expr(&global.init_expr)?;
    s.global(ty, &expr);
    Ok(())
}

pub fn translate_export_kind(kind: ExternalKind) -> Result<ExportKind> {
    match kind {
        ExternalKind::Table => Ok(ExportKind::Table),
        ExternalKind::Global => Ok(ExportKind::Global),
        ExternalKind::Tag => Ok(ExportKind::Tag),
        ExternalKind::Func => Ok(ExportKind::Func),
        ExternalKind::Memory => Ok(ExportKind::Memory),
    }
}

pub fn translate_export(
    e: &wasmparser::Export<'_>,
    sec: &mut wasm_encoder::ExportSection,
) -> Result<()> {
    sec.export(e.name, translate_export_kind(e.kind)?, e.index);
    Ok(())
}

pub fn translate_const_expr(expr: &wasmparser::ConstExpr<'_>) -> Result<wasm_encoder::ConstExpr> {
    let instructions = expr
        .get_operators_reader()
        .into_iter()
        .map(|op| op.map_err(Into::into).and_then(|op| translate_op(&op)))
        .collect::<Result<Vec<_>>>()?;
    Ok(wasm_encoder::ConstExpr::extended(instructions))
}

pub fn element(element: wasmparser::Element<'_>, s: &mut ElementSection) -> Result<()> {
    let mode = match &element.kind {
        ElementKind::Active {
            table_index,
            offset_expr,
        } => ElementMode::Active {
            table: *table_index,
            offset: &translate_const_expr(offset_expr)?,
        },
        ElementKind::Passive => ElementMode::Passive,
        ElementKind::Declared => ElementMode::Declared,
    };
    let elements = match element.items {
        ElementItems::Functions(array) => Elements::Functions(
            array
                .into_iter()
                .collect::<Result<_, BinaryReaderError>>()?,
        ),
        ElementItems::Expressions(ty, exprs) => Elements::Expressions(
            translate_ref_type(&ty)?,
            exprs
                .into_iter()
                .map(|expr| {
                    expr.map_err(Into::into)
                        .and_then(|expr| translate_const_expr(&expr))
                })
                .collect::<Result<_>>()?,
        ),
    };

    s.segment(ElementSegment { mode, elements });
    Ok(())
}

/// This is a pretty gnarly function that translates from `wasmparser`
/// operators to `wasm_encoder` operators. It's quite large because there's
/// quite a few wasm instructions. The theory though is that at least each
/// individual case is pretty self-contained.
pub fn translate_op(op: &Operator<'_>) -> Result<Instruction<'static>> {
    use wasm_encoder::Instruction as I;
    use wasmparser::Operator as O;
    Ok(match op {
        O::Unreachable => I::Unreachable,
        O::Nop => I::Nop,
        O::Block { blockty } => I::Block(translate_block_type(blockty)?),
        O::Loop { blockty } => I::Loop(translate_block_type(blockty)?),
        O::If { blockty } => I::If(translate_block_type(blockty)?),
        O::Else => I::Else,
        O::TryTable { try_table } => I::TryTable(
            translate_block_type(&try_table.ty)?,
            try_table
                .catches
                .iter()
                .map(|c| match c {
                    wasmparser::Catch::One { tag, label } => Catch::One {
                        tag: *tag,
                        label: *label,
                    },
                    wasmparser::Catch::OneRef { tag, label } => Catch::OneRef {
                        tag: *tag,
                        label: *label,
                    },
                    wasmparser::Catch::All { label } => Catch::All { label: *label },
                    wasmparser::Catch::AllRef { label } => Catch::All { label: *label },
                })
                .collect(),
        ),
        O::Throw { tag_index } => I::Throw(*tag_index),
        O::ThrowRef => I::ThrowRef,
        O::Try { blockty } => I::Try(translate_block_type(blockty)?),
        O::Catch { tag_index } => I::Catch(*tag_index),
        O::Rethrow { relative_depth } => I::Rethrow(*relative_depth),
        O::Delegate { relative_depth } => I::Delegate(*relative_depth),
        O::CatchAll => I::CatchAll,
        O::End => I::End,
        O::Br { relative_depth } => I::Br(*relative_depth),
        O::BrIf { relative_depth } => I::BrIf(*relative_depth),
        O::BrTable { targets } => I::BrTable(
            targets
                .targets()
                .into_iter()
                .collect::<Result<Vec<_>, BinaryReaderError>>()?
                .into(),
            targets.default(),
        ),
        O::Return => I::Return,
        O::Call { function_index } => I::Call(*function_index),
        O::CallIndirect {
            type_index,
            table_index,
        } => I::CallIndirect {
            type_index: *type_index,
            table_index: *table_index,
        },
        O::ReturnCall { function_index } => I::ReturnCall(*function_index),
        O::ReturnCallIndirect {
            type_index,
            table_index,
        } => I::ReturnCallIndirect {
            type_index: *type_index,
            table_index: *table_index,
        },
        O::Drop => I::Drop,
        O::Select => I::Select,
        O::TypedSelect { ty } => I::TypedSelect(translate_val_type(ty)?),
        O::LocalGet { local_index } => I::LocalGet(*local_index),
        O::LocalSet { local_index } => I::LocalSet(*local_index),
        O::LocalTee { local_index } => I::LocalTee(*local_index),
        O::GlobalGet { global_index } => I::GlobalGet(*global_index),
        O::GlobalSet { global_index } => I::GlobalSet(*global_index),
        O::I32Load { memarg } => I::I32Load(translate_memarg(memarg)?),
        O::I64Load { memarg } => I::I64Load(translate_memarg(memarg)?),
        O::F32Load { memarg } => I::F32Load(translate_memarg(memarg)?),
        O::F64Load { memarg } => I::F64Load(translate_memarg(memarg)?),
        O::I32Load8S { memarg } => I::I32Load8S(translate_memarg(memarg)?),
        O::I32Load8U { memarg } => I::I32Load8U(translate_memarg(memarg)?),
        O::I32Load16S { memarg } => I::I32Load16S(translate_memarg(memarg)?),
        O::I32Load16U { memarg } => I::I32Load16U(translate_memarg(memarg)?),
        O::I64Load8S { memarg } => I::I64Load8S(translate_memarg(memarg)?),
        O::I64Load8U { memarg } => I::I64Load8U(translate_memarg(memarg)?),
        O::I64Load16S { memarg } => I::I64Load16S(translate_memarg(memarg)?),
        O::I64Load16U { memarg } => I::I64Load16U(translate_memarg(memarg)?),
        O::I64Load32S { memarg } => I::I64Load32S(translate_memarg(memarg)?),
        O::I64Load32U { memarg } => I::I64Load32U(translate_memarg(memarg)?),
        O::I32Store { memarg } => I::I32Store(translate_memarg(memarg)?),
        O::I64Store { memarg } => I::I64Store(translate_memarg(memarg)?),
        O::F32Store { memarg } => I::F32Store(translate_memarg(memarg)?),
        O::F64Store { memarg } => I::F64Store(translate_memarg(memarg)?),
        O::I32Store8 { memarg } => I::I32Store8(translate_memarg(memarg)?),
        O::I32Store16 { memarg } => I::I32Store16(translate_memarg(memarg)?),
        O::I64Store8 { memarg } => I::I64Store8(translate_memarg(memarg)?),
        O::I64Store16 { memarg } => I::I64Store16(translate_memarg(memarg)?),
        O::I64Store32 { memarg } => I::I64Store32(translate_memarg(memarg)?),
        O::MemorySize { mem } => I::MemorySize(*mem),
        O::MemoryGrow { mem } => I::MemoryGrow(*mem),
        O::I32Const { value } => I::I32Const(*value),
        O::I64Const { value } => I::I64Const(*value),
        O::F32Const { value } => I::F32Const(value.into()),
        O::F64Const { value } => I::F64Const(value.into()),
        O::RefNull { hty } => I::RefNull(translate_heap_type(hty)?),
        O::RefIsNull => I::RefIsNull,
        O::RefFunc { function_index } => I::RefFunc(*function_index),
        O::RefEq => I::RefEq,
        O::I32Eqz => I::I32Eqz,
        O::I32Eq => I::I32Eq,
        O::I32Ne => I::I32Ne,
        O::I32LtS => I::I32LtS,
        O::I32LtU => I::I32LtU,
        O::I32GtS => I::I32GtS,
        O::I32GtU => I::I32GtU,
        O::I32LeS => I::I32LeS,
        O::I32LeU => I::I32LeU,
        O::I32GeS => I::I32GeS,
        O::I32GeU => I::I32GeU,
        O::I64Eqz => I::I64Eqz,
        O::I64Eq => I::I64Eq,
        O::I64Ne => I::I64Ne,
        O::I64LtS => I::I64LtS,
        O::I64LtU => I::I64LtU,
        O::I64GtS => I::I64GtS,
        O::I64GtU => I::I64GtU,
        O::I64LeS => I::I64LeS,
        O::I64LeU => I::I64LeU,
        O::I64GeS => I::I64GeS,
        O::I64GeU => I::I64GeU,
        O::F32Eq => I::F32Eq,
        O::F32Ne => I::F32Ne,
        O::F32Lt => I::F32Lt,
        O::F32Gt => I::F32Gt,
        O::F32Le => I::F32Le,
        O::F32Ge => I::F32Ge,
        O::F64Eq => I::F64Eq,
        O::F64Ne => I::F64Ne,
        O::F64Lt => I::F64Lt,
        O::F64Gt => I::F64Gt,
        O::F64Le => I::F64Le,
        O::F64Ge => I::F64Ge,
        O::I32Clz => I::I32Clz,
        O::I32Ctz => I::I32Ctz,
        O::I32Popcnt => I::I32Popcnt,
        O::I32Add => I::I32Add,
        O::I32Sub => I::I32Sub,
        O::I32Mul => I::I32Mul,
        O::I32DivS => I::I32DivS,
        O::I32DivU => I::I32DivU,
        O::I32RemS => I::I32RemS,
        O::I32RemU => I::I32RemU,
        O::I32And => I::I32And,
        O::I32Or => I::I32Or,
        O::I32Xor => I::I32Xor,
        O::I32Shl => I::I32Shl,
        O::I32ShrS => I::I32ShrS,
        O::I32ShrU => I::I32ShrU,
        O::I32Rotl => I::I32Rotl,
        O::I32Rotr => I::I32Rotr,
        O::I64Clz => I::I64Clz,
        O::I64Ctz => I::I64Ctz,
        O::I64Popcnt => I::I64Popcnt,
        O::I64Add => I::I64Add,
        O::I64Sub => I::I64Sub,
        O::I64Mul => I::I64Mul,
        O::I64DivS => I::I64DivS,
        O::I64DivU => I::I64DivU,
        O::I64RemS => I::I64RemS,
        O::I64RemU => I::I64RemU,
        O::I64And => I::I64And,
        O::I64Or => I::I64Or,
        O::I64Xor => I::I64Xor,
        O::I64Shl => I::I64Shl,
        O::I64ShrS => I::I64ShrS,
        O::I64ShrU => I::I64ShrU,
        O::I64Rotl => I::I64Rotl,
        O::I64Rotr => I::I64Rotr,
        O::F32Abs => I::F32Abs,
        O::F32Neg => I::F32Neg,
        O::F32Ceil => I::F32Ceil,
        O::F32Floor => I::F32Floor,
        O::F32Trunc => I::F32Trunc,
        O::F32Nearest => I::F32Nearest,
        O::F32Sqrt => I::F32Sqrt,
        O::F32Add => I::F32Add,
        O::F32Sub => I::F32Sub,
        O::F32Mul => I::F32Mul,
        O::F32Div => I::F32Div,
        O::F32Min => I::F32Min,
        O::F32Max => I::F32Max,
        O::F32Copysign => I::F32Copysign,
        O::F64Abs => I::F64Abs,
        O::F64Neg => I::F64Neg,
        O::F64Ceil => I::F64Ceil,
        O::F64Floor => I::F64Floor,
        O::F64Trunc => I::F64Trunc,
        O::F64Nearest => I::F64Nearest,
        O::F64Sqrt => I::F64Sqrt,
        O::F64Add => I::F64Add,
        O::F64Sub => I::F64Sub,
        O::F64Mul => I::F64Mul,
        O::F64Div => I::F64Div,
        O::F64Min => I::F64Min,
        O::F64Max => I::F64Max,
        O::F64Copysign => I::F64Copysign,
        O::I32WrapI64 => I::I32WrapI64,
        O::I32TruncF32S => I::I32TruncF32S,
        O::I32TruncF32U => I::I32TruncF32U,
        O::I32TruncF64S => I::I32TruncF64S,
        O::I32TruncF64U => I::I32TruncF64U,
        O::I64ExtendI32S => I::I64ExtendI32S,
        O::I64ExtendI32U => I::I64ExtendI32U,
        O::I64TruncF32S => I::I64TruncF32S,
        O::I64TruncF32U => I::I64TruncF32U,
        O::I64TruncF64S => I::I64TruncF64S,
        O::I64TruncF64U => I::I64TruncF64U,
        O::F32ConvertI32S => I::F32ConvertI32S,
        O::F32ConvertI32U => I::F32ConvertI32U,
        O::F32ConvertI64S => I::F32ConvertI64S,
        O::F32ConvertI64U => I::F32ConvertI64U,
        O::F32DemoteF64 => I::F32DemoteF64,
        O::F64ConvertI32S => I::F64ConvertI32S,
        O::F64ConvertI32U => I::F64ConvertI32U,
        O::F64ConvertI64S => I::F64ConvertI64S,
        O::F64ConvertI64U => I::F64ConvertI64U,
        O::F64PromoteF32 => I::F64PromoteF32,
        O::I32ReinterpretF32 => I::I32ReinterpretF32,
        O::I64ReinterpretF64 => I::I64ReinterpretF64,
        O::F32ReinterpretI32 => I::F32ReinterpretI32,
        O::F64ReinterpretI64 => I::F64ReinterpretI64,
        O::I32Extend8S => I::I32Extend8S,
        O::I32Extend16S => I::I32Extend16S,
        O::I64Extend8S => I::I64Extend8S,
        O::I64Extend16S => I::I64Extend16S,
        O::I64Extend32S => I::I64Extend32S,
        O::StructNew { struct_type_index } => I::StructNew(*struct_type_index),
        O::StructNewDefault { struct_type_index } => I::StructNewDefault(*struct_type_index),
        O::StructGet {
            struct_type_index,
            field_index,
        } => I::StructGet {
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::StructGetS {
            struct_type_index,
            field_index,
        } => I::StructGetS {
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::StructGetU {
            struct_type_index,
            field_index,
        } => I::StructGetU {
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::StructSet {
            struct_type_index,
            field_index,
        } => I::StructSet {
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::ArrayNew { array_type_index } => I::ArrayNew(*array_type_index),
        O::ArrayNewDefault { array_type_index } => I::ArrayNewDefault(*array_type_index),
        O::ArrayNewFixed {
            array_type_index,
            array_size,
        } => I::ArrayNewFixed {
            array_type_index: *array_type_index,
            array_size: *array_size,
        },
        O::ArrayNewData {
            array_type_index,
            array_data_index,
        } => I::ArrayNewData {
            array_type_index: *array_type_index,
            array_data_index: *array_data_index,
        },
        O::ArrayNewElem {
            array_type_index,
            array_elem_index,
        } => I::ArrayNewElem {
            array_type_index: *array_type_index,
            array_elem_index: *array_elem_index,
        },
        O::ArrayGet { array_type_index } => I::ArrayGet(*array_type_index),
        O::ArrayGetS { array_type_index } => I::ArrayGetS(*array_type_index),
        O::ArrayGetU { array_type_index } => I::ArrayGetU(*array_type_index),
        O::ArraySet { array_type_index } => I::ArraySet(*array_type_index),
        O::ArrayLen => I::ArrayLen,
        O::ArrayFill { array_type_index } => I::ArrayFill(*array_type_index),
        O::ArrayCopy {
            array_type_index_dst,
            array_type_index_src,
        } => I::ArrayCopy {
            array_type_index_dst: *array_type_index_dst,
            array_type_index_src: *array_type_index_src,
        },
        O::ArrayInitData {
            array_type_index,
            array_data_index,
        } => I::ArrayInitData {
            array_type_index: *array_type_index,
            array_data_index: *array_data_index,
        },
        O::ArrayInitElem {
            array_type_index,
            array_elem_index,
        } => I::ArrayInitElem {
            array_type_index: *array_type_index,
            array_elem_index: *array_elem_index,
        },
        O::RefTestNonNull { hty } => I::RefTestNonNull(translate_heap_type(hty)?),
        O::RefTestNullable { hty } => I::RefTestNullable(translate_heap_type(hty)?),
        O::RefCastNonNull { hty } => I::RefCastNonNull(translate_heap_type(hty)?),
        O::RefCastNullable { hty } => I::RefCastNullable(translate_heap_type(hty)?),
        O::BrOnCast {
            relative_depth,
            from_ref_type,
            to_ref_type,
        } => I::BrOnCast {
            relative_depth: *relative_depth,
            from_ref_type: translate_ref_type(from_ref_type)?,
            to_ref_type: translate_ref_type(to_ref_type)?,
        },
        O::BrOnCastFail {
            relative_depth,
            from_ref_type,
            to_ref_type,
        } => I::BrOnCastFail {
            relative_depth: *relative_depth,
            from_ref_type: translate_ref_type(from_ref_type)?,
            to_ref_type: translate_ref_type(to_ref_type)?,
        },
        O::AnyConvertExtern => I::AnyConvertExtern,
        O::ExternConvertAny => I::ExternConvertAny,
        O::RefI31 => I::RefI31,
        O::I31GetS => I::I31GetS,
        O::I31GetU => I::I31GetU,
        O::I32TruncSatF32S => I::I32TruncSatF32S,
        O::I32TruncSatF32U => I::I32TruncSatF32U,
        O::I32TruncSatF64S => I::I32TruncSatF64S,
        O::I32TruncSatF64U => I::I32TruncSatF64U,
        O::I64TruncSatF32S => I::I64TruncSatF32S,
        O::I64TruncSatF32U => I::I64TruncSatF32U,
        O::I64TruncSatF64S => I::I64TruncSatF64S,
        O::I64TruncSatF64U => I::I64TruncSatF64U,
        O::MemoryInit { data_index, mem } => I::MemoryInit {
            data_index: *data_index,
            mem: *mem,
        },
        O::DataDrop { data_index } => I::DataDrop(*data_index),
        O::MemoryCopy { dst_mem, src_mem } => I::MemoryCopy {
            dst_mem: *dst_mem,
            src_mem: *src_mem,
        },
        O::MemoryFill { mem } => I::MemoryFill(*mem),
        O::TableInit { elem_index, table } => I::TableInit {
            elem_index: *elem_index,
            table: *table,
        },
        O::ElemDrop { elem_index } => I::ElemDrop(*elem_index),
        O::TableCopy {
            dst_table,
            src_table,
        } => I::TableCopy {
            dst_table: *dst_table,
            src_table: *src_table,
        },
        O::TableFill { table } => I::TableFill(*table),
        O::TableGet { table } => I::TableGet(*table),
        O::TableSet { table } => I::TableSet(*table),
        O::TableGrow { table } => I::TableGrow(*table),
        O::TableSize { table } => I::TableSize(*table),
        O::MemoryDiscard { mem } => I::MemoryDiscard(*mem),
        O::MemoryAtomicNotify { memarg } => I::MemoryAtomicNotify(translate_memarg(memarg)?),
        O::MemoryAtomicWait32 { memarg } => I::MemoryAtomicWait32(translate_memarg(memarg)?),
        O::MemoryAtomicWait64 { memarg } => I::MemoryAtomicWait64(translate_memarg(memarg)?),
        O::AtomicFence => I::AtomicFence,
        O::I32AtomicLoad { memarg } => I::I32AtomicLoad(translate_memarg(memarg)?),
        O::I64AtomicLoad { memarg } => I::I64AtomicLoad(translate_memarg(memarg)?),
        O::I32AtomicLoad8U { memarg } => I::I32AtomicLoad8U(translate_memarg(memarg)?),
        O::I32AtomicLoad16U { memarg } => I::I32AtomicLoad16U(translate_memarg(memarg)?),
        O::I64AtomicLoad8U { memarg } => I::I64AtomicLoad8U(translate_memarg(memarg)?),
        O::I64AtomicLoad16U { memarg } => I::I64AtomicLoad16U(translate_memarg(memarg)?),
        O::I64AtomicLoad32U { memarg } => I::I64AtomicLoad32U(translate_memarg(memarg)?),
        O::I32AtomicStore { memarg } => I::I32AtomicStore(translate_memarg(memarg)?),
        O::I64AtomicStore { memarg } => I::I64AtomicStore(translate_memarg(memarg)?),
        O::I32AtomicStore8 { memarg } => I::I32AtomicStore8(translate_memarg(memarg)?),
        O::I32AtomicStore16 { memarg } => I::I32AtomicStore16(translate_memarg(memarg)?),
        O::I64AtomicStore8 { memarg } => I::I64AtomicStore8(translate_memarg(memarg)?),
        O::I64AtomicStore16 { memarg } => I::I64AtomicStore16(translate_memarg(memarg)?),
        O::I64AtomicStore32 { memarg } => I::I64AtomicStore32(translate_memarg(memarg)?),
        O::I32AtomicRmwAdd { memarg } => I::I32AtomicRmwAdd(translate_memarg(memarg)?),
        O::I64AtomicRmwAdd { memarg } => I::I64AtomicRmwAdd(translate_memarg(memarg)?),
        O::I32AtomicRmw8AddU { memarg } => I::I32AtomicRmw8AddU(translate_memarg(memarg)?),
        O::I32AtomicRmw16AddU { memarg } => I::I32AtomicRmw16AddU(translate_memarg(memarg)?),
        O::I64AtomicRmw8AddU { memarg } => I::I64AtomicRmw8AddU(translate_memarg(memarg)?),
        O::I64AtomicRmw16AddU { memarg } => I::I64AtomicRmw16AddU(translate_memarg(memarg)?),
        O::I64AtomicRmw32AddU { memarg } => I::I64AtomicRmw32AddU(translate_memarg(memarg)?),
        O::I32AtomicRmwSub { memarg } => I::I32AtomicRmwSub(translate_memarg(memarg)?),
        O::I64AtomicRmwSub { memarg } => I::I64AtomicRmwSub(translate_memarg(memarg)?),
        O::I32AtomicRmw8SubU { memarg } => I::I32AtomicRmw8SubU(translate_memarg(memarg)?),
        O::I32AtomicRmw16SubU { memarg } => I::I32AtomicRmw16SubU(translate_memarg(memarg)?),
        O::I64AtomicRmw8SubU { memarg } => I::I64AtomicRmw8SubU(translate_memarg(memarg)?),
        O::I64AtomicRmw16SubU { memarg } => I::I64AtomicRmw16SubU(translate_memarg(memarg)?),
        O::I64AtomicRmw32SubU { memarg } => I::I64AtomicRmw32SubU(translate_memarg(memarg)?),
        O::I32AtomicRmwAnd { memarg } => I::I32AtomicRmwAnd(translate_memarg(memarg)?),
        O::I64AtomicRmwAnd { memarg } => I::I64AtomicRmwAnd(translate_memarg(memarg)?),
        O::I32AtomicRmw8AndU { memarg } => I::I32AtomicRmw8AndU(translate_memarg(memarg)?),
        O::I32AtomicRmw16AndU { memarg } => I::I32AtomicRmw16AndU(translate_memarg(memarg)?),
        O::I64AtomicRmw8AndU { memarg } => I::I64AtomicRmw8AndU(translate_memarg(memarg)?),
        O::I64AtomicRmw16AndU { memarg } => I::I64AtomicRmw16AndU(translate_memarg(memarg)?),
        O::I64AtomicRmw32AndU { memarg } => I::I64AtomicRmw32AndU(translate_memarg(memarg)?),
        O::I32AtomicRmwOr { memarg } => I::I32AtomicRmwOr(translate_memarg(memarg)?),
        O::I64AtomicRmwOr { memarg } => I::I64AtomicRmwOr(translate_memarg(memarg)?),
        O::I32AtomicRmw8OrU { memarg } => I::I32AtomicRmw8OrU(translate_memarg(memarg)?),
        O::I32AtomicRmw16OrU { memarg } => I::I32AtomicRmw16OrU(translate_memarg(memarg)?),
        O::I64AtomicRmw8OrU { memarg } => I::I64AtomicRmw8OrU(translate_memarg(memarg)?),
        O::I64AtomicRmw16OrU { memarg } => I::I64AtomicRmw16OrU(translate_memarg(memarg)?),
        O::I64AtomicRmw32OrU { memarg } => I::I64AtomicRmw32OrU(translate_memarg(memarg)?),
        O::I32AtomicRmwXor { memarg } => I::I32AtomicRmwXor(translate_memarg(memarg)?),
        O::I64AtomicRmwXor { memarg } => I::I64AtomicRmwXor(translate_memarg(memarg)?),
        O::I32AtomicRmw8XorU { memarg } => I::I32AtomicRmw8XorU(translate_memarg(memarg)?),
        O::I32AtomicRmw16XorU { memarg } => I::I32AtomicRmw16XorU(translate_memarg(memarg)?),
        O::I64AtomicRmw8XorU { memarg } => I::I64AtomicRmw8XorU(translate_memarg(memarg)?),
        O::I64AtomicRmw16XorU { memarg } => I::I64AtomicRmw16XorU(translate_memarg(memarg)?),
        O::I64AtomicRmw32XorU { memarg } => I::I64AtomicRmw32XorU(translate_memarg(memarg)?),
        O::I32AtomicRmwXchg { memarg } => I::I32AtomicRmwXchg(translate_memarg(memarg)?),
        O::I64AtomicRmwXchg { memarg } => I::I64AtomicRmwXchg(translate_memarg(memarg)?),
        O::I32AtomicRmw8XchgU { memarg } => I::I32AtomicRmw8XchgU(translate_memarg(memarg)?),
        O::I32AtomicRmw16XchgU { memarg } => I::I32AtomicRmw16XchgU(translate_memarg(memarg)?),
        O::I64AtomicRmw8XchgU { memarg } => I::I64AtomicRmw8XchgU(translate_memarg(memarg)?),
        O::I64AtomicRmw16XchgU { memarg } => I::I64AtomicRmw16XchgU(translate_memarg(memarg)?),
        O::I64AtomicRmw32XchgU { memarg } => I::I64AtomicRmw32XchgU(translate_memarg(memarg)?),
        O::I32AtomicRmwCmpxchg { memarg } => I::I32AtomicRmwCmpxchg(translate_memarg(memarg)?),
        O::I64AtomicRmwCmpxchg { memarg } => I::I64AtomicRmwCmpxchg(translate_memarg(memarg)?),
        O::I32AtomicRmw8CmpxchgU { memarg } => I::I32AtomicRmw8CmpxchgU(translate_memarg(memarg)?),
        O::I32AtomicRmw16CmpxchgU { memarg } => {
            I::I32AtomicRmw16CmpxchgU(translate_memarg(memarg)?)
        }
        O::I64AtomicRmw8CmpxchgU { memarg } => I::I64AtomicRmw8CmpxchgU(translate_memarg(memarg)?),
        O::I64AtomicRmw16CmpxchgU { memarg } => {
            I::I64AtomicRmw16CmpxchgU(translate_memarg(memarg)?)
        }
        O::I64AtomicRmw32CmpxchgU { memarg } => {
            I::I64AtomicRmw32CmpxchgU(translate_memarg(memarg)?)
        }
        O::GlobalAtomicGet {
            ordering,
            global_index,
        } => I::GlobalAtomicGet {
            ordering: translate_ordering(*ordering),
            global_index: *global_index,
        },
        O::GlobalAtomicSet {
            ordering,
            global_index,
        } => I::GlobalAtomicSet {
            ordering: translate_ordering(*ordering),
            global_index: *global_index,
        },
        O::GlobalAtomicRmwAdd {
            ordering,
            global_index,
        } => I::GlobalAtomicRmwAdd {
            ordering: translate_ordering(*ordering),
            global_index: *global_index,
        },
        O::GlobalAtomicRmwSub {
            ordering,
            global_index,
        } => I::GlobalAtomicRmwSub {
            ordering: translate_ordering(*ordering),
            global_index: *global_index,
        },
        O::GlobalAtomicRmwAnd {
            ordering,
            global_index,
        } => I::GlobalAtomicRmwAnd {
            ordering: translate_ordering(*ordering),
            global_index: *global_index,
        },
        O::GlobalAtomicRmwOr {
            ordering,
            global_index,
        } => I::GlobalAtomicRmwOr {
            ordering: translate_ordering(*ordering),
            global_index: *global_index,
        },
        O::GlobalAtomicRmwXor {
            ordering,
            global_index,
        } => I::GlobalAtomicRmwXor {
            ordering: translate_ordering(*ordering),
            global_index: *global_index,
        },
        O::GlobalAtomicRmwXchg {
            ordering,
            global_index,
        } => I::GlobalAtomicRmwXchg {
            ordering: translate_ordering(*ordering),
            global_index: *global_index,
        },
        O::GlobalAtomicRmwCmpxchg {
            ordering,
            global_index,
        } => I::GlobalAtomicRmwCmpxchg {
            ordering: translate_ordering(*ordering),
            global_index: *global_index,
        },
        O::TableAtomicGet {
            ordering,
            table_index,
        } => I::TableAtomicGet {
            ordering: translate_ordering(*ordering),
            table_index: *table_index,
        },
        O::TableAtomicSet {
            ordering,
            table_index,
        } => I::TableAtomicSet {
            ordering: translate_ordering(*ordering),
            table_index: *table_index,
        },
        O::TableAtomicRmwXchg {
            ordering,
            table_index,
        } => I::TableAtomicRmwXchg {
            ordering: translate_ordering(*ordering),
            table_index: *table_index,
        },
        O::TableAtomicRmwCmpxchg {
            ordering,
            table_index,
        } => I::TableAtomicRmwCmpxchg {
            ordering: translate_ordering(*ordering),
            table_index: *table_index,
        },
        O::StructAtomicGet {
            ordering,
            struct_type_index,
            field_index,
        } => I::StructAtomicGet {
            ordering: translate_ordering(*ordering),
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::StructAtomicGetS {
            ordering,
            struct_type_index,
            field_index,
        } => I::StructAtomicGetS {
            ordering: translate_ordering(*ordering),
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::StructAtomicGetU {
            ordering,
            struct_type_index,
            field_index,
        } => I::StructAtomicGetU {
            ordering: translate_ordering(*ordering),
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::StructAtomicSet {
            ordering,
            struct_type_index,
            field_index,
        } => I::StructAtomicSet {
            ordering: translate_ordering(*ordering),
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::StructAtomicRmwAdd {
            ordering,
            struct_type_index,
            field_index,
        } => I::StructAtomicRmwAdd {
            ordering: translate_ordering(*ordering),
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::StructAtomicRmwSub {
            ordering,
            struct_type_index,
            field_index,
        } => I::StructAtomicRmwSub {
            ordering: translate_ordering(*ordering),
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::StructAtomicRmwAnd {
            ordering,
            struct_type_index,
            field_index,
        } => I::StructAtomicRmwAnd {
            ordering: translate_ordering(*ordering),
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::StructAtomicRmwOr {
            ordering,
            struct_type_index,
            field_index,
        } => I::StructAtomicRmwOr {
            ordering: translate_ordering(*ordering),
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::StructAtomicRmwXor {
            ordering,
            struct_type_index,
            field_index,
        } => I::StructAtomicRmwXor {
            ordering: translate_ordering(*ordering),
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::StructAtomicRmwXchg {
            ordering,
            struct_type_index,
            field_index,
        } => I::StructAtomicRmwXchg {
            ordering: translate_ordering(*ordering),
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::StructAtomicRmwCmpxchg {
            ordering,
            struct_type_index,
            field_index,
        } => I::StructAtomicRmwCmpxchg {
            ordering: translate_ordering(*ordering),
            struct_type_index: *struct_type_index,
            field_index: *field_index,
        },
        O::ArrayAtomicGet {
            ordering,
            array_type_index,
        } => I::ArrayAtomicGet {
            ordering: translate_ordering(*ordering),
            array_type_index: *array_type_index,
        },
        O::ArrayAtomicGetS {
            ordering,
            array_type_index,
        } => I::ArrayAtomicGetS {
            ordering: translate_ordering(*ordering),
            array_type_index: *array_type_index,
        },
        O::ArrayAtomicGetU {
            ordering,
            array_type_index,
        } => I::ArrayAtomicGetU {
            ordering: translate_ordering(*ordering),
            array_type_index: *array_type_index,
        },
        O::ArrayAtomicSet {
            ordering,
            array_type_index,
        } => I::ArrayAtomicSet {
            ordering: translate_ordering(*ordering),
            array_type_index: *array_type_index,
        },
        O::ArrayAtomicRmwAdd {
            ordering,
            array_type_index,
        } => I::ArrayAtomicRmwAdd {
            ordering: translate_ordering(*ordering),
            array_type_index: *array_type_index,
        },
        O::ArrayAtomicRmwSub {
            ordering,
            array_type_index,
        } => I::ArrayAtomicRmwSub {
            ordering: translate_ordering(*ordering),
            array_type_index: *array_type_index,
        },
        O::ArrayAtomicRmwAnd {
            ordering,
            array_type_index,
        } => I::ArrayAtomicRmwAnd {
            ordering: translate_ordering(*ordering),
            array_type_index: *array_type_index,
        },
        O::ArrayAtomicRmwOr {
            ordering,
            array_type_index,
        } => I::ArrayAtomicRmwOr {
            ordering: translate_ordering(*ordering),
            array_type_index: *array_type_index,
        },
        O::ArrayAtomicRmwXor {
            ordering,
            array_type_index,
        } => I::ArrayAtomicRmwXor {
            ordering: translate_ordering(*ordering),
            array_type_index: *array_type_index,
        },
        O::ArrayAtomicRmwXchg {
            ordering,
            array_type_index,
        } => I::ArrayAtomicRmwXchg {
            ordering: translate_ordering(*ordering),
            array_type_index: *array_type_index,
        },
        O::ArrayAtomicRmwCmpxchg {
            ordering,
            array_type_index,
        } => I::ArrayAtomicRmwCmpxchg {
            ordering: translate_ordering(*ordering),
            array_type_index: *array_type_index,
        },
        O::RefI31Shared => I::RefI31Shared,
        O::V128Load { memarg } => I::V128Load(translate_memarg(memarg)?),
        O::V128Load8x8S { memarg } => I::V128Load8x8S(translate_memarg(memarg)?),
        O::V128Load8x8U { memarg } => I::V128Load8x8U(translate_memarg(memarg)?),
        O::V128Load16x4S { memarg } => I::V128Load16x4S(translate_memarg(memarg)?),
        O::V128Load16x4U { memarg } => I::V128Load16x4U(translate_memarg(memarg)?),
        O::V128Load32x2S { memarg } => I::V128Load32x2S(translate_memarg(memarg)?),
        O::V128Load32x2U { memarg } => I::V128Load32x2U(translate_memarg(memarg)?),
        O::V128Load8Splat { memarg } => I::V128Load8Splat(translate_memarg(memarg)?),
        O::V128Load16Splat { memarg } => I::V128Load16Splat(translate_memarg(memarg)?),
        O::V128Load32Splat { memarg } => I::V128Load32Splat(translate_memarg(memarg)?),
        O::V128Load64Splat { memarg } => I::V128Load64Splat(translate_memarg(memarg)?),
        O::V128Load32Zero { memarg } => I::V128Load32Zero(translate_memarg(memarg)?),
        O::V128Load64Zero { memarg } => I::V128Load64Zero(translate_memarg(memarg)?),
        O::V128Store { memarg } => I::V128Store(translate_memarg(memarg)?),
        O::V128Load8Lane { memarg, lane } => I::V128Load8Lane {
            memarg: translate_memarg(memarg)?,
            lane: *lane,
        },
        O::V128Load16Lane { memarg, lane } => I::V128Load16Lane {
            memarg: translate_memarg(memarg)?,
            lane: *lane,
        },
        O::V128Load32Lane { memarg, lane } => I::V128Load32Lane {
            memarg: translate_memarg(memarg)?,
            lane: *lane,
        },
        O::V128Load64Lane => I::V128Load64Lane,
        O::V128Store8Lane => I::V128Store8Lane,
        O::V128Store16Lane => I::V128Store16Lane,
        O::V128Store32Lane => I::V128Store32Lane,
        O::V128Store64Lane => I::V128Store64Lane,
        O::V128Const { value } => I::V128Const(value.i128()),
        O::I8x16Shuffle { lanes } => I::I8x16Shuffle(*lanes),
        O::I8x16ExtractLaneS { lane } => I::I8x16ExtractLaneS(*lane),
        O::I8x16ExtractLaneU { lane } => I::I8x16ExtractLaneU(*lane),
        O::I8x16ReplaceLane { lane } => I::I8x16ReplaceLane(*lane),
        O::I16x8ExtractLaneS { lane } => I::I16x8ExtractLaneS(*lane),
        O::I16x8ExtractLaneU { lane } => I::I16x8ExtractLaneU(*lane),
        O::I16x8ReplaceLane { lane } => I::I16x8ReplaceLane(*lane),
        O::I32x4ExtractLane { lane } => I::I32x4ExtractLane(*lane),
        O::I32x4ReplaceLane { lane } => I::I32x4ReplaceLane(*lane),
        O::I64x2ExtractLane { lane } => I::I64x2ExtractLane(*lane),
        O::I64x2ReplaceLane { lane } => I::I64x2ReplaceLane(*lane),
        O::F32x4ExtractLane { lane } => I::F32x4ExtractLane(*lane),
        O::F32x4ReplaceLane { lane } => I::F32x4ReplaceLane(*lane),
        O::F64x2ExtractLane { lane } => I::F64x2ExtractLane(*lane),
        O::F64x2ReplaceLane { lane } => I::F64x2ReplaceLane(*lane),
        O::I8x16Swizzle => I::I8x16Swizzle,
        O::I8x16Splat => I::I8x16Splat,
        O::I16x8Splat => I::I16x8Splat,
        O::I32x4Splat => I::I32x4Splat,
        O::I64x2Splat => I::I64x2Splat,
        O::F32x4Splat => I::F32x4Splat,
        O::F64x2Splat => I::F64x2Splat,
        O::I8x16Eq => I::I8x16Eq,
        O::I8x16Ne => I::I8x16Ne,
        O::I8x16LtS => I::I8x16LtS,
        O::I8x16LtU => I::I8x16LtU,
        O::I8x16GtS => I::I8x16GtS,
        O::I8x16GtU => I::I8x16GtU,
        O::I8x16LeS => I::I8x16LeS,
        O::I8x16LeU => I::I8x16LeU,
        O::I8x16GeS => I::I8x16GeS,
        O::I8x16GeU => I::I8x16GeU,
        O::I16x8Eq => I::I16x8Eq,
        O::I16x8Ne => I::I16x8Ne,
        O::I16x8LtS => I::I16x8LtS,
        O::I16x8LtU => I::I16x8LtU,
        O::I16x8GtS => I::I16x8GtS,
        O::I16x8GtU => I::I16x8GtU,
        O::I16x8LeS => I::I16x8LeS,
        O::I16x8LeU => I::I16x8LeU,
        O::I16x8GeS => I::I16x8GeS,
        O::I16x8GeU => I::I16x8GeU,
        O::I32x4Eq => I::I32x4Eq,
        O::I32x4Ne => I::I32x4Ne,
        O::I32x4LtS => I::I32x4LtS,
        O::I32x4LtU => I::I32x4LtU,
        O::I32x4GtS => I::I32x4GtS,
        O::I32x4GtU => I::I32x4GtU,
        O::I32x4LeS => I::I32x4LeS,
        O::I32x4LeU => I::I32x4LeU,
        O::I32x4GeS => I::I32x4GeS,
        O::I32x4GeU => I::I32x4GeU,
        O::I64x2Eq => I::I64x2Eq,
        O::I64x2Ne => I::I64x2Ne,
        O::I64x2LtS => I::I64x2LtS,
        O::I64x2GtS => I::I64x2GtS,
        O::I64x2LeS => I::I64x2LeS,
        O::I64x2GeS => I::I64x2GeS,
        O::F32x4Eq => I::F32x4Eq,
        O::F32x4Ne => I::F32x4Ne,
        O::F32x4Lt => I::F32x4Lt,
        O::F32x4Gt => I::F32x4Gt,
        O::F32x4Le => I::F32x4Le,
        O::F32x4Ge => I::F32x4Ge,
        O::F64x2Eq => I::F64x2Eq,
        O::F64x2Ne => I::F64x2Ne,
        O::F64x2Lt => I::F64x2Lt,
        O::F64x2Gt => I::F64x2Gt,
        O::F64x2Le => I::F64x2Le,
        O::F64x2Ge => I::F64x2Ge,
        O::V128Not => I::V128Not,
        O::V128And => I::V128And,
        O::V128AndNot => I::V128AndNot,
        O::V128Or => I::V128Or,
        O::V128Xor => I::V128Xor,
        O::V128Bitselect => I::V128Bitselect,
        O::V128AnyTrue => I::V128AnyTrue,
        O::I8x16Abs => I::I8x16Abs,
        O::I8x16Neg => I::I8x16Neg,
        O::I8x16Popcnt => I::I8x16Popcnt,
        O::I8x16AllTrue => I::I8x16AllTrue,
        O::I8x16Bitmask => I::I8x16Bitmask,
        O::I8x16NarrowI16x8S => I::I8x16NarrowI16x8S,
        O::I8x16NarrowI16x8U => I::I8x16NarrowI16x8U,
        O::I8x16Shl => I::I8x16Shl,
        O::I8x16ShrS => I::I8x16ShrS,
        O::I8x16ShrU => I::I8x16ShrU,
        O::I8x16Add => I::I8x16Add,
        O::I8x16AddSatS => I::I8x16AddSatS,
        O::I8x16AddSatU => I::I8x16AddSatU,
        O::I8x16Sub => I::I8x16Sub,
        O::I8x16SubSatS => I::I8x16SubSatS,
        O::I8x16SubSatU => I::I8x16SubSatU,
        O::I8x16MinS => I::I8x16MinS,
        O::I8x16MinU => I::I8x16MinU,
        O::I8x16MaxS => I::I8x16MaxS,
        O::I8x16MaxU => I::I8x16MaxU,
        O::I8x16AvgrU => I::I8x16AvgrU,
        O::I16x8ExtAddPairwiseI8x16S => I::I16x8ExtAddPairwiseI8x16S,
        O::I16x8ExtAddPairwiseI8x16U => I::I16x8ExtAddPairwiseI8x16U,
        O::I16x8Abs => I::I16x8Abs,
        O::I16x8Neg => I::I16x8Neg,
        O::I16x8Q15MulrSatS => I::I16x8Q15MulrSatS,
        O::I16x8AllTrue => I::I16x8AllTrue,
        O::I16x8Bitmask => I::I16x8Bitmask,
        O::I16x8NarrowI32x4S => I::I16x8NarrowI32x4S,
        O::I16x8NarrowI32x4U => I::I16x8NarrowI32x4U,
        O::I16x8ExtendLowI8x16S => I::I16x8ExtendLowI8x16S,
        O::I16x8ExtendHighI8x16S => I::I16x8ExtendHighI8x16S,
        O::I16x8ExtendLowI8x16U => I::I16x8ExtendLowI8x16U,
        O::I16x8ExtendHighI8x16U => I::I16x8ExtendHighI8x16U,
        O::I16x8Shl => I::I16x8Shl,
        O::I16x8ShrS => I::I16x8ShrS,
        O::I16x8ShrU => I::I16x8ShrU,
        O::I16x8Add => I::I16x8Add,
        O::I16x8AddSatS => I::I16x8AddSatS,
        O::I16x8AddSatU => I::I16x8AddSatU,
        O::I16x8Sub => I::I16x8Sub,
        O::I16x8SubSatS => I::I16x8SubSatS,
        O::I16x8SubSatU => I::I16x8SubSatU,
        O::I16x8Mul => I::I16x8Mul,
        O::I16x8MinS => I::I16x8MinS,
        O::I16x8MinU => I::I16x8MinU,
        O::I16x8MaxS => I::I16x8MaxS,
        O::I16x8MaxU => I::I16x8MaxU,
        O::I16x8AvgrU => I::I16x8AvgrU,
        O::I16x8ExtMulLowI8x16S => I::I16x8ExtMulLowI8x16S,
        O::I16x8ExtMulHighI8x16S => I::I16x8ExtMulHighI8x16S,
        O::I16x8ExtMulLowI8x16U => I::I16x8ExtMulLowI8x16U,
        O::I16x8ExtMulHighI8x16U => I::I16x8ExtMulHighI8x16U,
        O::I32x4ExtAddPairwiseI16x8S => I::I32x4ExtAddPairwiseI16x8S,
        O::I32x4ExtAddPairwiseI16x8U => I::I32x4ExtAddPairwiseI16x8U,
        O::I32x4Abs => I::I32x4Abs,
        O::I32x4Neg => I::I32x4Neg,
        O::I32x4AllTrue => I::I32x4AllTrue,
        O::I32x4Bitmask => I::I32x4Bitmask,
        O::I32x4ExtendLowI16x8S => I::I32x4ExtendLowI16x8S,
        O::I32x4ExtendHighI16x8S => I::I32x4ExtendHighI16x8S,
        O::I32x4ExtendLowI16x8U => I::I32x4ExtendLowI16x8U,
        O::I32x4ExtendHighI16x8U => I::I32x4ExtendHighI16x8U,
        O::I32x4Shl => I::I32x4Shl,
        O::I32x4ShrS => I::I32x4ShrS,
        O::I32x4ShrU => I::I32x4ShrU,
        O::I32x4Add => I::I32x4Add,
        O::I32x4Sub => I::I32x4Sub,
        O::I32x4Mul => I::I32x4Mul,
        O::I32x4MinS => I::I32x4MinS,
        O::I32x4MinU => I::I32x4MinU,
        O::I32x4MaxS => I::I32x4MaxS,
        O::I32x4MaxU => I::I32x4MaxU,
        O::I32x4DotI16x8S => I::I32x4DotI16x8S,
        O::I32x4ExtMulLowI16x8S => I::I32x4ExtMulLowI16x8S,
        O::I32x4ExtMulHighI16x8S => I::I32x4ExtMulHighI16x8S,
        O::I32x4ExtMulLowI16x8U => I::I32x4ExtMulLowI16x8U,
        O::I32x4ExtMulHighI16x8U => I::I32x4ExtMulHighI16x8U,
        O::I64x2Abs => I::I64x2Abs,
        O::I64x2Neg => I::I64x2Neg,
        O::I64x2AllTrue => I::I64x2AllTrue,
        O::I64x2Bitmask => I::I64x2Bitmask,
        O::I64x2ExtendLowI32x4S => I::I64x2ExtendLowI32x4S,
        O::I64x2ExtendHighI32x4S => I::I64x2ExtendHighI32x4S,
        O::I64x2ExtendLowI32x4U => I::I64x2ExtendLowI32x4U,
        O::I64x2ExtendHighI32x4U => I::I64x2ExtendHighI32x4U,
        O::I64x2Shl => I::I64x2Shl,
        O::I64x2ShrS => I::I64x2ShrS,
        O::I64x2ShrU => I::I64x2ShrU,
        O::I64x2Add => I::I64x2Add,
        O::I64x2Sub => I::I64x2Sub,
        O::I64x2Mul => I::I64x2Mul,
        O::I64x2ExtMulLowI32x4S => I::I64x2ExtMulLowI32x4S,
        O::I64x2ExtMulHighI32x4S => I::I64x2ExtMulHighI32x4S,
        O::I64x2ExtMulLowI32x4U => I::I64x2ExtMulLowI32x4U,
        O::I64x2ExtMulHighI32x4U => I::I64x2ExtMulHighI32x4U,
        O::F32x4Ceil => I::F32x4Ceil,
        O::F32x4Floor => I::F32x4Floor,
        O::F32x4Trunc => I::F32x4Trunc,
        O::F32x4Nearest => I::F32x4Nearest,
        O::F32x4Abs => I::F32x4Abs,
        O::F32x4Neg => I::F32x4Neg,
        O::F32x4Sqrt => I::F32x4Sqrt,
        O::F32x4Add => I::F32x4Add,
        O::F32x4Sub => I::F32x4Sub,
        O::F32x4Mul => I::F32x4Mul,
        O::F32x4Div => I::F32x4Div,
        O::F32x4Min => I::F32x4Min,
        O::F32x4Max => I::F32x4Max,
        O::F32x4PMin => I::F32x4PMin,
        O::F32x4PMax => I::F32x4PMax,
        O::F64x2Ceil => I::F64x2Ceil,
        O::F64x2Floor => I::F64x2Floor,
        O::F64x2Trunc => I::F64x2Trunc,
        O::F64x2Nearest => I::F64x2Nearest,
        O::F64x2Abs => I::F64x2Abs,
        O::F64x2Neg => I::F64x2Neg,
        O::F64x2Sqrt => I::F64x2Sqrt,
        O::F64x2Add => I::F64x2Add,
        O::F64x2Sub => I::F64x2Sub,
        O::F64x2Mul => I::F64x2Mul,
        O::F64x2Div => I::F64x2Div,
        O::F64x2Min => I::F64x2Min,
        O::F64x2Max => I::F64x2Max,
        O::F64x2PMin => I::F64x2PMin,
        O::F64x2PMax => I::F64x2PMax,
        O::I32x4TruncSatF32x4S => I::I32x4TruncSatF32x4S,
        O::I32x4TruncSatF32x4U => I::I32x4TruncSatF32x4U,
        O::F32x4ConvertI32x4S => I::F32x4ConvertI32x4S,
        O::F32x4ConvertI32x4U => I::F32x4ConvertI32x4U,
        O::I32x4TruncSatF64x2SZero => I::I32x4TruncSatF64x2SZero,
        O::I32x4TruncSatF64x2UZero => I::I32x4TruncSatF64x2UZero,
        O::F64x2ConvertLowI32x4S => I::F64x2ConvertLowI32x4S,
        O::F64x2ConvertLowI32x4U => I::F64x2ConvertLowI32x4U,
        O::F32x4DemoteF64x2Zero => I::F32x4DemoteF64x2Zero,
        O::F64x2PromoteLowF32x4 => I::F64x2PromoteLowF32x4,
        O::I8x16RelaxedSwizzle => I::I8x16RelaxedSwizzle,
        O::I32x4RelaxedTruncF32x4S => I::I32x4RelaxedTruncF32x4S,
        O::I32x4RelaxedTruncF32x4U => I::I32x4RelaxedTruncF32x4U,
        O::I32x4RelaxedTruncF64x2SZero => I::I32x4RelaxedTruncF64x2SZero,
        O::I32x4RelaxedTruncF64x2UZero => I::I32x4RelaxedTruncF64x2UZero,
        O::F32x4RelaxedMadd => I::F32x4RelaxedMadd,
        O::F32x4RelaxedNmadd => I::F32x4RelaxedNmadd,
        O::F64x2RelaxedMadd => I::F64x2RelaxedMadd,
        O::F64x2RelaxedNmadd => I::F64x2RelaxedNmadd,
        O::I8x16RelaxedLaneselect => I::I8x16RelaxedLaneselect,
        O::I16x8RelaxedLaneselect => I::I16x8RelaxedLaneselect,
        O::I32x4RelaxedLaneselect => I::I32x4RelaxedLaneselect,
        O::I64x2RelaxedLaneselect => I::I64x2RelaxedLaneselect,
        O::F32x4RelaxedMin => I::F32x4RelaxedMin,
        O::F32x4RelaxedMax => I::F32x4RelaxedMax,
        O::F64x2RelaxedMin => I::F64x2RelaxedMin,
        O::F64x2RelaxedMax => I::F64x2RelaxedMax,
        O::I16x8RelaxedQ15mulrS => I::I16x8RelaxedQ15mulrS,
        O::I16x8RelaxedDotI8x16I7x16S => I::I16x8RelaxedDotI8x16I7x16S,
        O::I32x4RelaxedDotI8x16I7x16AddS => I::I32x4RelaxedDotI8x16I7x16AddS,
        O::CallRef { type_index } => I::CallRef(*type_index),
        O::ReturnCallRef { type_index } => I::ReturnCallRef(*type_index),
        O::RefAsNonNull => I::RefAsNonNull,
        O::BrOnNull { relative_depth } => I::BrOnNull(*relative_depth),
        O::BrOnNonNull { relative_depth } => I::BrOnNonNull(*relative_depth),
        O::ContNew { cont_type_index } => I::ContNew(*cont_type_index),
        O::ContBind => I::ContBind,
        O::Suspend { tag_index } => I::Suspend(*tag_index),
        O::Resume => I::Resume,
        O::ResumeThrow => I::ResumeThrow,
        O::Switch => I::Switch,
        O::I64Add128 => I::I64Add128,
        O::I64Sub128 => I::I64Sub128,
        O::I64MulWideS => I::I64MulWideS,
        O::I64MulWideU => I::I64MulWideU,
    })
}

fn translate_ordering(ordering: Ordering) -> wasm_encoder::Ordering {
    match ordering {
        Ordering::AcqRel => wasm_encoder::Ordering::AcqRel,
        Ordering::SeqCst => wasm_encoder::Ordering::SeqCst,
    }
}

pub fn translate_block_type(ty: &wasmparser::BlockType) -> Result<BlockType> {
    match ty {
        wasmparser::BlockType::Empty => Ok(BlockType::Empty),
        wasmparser::BlockType::Type(ty) => Ok(BlockType::Result(translate_val_type(ty)?)),
        wasmparser::BlockType::FuncType(f) => Ok(BlockType::FunctionType(*f)),
    }
}

pub fn translate_memarg(memarg: &wasmparser::MemArg) -> Result<MemArg> {
    Ok(MemArg {
        offset: memarg.offset,
        align: memarg.align.into(),
        memory_index: memarg.memory,
    })
}

pub fn translate_data(data: wasmparser::Data<'_>, s: &mut DataSection) -> Result<()> {
    let mode = match &data.kind {
        DataKind::Active {
            memory_index,
            offset_expr,
        } => DataSegmentMode::Active {
            memory_index: *memory_index,
            offset: &translate_const_expr(offset_expr)?,
        },
        DataKind::Passive => DataSegmentMode::Passive,
    };
    s.segment(DataSegment {
        mode,
        data: data.data.iter().copied(),
    });
    Ok(())
}

pub fn translate_code(body: FunctionBody<'_>, s: &mut CodeSection) -> Result<()> {
    let locals = body
        .get_locals_reader()?
        .into_iter()
        .map(|local| {
            let (cnt, ty) = local?;
            Ok((cnt, translate_val_type(&ty)?))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut func = Function::new(locals);

    let reader = body.get_operators_reader()?;
    for op in reader {
        let op = op?;
        func.instruction(&translate_op(&op)?);
    }
    s.function(&func);
    Ok(())
}
