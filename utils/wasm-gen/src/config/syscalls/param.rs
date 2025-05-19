// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::DEFAULT_INITIAL_SIZE;
use arbitrary::{Result, Unstructured};
use gear_core::primitives::CodeId;
use gear_utils::NonEmpty;
use gsys::Hash;
use std::{collections::HashMap, ops::RangeInclusive};

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
    pub fn new() -> Self {
        Self {
            regular: HashMap::new(),
            ptr: HashMap::new(),
        }
    }

    pub fn with_default_regular_config(self) -> Self {
        use RegularParamType::*;

        let free_start = DEFAULT_INITIAL_SIZE as i64;
        let free_end = free_start + 5;

        // Setting regular params rules.
        self.with_rule(Length, (0..=1600).into())
            .with_rule(Gas, (0..=10_000_000_000).into())
            .with_rule(Offset, (0..=10).into())
            .with_rule(DurationBlockNumber, (1..=8).into())
            .with_rule(DelayBlockNumber, (0..=4).into())
            .with_rule(Handler, (0..=100).into())
            .with_rule(Free, (free_start..=free_end).into())
            .with_rule(FreeUpperBound, (0..=10).into())
            .with_rule(Version, (1..=1).into())
    }

    pub fn with_default_ptr_config(self) -> Self {
        let range = 0..=100_000_000_000;
        // Setting ptr params rules.
        self.with_ptr_rule(PtrParamAllowedValues::Value(range.clone()))
            .with_ptr_rule(PtrParamAllowedValues::ActorIdWithValue {
                actor_kind: ActorKind::default(),
                range: range.clone(),
            })
            .with_ptr_rule(PtrParamAllowedValues::ActorId(ActorKind::default()))
            .with_ptr_rule(PtrParamAllowedValues::ReservationIdWithValue(range.clone()))
            .with_ptr_rule(PtrParamAllowedValues::ReservationIdWithActorIdAndValue {
                actor_kind: ActorKind::default(),
                range,
            })
            .with_ptr_rule(PtrParamAllowedValues::ReservationId)
            .with_ptr_rule(PtrParamAllowedValues::WaitedMessageId)
    }

    /// Set rules for a regular syscall param.
    pub fn with_rule(
        mut self,
        param: RegularParamType,
        allowed_values: RegularParamAllowedValues,
    ) -> Self {
        matches!(param, RegularParamType::Pointer(_))
            .then(|| panic!("Rules for pointers are defined in `set_ptr_rule` method."));

        self.regular.insert(param, allowed_values);

        self
    }

    /// Set rules for memory pointer syscall param.
    pub fn with_ptr_rule(mut self, allowed_values: PtrParamAllowedValues) -> Self {
        let ptr = match allowed_values {
            PtrParamAllowedValues::Value(_) => Ptr::Value,
            PtrParamAllowedValues::ActorIdWithValue { .. } => Ptr::HashWithValue(HashType::ActorId),
            PtrParamAllowedValues::ActorId(_) => Ptr::Hash(HashType::ActorId),
            PtrParamAllowedValues::ReservationIdWithValue(_) => {
                Ptr::HashWithValue(HashType::ReservationId)
            }
            PtrParamAllowedValues::ReservationIdWithActorIdAndValue { .. } => {
                Ptr::TwoHashesWithValue(HashType::ReservationId, HashType::ActorId)
            }
            PtrParamAllowedValues::ReservationId => Ptr::Hash(HashType::ReservationId),
            PtrParamAllowedValues::CodeIdsWithValue { .. } => Ptr::HashWithValue(HashType::CodeId),
            PtrParamAllowedValues::WaitedMessageId => Ptr::Hash(HashType::MessageId),
        };

        self.ptr.insert(ptr, allowed_values);

        self
    }

    /// Get allowed values for the regular syscall param.
    pub fn get_rule(&self, param: RegularParamType) -> Option<RegularParamAllowedValues> {
        self.regular.get(&param).cloned()
    }

    /// Get allowed values for the pointer syscall param.
    pub fn get_ptr_rule(&self, ptr: Ptr) -> Option<PtrParamAllowedValues> {
        self.ptr.get(&ptr).cloned()
    }
}

impl SyscallsParamsConfig {
    /// New [`SyscallsParamsConfig`] with all rules set to produce one constant value
    /// for regular (non memory ptr value) params.
    #[cfg(test)]
    pub(crate) fn const_regular_params(value: i64) -> Self {
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
}

impl Default for SyscallsParamsConfig {
    fn default() -> Self {
        Self::new()
            .with_default_regular_config()
            .with_default_ptr_config()
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
/// Currently it allows defining only actor kinds (`SyscallDestination`)
/// and message values for syscalls that send messages to actors.
// TODO #3591 Support other hash types.
#[derive(Debug, Clone)]
pub enum PtrParamAllowedValues {
    /// Possible range of values for `Ptr::Value` pointer type. This pointer
    /// type is usually define as a message value param type for "reply" syscalls
    /// kind.
    Value(RangeInclusive<u128>),
    /// Variant of `Ptr::HashWithValue` pointer type, where hash is actor id.
    ActorIdWithValue {
        actor_kind: ActorKind,
        range: RangeInclusive<u128>,
    },
    /// Variant of `Ptr::Hash` pointer type, where hash is actor id.
    ActorId(ActorKind),
    /// Variant of `Ptr::HashWithValue` pointer type, where hash is reservation id.
    ReservationIdWithValue(RangeInclusive<u128>),
    /// Variant of `Ptr::TwoHashesWithValue` pointer type, where hashes are
    /// reservation id and actor id.
    ReservationIdWithActorIdAndValue {
        actor_kind: ActorKind,
        range: RangeInclusive<u128>,
    },
    /// Variant of `Ptr::Hash` pointer type, where hash is reservation id.
    ReservationId,
    /// Variant of `Ptr::Hash` pointer type, where hash is code id.
    CodeIdsWithValue {
        code_ids: NonEmpty<CodeId>,
        range: RangeInclusive<u128>,
    },
    /// Variant of `Ptr::Hash` pointer type, where hash is waited message id.
    WaitedMessageId,
}

/// Actor kind, which is actually a syscall destination choice.
///
/// `gr_send*`, `gr_exit` and other message sending syscalls generated
/// from this crate can send messages to different destination
/// in accordance to the config. It's either to the message source,
/// to some existing known address, or to some random, most probably
/// non-existing, address.
#[derive(Debug, Clone, Default)]
pub enum ActorKind {
    /// The source of the incoming message will be used as
    /// a destination for an outgoing message.
    Source,
    /// Some random address from the collection of existing
    /// addresses will be used as a destination for an outgoing
    /// message.
    ExistingAddresses(NonEmpty<Hash>),
    /// Absolutely random address will be generated for
    /// an outgoing message destination.
    #[default]
    Random,
}

impl ActorKind {
    /// Check whether syscall destination is a result of `gr_source`.
    pub fn is_source(&self) -> bool {
        matches!(&self, ActorKind::Source)
    }

    /// Check whether syscall destination is defined randomly.
    pub fn is_random(&self) -> bool {
        matches!(&self, ActorKind::Random)
    }

    /// Check whether syscall destination is defined from a collection of existing addresses.
    pub fn is_existing_addresses(&self) -> bool {
        matches!(&self, ActorKind::ExistingAddresses(_))
    }
}
