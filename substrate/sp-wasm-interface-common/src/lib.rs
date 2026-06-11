// This file is part of Substrate.

// Copyright (C) 2019-2022 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Types and traits for interfacing between the host and the wasm runtime.

#![cfg_attr(not(feature = "std"), no_std)]

use core::{marker::PhantomData, mem};
use sp_std::borrow::Cow;

pub mod util;

#[cfg(feature = "wasmi")]
pub use wasmi;

#[cfg(feature = "wasmi")]
pub mod wasmi_impl;

/// Value types supported by Substrate on the boundary between host/Wasm.
#[derive(Copy, Clone, PartialEq, Debug, Eq)]
pub enum ValueType {
    /// An `i32` value type.
    I32,
    /// An `i64` value type.
    I64,
    /// An `f32` value type.
    F32,
    /// An `f64` value type.
    F64,
}

impl From<ValueType> for u8 {
    fn from(val: ValueType) -> u8 {
        match val {
            ValueType::I32 => 0,
            ValueType::I64 => 1,
            ValueType::F32 => 2,
            ValueType::F64 => 3,
        }
    }
}

impl TryFrom<u8> for ValueType {
    type Error = ();

    fn try_from(val: u8) -> core::result::Result<ValueType, ()> {
        match val {
            0 => Ok(Self::I32),
            1 => Ok(Self::I64),
            2 => Ok(Self::F32),
            3 => Ok(Self::F64),
            _ => Err(()),
        }
    }
}

/// Values supported by Substrate on the boundary between host/Wasm.
#[derive(PartialEq, Debug, Clone, Copy, codec::Encode, codec::Decode)]
pub enum Value {
    /// A 32-bit integer.
    I32(i32),
    /// A 64-bit integer.
    I64(i64),
    /// A 32-bit floating-point number stored as raw bit pattern.
    ///
    /// You can materialize this value using `f32::from_bits`.
    F32(u32),
    /// A 64-bit floating-point number stored as raw bit pattern.
    ///
    /// You can materialize this value using `f64::from_bits`.
    F64(u64),
}

impl Value {
    /// Returns the type of this value.
    pub fn value_type(&self) -> ValueType {
        match self {
            Value::I32(_) => ValueType::I32,
            Value::I64(_) => ValueType::I64,
            Value::F32(_) => ValueType::F32,
            Value::F64(_) => ValueType::F64,
        }
    }

    /// Return `Self` as `i32`.
    pub fn as_i32(&self) -> Option<i32> {
        match self {
            Self::I32(val) => Some(*val),
            _ => None,
        }
    }
}

/// Something that can be converted into a wasm compatible `Value`.
pub trait IntoValue {
    /// The type of the value in wasm.
    const VALUE_TYPE: ValueType;

    /// Convert `self` into a wasm `Value`.
    fn into_value(self) -> Value;
}

/// Something that can be created from a wasm `Value`.
pub trait TryFromValue: Sized {
    /// Try to convert the given `Value` into `Self`.
    fn try_from_value(val: Value) -> Option<Self>;
}

macro_rules! impl_into_and_from_value {
	(
		$(
			$type:ty, $( < $gen:ident >, )? $value_variant:ident,
		)*
	) => {
		$(
			impl $( <$gen> )? IntoValue for $type {
				const VALUE_TYPE: ValueType = ValueType::$value_variant;
				fn into_value(self) -> Value { Value::$value_variant(self as _) }
			}

			impl $( <$gen> )? TryFromValue for $type {
				fn try_from_value(val: Value) -> Option<Self> {
					match val {
						Value::$value_variant(val) => Some(val as _),
						_ => None,
					}
				}
			}
		)*
	}
}

impl_into_and_from_value! {
    u8, I32,
    u16, I32,
    u32, I32,
    u64, I64,
    i8, I32,
    i16, I32,
    i32, I32,
    i64, I64,
}

/// Provides `Sealed` trait to prevent implementing trait `PointerType` and `WasmTy` outside of this
/// crate.
mod private {
    pub trait Sealed {}

    impl Sealed for u8 {}
    impl Sealed for u16 {}
    impl Sealed for u32 {}
    impl Sealed for u64 {}

    impl Sealed for i32 {}
    impl Sealed for i64 {}
}

/// Something that can be wrapped in a wasm `Pointer`.
///
/// This trait is sealed.
pub trait PointerType: Sized + private::Sealed {
    /// The size of the type in wasm.
    const SIZE: u32 = mem::size_of::<Self>() as u32;
}

impl PointerType for u8 {}
impl PointerType for u16 {}
impl PointerType for u32 {}
impl PointerType for u64 {}

/// Type to represent a pointer in wasm at the host.
#[derive(Debug, PartialEq, Eq)]
pub struct Pointer<T: PointerType> {
    ptr: u32,
    _marker: PhantomData<T>,
}

impl<T: PointerType> Copy for Pointer<T> {}

impl<T: PointerType> Clone for Pointer<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: PointerType> Pointer<T> {
    /// Create a new instance of `Self`.
    pub fn new(ptr: u32) -> Self {
        Self {
            ptr,
            _marker: Default::default(),
        }
    }

    /// Calculate the offset from this pointer.
    ///
    /// `offset` is in units of `T`. So, `3` means `3 * mem::size_of::<T>()` as offset to the
    /// pointer.
    ///
    /// Returns an `Option` to respect that the pointer could probably overflow.
    pub fn offset(self, offset: u32) -> Option<Self> {
        offset
            .checked_mul(T::SIZE)
            .and_then(|o| self.ptr.checked_add(o))
            .map(|ptr| Self {
                ptr,
                _marker: Default::default(),
            })
    }

    /// Create a null pointer.
    pub fn null() -> Self {
        Self::new(0)
    }

    /// Cast this pointer of type `T` to a pointer of type `R`.
    pub fn cast<R: PointerType>(self) -> Pointer<R> {
        Pointer::new(self.ptr)
    }
}

impl<T: PointerType> From<u32> for Pointer<T> {
    fn from(ptr: u32) -> Self {
        Pointer::new(ptr)
    }
}

impl<T: PointerType> From<Pointer<T>> for u32 {
    fn from(ptr: Pointer<T>) -> Self {
        ptr.ptr
    }
}

impl<T: PointerType> From<Pointer<T>> for u64 {
    fn from(ptr: Pointer<T>) -> Self {
        u64::from(ptr.ptr)
    }
}

impl<T: PointerType> From<Pointer<T>> for usize {
    fn from(ptr: Pointer<T>) -> Self {
        ptr.ptr as _
    }
}

impl<T: PointerType> IntoValue for Pointer<T> {
    const VALUE_TYPE: ValueType = ValueType::I32;
    fn into_value(self) -> Value {
        Value::I32(self.ptr as _)
    }
}

impl<T: PointerType> TryFromValue for Pointer<T> {
    fn try_from_value(val: Value) -> Option<Self> {
        match val {
            Value::I32(val) => Some(Self::new(val as _)),
            _ => None,
        }
    }
}

/// The signature of a function.
#[derive(Eq, PartialEq, Debug, Clone)]
pub struct Signature {
    /// The arguments of a function.
    pub args: Cow<'static, [ValueType]>,
    /// The optional return value of a function.
    pub return_value: Option<ValueType>,
}

impl Signature {
    /// Create a new instance of `Signature`.
    pub fn new<T: Into<Cow<'static, [ValueType]>>>(
        args: T,
        return_value: Option<ValueType>,
    ) -> Self {
        Self {
            args: args.into(),
            return_value,
        }
    }

    /// Create a new instance of `Signature` with the given `args` and without any return value.
    pub fn new_with_args<T: Into<Cow<'static, [ValueType]>>>(args: T) -> Self {
        Self {
            args: args.into(),
            return_value: None,
        }
    }
}

/// The word size used in wasm. Normally known as `usize` in Rust.
pub type WordSize = u32;

/// Sandbox memory identifier.
pub type MemoryId = u32;

/// Host pointer sized for both 32-bit and 64-bit architectures.
pub type HostPointer = u64;

/// Typed value that can be returned from a function.
///
/// Basically a `TypedValue` plus `Unit`, for functions which return nothing.
#[derive(Clone, Copy, PartialEq, codec::Encode, codec::Decode, Debug)]
pub enum ReturnValue {
    /// For returning nothing.
    Unit,
    /// For returning some concrete value.
    Value(Value),
}

impl From<Value> for ReturnValue {
    fn from(v: Value) -> ReturnValue {
        ReturnValue::Value(v)
    }
}

impl ReturnValue {
    /// Maximum number of bytes `ReturnValue` might occupy when serialized with `SCALE`.
    ///
    /// Breakdown:
    ///  1 byte for encoding unit/value variant
    ///  1 byte for encoding value type
    ///  8 bytes for encoding the biggest value types available in wasm: f64, i64.
    pub const ENCODED_MAX_SIZE: usize = 10;
}

#[cfg(test)]
mod tests {
    use super::*;
    use codec::Encode;

    #[test]
    fn pointer_offset_works() {
        let ptr = Pointer::<u32>::null();

        assert_eq!(ptr.offset(10).unwrap(), Pointer::new(40));
        assert_eq!(ptr.offset(32).unwrap(), Pointer::new(128));

        let ptr = Pointer::<u64>::null();

        assert_eq!(ptr.offset(10).unwrap(), Pointer::new(80));
        assert_eq!(ptr.offset(32).unwrap(), Pointer::new(256));
    }

    #[test]
    fn return_value_encoded_max_size() {
        let encoded = ReturnValue::Value(Value::I64(-1)).encode();
        assert_eq!(encoded.len(), ReturnValue::ENCODED_MAX_SIZE);
    }
}
