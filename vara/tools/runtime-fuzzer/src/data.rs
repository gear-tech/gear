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

use crate::{generator::GearCallsGenerator, runtime::BalanceManager};
use gear_wasm_gen::wasm_gen_arbitrary::{Arbitrary, Error, Result, Unstructured};
use std::{any, fmt::Debug, marker::PhantomData};

/// This is a wrapper over random bytes provided from fuzzer.
///
/// It's main purpose is to be a mock implementor of `Debug`.
/// For more info see `Debug` impl.
pub struct FuzzerInput<'a>(&'a [u8]);

#[cfg(test)]
impl<'a> FuzzerInput<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self(data)
    }
}

impl<'a> Arbitrary<'a> for FuzzerInput<'a> {
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
        let data = u.peek_bytes(u.len()).ok_or(Error::NotEnoughData)?;

        Ok(Self(data))
    }
}

/// That's done because when fuzzer finds a crash it prints a [`Debug`] string of the crashing input.
/// Fuzzer constructs from the input an array of [`GearCall`](gear_call_gen::GearCall) with pretty large codes and payloads,
/// therefore to avoid printing huge amount of data we do a mock implementation of [`Debug`].
impl Debug for FuzzerInput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RuntimeFuzzerInput")
            .field(&"Mock `Debug` impl")
            .finish()
    }
}

impl<'a> FuzzerInput<'a> {
    pub(crate) fn inner(&self) -> &'a [u8] {
        self.0
    }

    pub(crate) fn into_data_requirements(
        self,
    ) -> Result<(
        FulfilledDataRequirement<'a, BalanceManager<'a>>,
        FulfilledDataRequirement<'a, GearCallsGenerator<'a>>,
    )> {
        let FuzzerInput(data) = self;
        let balance_manager_data_requirement = DataRequirement::<BalanceManager>::new();
        let gear_calls_data_requirement = DataRequirement::<GearCallsGenerator>::new();

        let total_data_required =
            balance_manager_data_requirement.size + gear_calls_data_requirement.size;
        if data.len() < total_data_required {
            log::trace!(
                "Not enough data for fuzzing, expected - {}, got - {}",
                total_data_required,
                data.len(),
            );

            return Err(Error::NotEnoughData);
        }

        let (balance_manager_data, gear_calls_data) =
            data.split_at(balance_manager_data_requirement.size);
        balance_manager_data_requirement
            .try_fulfill(balance_manager_data)
            .and_then(|eef| {
                gear_calls_data_requirement
                    .try_fulfill(gear_calls_data)
                    .map(|gcf| (eef, gcf))
            })
    }
}

pub(crate) struct DataRequirement<T> {
    pub(crate) size: usize,
    _phantom: PhantomData<T>,
}

impl DataRequirement<BalanceManager<'_>> {
    fn new() -> Self {
        Self {
            size: BalanceManager::random_data_requirement(),
            _phantom: PhantomData,
        }
    }
}

impl DataRequirement<GearCallsGenerator<'_>> {
    fn new() -> Self {
        Self {
            // Take 90% from required, because required is counted with max
            // possible payload and salt sizes.
            size: GearCallsGenerator::random_data_requirement() * 90 / 100,
            _phantom: PhantomData,
        }
    }
}

impl<T> DataRequirement<T> {
    pub(crate) fn try_fulfill<'a>(
        &self,
        data: &'a [u8],
    ) -> Result<FulfilledDataRequirement<'a, T>> {
        if data.len() < self.size {
            log::trace!(
                "Insufficient data for {:?}: expected - {}, got - {}.",
                any::type_name::<T>(),
                self.size,
                data.len()
            );

            return Err(Error::NotEnoughData);
        }

        Ok(FulfilledDataRequirement {
            data: &data[..self.size],
            _phantom: PhantomData,
        })
    }
}

pub(crate) struct FulfilledDataRequirement<'a, T> {
    pub(crate) data: &'a [u8],
    _phantom: PhantomData<T>,
}
