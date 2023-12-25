// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! Entities configuring syscalls params allowed values.
//!
//! Types here are used to create [`crate::SyscallsConfig`].

use crate::{SyscallDestination, DEFAULT_INITIAL_SIZE};
use arbitrary::{Result, Unstructured};
use gsys::Hash;
use std::{collections::HashMap, mem, ops::RangeInclusive};

pub use gear_wasm_instrument::syscalls::{HashType, Ptr, RegularParamType};

/// Syscalls params config.
///
/// This is basically a map, which creates a relationship between each kind of
/// param, that a syscall can have, and allowed values ("rules") for each of
/// the params.
///
/// The config manages differently memory pointer value params and other kinds
/// of params, like gas, length, offset and etc.
///
/// # Note:
///
/// By default rules for `Alloc` param *can be, but are not* defined in current
/// module. That's because this is a param of the memory-related syscall, params
/// to which must be defined based on the memory configuration of the wasm module.
/// The client knows more about the memory configuration and possibly allowed values
/// for the param.
///
/// Should be also stated that `Length` param is processed differently, if it's a length
/// of the memory array read by backend from wasm. In this case, this param value is computed
/// based on memory size of the wasm module during syscall params processing. For other cases,
/// the values for the param will regulated by rules, if they are set.
#[derive(Debug, Clone)]
pub struct SyscallsParamsConfig {
    regular: HashMap<RegularParamType, RegularParamAllowedValues>,
    pub(super) ptr: HashMap<Ptr, PtrParamAllowedValues>,
}

impl SyscallsParamsConfig {
    pub fn default_regular() -> Self {
        use RegularParamType::*;

        let free_start = DEFAULT_INITIAL_SIZE as i64;
        let free_end = free_start + 5;

        let mut this = Self::empty();

        // Setting regular params rules.
        this.set_rule(Length, (0..=1600).into());
        this.set_rule(Gas, (0..=250_000_000_000).into());
        this.set_rule(Offset, (0..=10).into());
        this.set_rule(DurationBlockNumber, (1..=8).into());
        this.set_rule(DelayBlockNumber, (0..=4).into());
        this.set_rule(Handler, (0..=100).into());
        this.set_rule(Free, (free_start..=free_end).into());
        this.set_rule(FreeUpperBound, (0..=10).into());
        this.set_rule(Version, (1..=1).into());

        this
    }

    pub fn default_ptr() -> Self {
        let mut this = Self::empty();

        let range = 0..=100_000_000_000;
        // Setting ptr params rules.
        this.set_ptr_rule(PtrParamAllowedValues::Value(range.clone()));
        this.set_ptr_rule(PtrParamAllowedValues::ActorIdWithValue {
            actor: SyscallDestination::default(),
            range: range.clone(),
        });
        this.set_ptr_rule(PtrParamAllowedValues::ActorId(SyscallDestination::default()));

        this
    }

    pub fn empty() -> Self {
        Self {
            regular: HashMap::new(),
            ptr: HashMap::new(),
        }
    }

    /// New [`SyscallsParamsConfig`] with all rules set to produce one constant value
    /// for regular (non memory ptr value) params.
    pub fn const_regular_params(value: i64) -> Self {
        use RegularParamType::*;

        let allowed_values: RegularParamAllowedValues = (value..=value).into();
        Self {
            regular: [
                Length,
                Gas,
                Offset,
                DurationBlockNumber,
                DelayBlockNumber,
                Handler,
                Free,
                FreeUpperBound,
                Version,
            ]
            .into_iter()
            .map(|param_type| (param_type, allowed_values.clone()))
            .collect(),
            ptr: HashMap::new(),
        }
    }

    /// Get allowed values for the regular syscall param.
    pub fn get_rule(&self, param: RegularParamType) -> Option<RegularParamAllowedValues> {
        self.regular.get(&param).cloned()
    }

    /// Get allowed values for the pointer syscall param.
    pub fn get_ptr_rule(&self, ptr: Ptr) -> Option<PtrParamAllowedValues> {
        self.ptr.get(&ptr).cloned()
    }

    /// Set rules for a regular syscall param.
    pub fn set_rule(&mut self, param: RegularParamType, allowed_values: RegularParamAllowedValues) {
        matches!(param, RegularParamType::Pointer(_))
            .then(|| panic!("Rules for pointers are defined in `set_ptr_rule` method."));

        self.regular.insert(param, allowed_values);
    }

    /// Set rules for memory pointer syscall param.
    pub fn set_ptr_rule(&mut self, allowed_values: PtrParamAllowedValues) {
        use Ptr::*;

        let ptr = allowed_values.clone().into();
        // TODO review prcessing and adding rules for ptrs
        let allowed_values = match ptr {
            Hash(_) | Value | HashWithValue(_) | TwoHashesWithValue(_, _) => allowed_values,
            TwoHashes(_, _) => unimplemented!(
                "Currently unsupported defining ptr param filler config for `TwoHashes`."
            ),
            BlockNumber
            | BlockTimestamp
            | SizedBufferStart { .. }
            | BufferStart
            | Gas
            | Length
            | BlockNumberWithHash(_) => panic!("Impossible to set rules for non ptr params."),
            MutBlockNumber
            | MutBlockTimestamp
            | MutSizedBufferStart { .. }
            | MutBufferStart
            | MutHash(_)
            | MutGas
            | MutLength
            | MutValue
            | MutBlockNumberWithHash(_)
            | MutHashWithValue(_)
            | MutTwoHashes(_, _)
            | MutTwoHashesWithValue(_, _) => {
                panic!("Mutable pointers values are set by executor, not by wasm itself.")
            }
        };

        self.ptr.insert(ptr, allowed_values);
    }
}

impl Default for SyscallsParamsConfig {
    fn default() -> Self {
        let SyscallsParamsConfig { regular, .. } = Self::default_regular();
        let SyscallsParamsConfig { ptr, .. } = Self::default_ptr();

        Self { regular, ptr }
    }
}

/// Range of allowed values for the syscall param.
#[derive(Debug, Clone)]
pub struct RegularParamAllowedValues(RangeInclusive<i64>);

impl RegularParamAllowedValues {
    /// Zero param value.
    ///
    /// That means that for particular param `0` will be always
    /// it's value.
    pub fn zero() -> Self {
        Self(0..=0)
    }

    /// Constant param value.
    ///
    /// That means that for particular param `value` will be always
    /// it's value.
    pub fn constant(value: i64) -> Self {
        Self(value..=value)
    }

    /// Get i32 value for the param from it's allowed range.
    pub fn get_i32(&self, unstructured: &mut Unstructured) -> Result<i32> {
        let current_range_start = *self.0.start();
        let current_range_end = *self.0.end();

        let start = if current_range_start < i32::MIN as i64 {
            i32::MIN
        } else {
            current_range_start as i32
        };
        let end = if current_range_end > i32::MAX as i64 {
            i32::MAX
        } else {
            current_range_end as i32
        };

        unstructured.int_in_range(start..=end)
    }

    /// Get i64 value for the param from it's allowed range.
    pub fn get_i64(&self, unstructured: &mut Unstructured) -> Result<i64> {
        unstructured.int_in_range(self.0.clone())
    }
}

impl From<RangeInclusive<i64>> for RegularParamAllowedValues {
    fn from(range: RangeInclusive<i64>) -> Self {
        Self(range)
    }
}

impl Default for RegularParamAllowedValues {
    fn default() -> Self {
        Self::zero()
    }
}

/// Allowed values for syscalls pointer params.
///
/// Currently it allows defining only message
/// values for.
// TODO #3591 Support hashes to be defined from config.
#[derive(Debug, Clone)]
pub enum PtrParamAllowedValues {
    Value(RangeInclusive<u128>),
    ActorIdWithValue {
        actor: SyscallDestination,
        range: RangeInclusive<u128>,
    },
    ActorId(SyscallDestination),
    // HashWithValue {
    //     ty: HashType,
    //     range: RangeInclusive<u128>,
    // },
    // TwoHashesWithValue {
    //     ty1: HashType,
    //     ty2: HashType,
    //     range: RangeInclusive<u128>,
    // },
}

impl PtrParamAllowedValues {
    pub const VALUE_WORDS: usize = mem::size_of::<u128>() / mem::size_of::<i32>();
    pub const HASH_WORDS: usize = mem::size_of::<Hash>() / mem::size_of::<i32>();
    pub const HASH_WITH_VALUE_WORDS: usize = Self::HASH_WORDS + Self::VALUE_WORDS;
    pub const TWO_HASHES_WITH_VALUE_WORDS: usize = 2 * Self::HASH_WORDS + Self::VALUE_WORDS;

    // /// Get the actual data that should be written into the memory.
    // pub fn get(&self, unstructured: &mut Unstructured) -> Result<Vec<i32>> {
    //     match self {
    //         PtrParamAllowedValues::Value(range) => Self::get_value(unstructured, range.clone()),
    //         PtrParamAllowedValues::HashWithValue { range, .. } => {
    //             let mut ret = Vec::with_capacity(Self::HASH_WITH_VALUE_WORDS);
    //             ret.extend(Self::get_default_hash());
    //             ret.extend(Self::get_value(unstructured, range.clone())?);

    //             Ok(ret)
    //         }
    //         PtrParamAllowedValues::TwoHashesWithValue { range, .. } => {
    //             let mut ret = Vec::with_capacity(Self::TWO_HASHES_WITH_VALUE_WORDS);
    //             ret.extend(Self::get_default_hash());
    //             ret.extend(Self::get_default_hash());
    //             ret.extend(Self::get_value(unstructured, range.clone())?);

    //             Ok(ret)
    //         }
    //     }
    // }

    // fn get_value(unstructured: &mut Unstructured, range: RangeInclusive<u128>) -> Result<Vec<i32>> {
    //     let value = unstructured.int_in_range(range)?;
    //     Ok(Self::get_for_instructions(value.to_le_bytes()))
    // }

    // fn get_default_hash() -> Vec<i32> {
    //     Self::get_for_instructions(Hash::default())
    // }

    pub(crate) fn get_for_instructions<const N: usize>(raw_data: [u8; N]) -> Vec<i32> {
        let data_size = mem::size_of_val(&raw_data);
        let wasm_ptr_size = mem::size_of::<i32>();

        if data_size % wasm_ptr_size != 0 {
            panic!("data size isn't multiply of wasm word size")
        }

        raw_data
            .chunks(wasm_ptr_size)
            .map(|word_bytes| {
                i32::from_le_bytes(
                    word_bytes
                        .try_into()
                        .expect("Chunks are of the exact size."),
                )
            })
            .collect()
    }
}

impl From<PtrParamAllowedValues> for Ptr {
    fn from(ptr_data: PtrParamAllowedValues) -> Self {
        match ptr_data {
            PtrParamAllowedValues::Value(_) => Ptr::Value,
            PtrParamAllowedValues::ActorIdWithValue { .. } => Ptr::HashWithValue(HashType::ActorId),
            PtrParamAllowedValues::ActorId(_) => Ptr::Hash(HashType::ActorId),
            // PtrParamAllowedValues::HashWithValue { ty, .. } => Ptr::HashWithValue(ty),
            // PtrParamAllowedValues::TwoHashesWithValue { ty1, ty2, .. } => {
            //     Ptr::TwoHashesWithValue(ty1, ty2)
            // }
        }
    }
}
