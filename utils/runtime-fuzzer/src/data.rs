// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use crate::generator::{GearCallsGenerator, GenerationEnvironment};
use arbitrary::{Arbitrary, Error, Result, Unstructured};
use std::{any, fmt::Debug, marker::PhantomData};

/// This is a wrapper over random bytes provided from fuzzer.
///
/// It's main purpose is to be a mock implementor of `Debug`.
/// For more info see `Debug` impl.
pub struct FuzzerInput<'a>(&'a [u8]);

impl<'a> Arbitrary<'a> for FuzzerInput<'a> {
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
        let data = u.peek_bytes(u.len()).ok_or(Error::NotEnoughData)?;

        Ok(Self(data))
    }
}

/// That's done because when fuzzer finds a crash it prints a [`Debug`] string of the crashing input.
/// Fuzzer constructs from the input an array of [`GearCall`] with pretty large codes and payloads,
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

    pub(crate) fn into_data_requirements(self) -> Result<(
        FulfilledDataRequirement<'a, GenerationEnvironment<'a>>,
        FulfilledDataRequirement<'a, GearCallsGenerator<'a>>,
    )> {
        let FuzzerInput(data) = self;
        let exec_env_data_requirement = DataRequirement::<GenerationEnvironment>::new();
        let gear_calls_data_requirement = DataRequirement::<GearCallsGenerator>::new();

        let total_required_size =
            exec_env_data_requirement.min_size + gear_calls_data_requirement.min_size;
        if data.len() < total_required_size {
            log::trace!(
                "Not enough data for fuzzing, expected - {}, got - {}",
                total_required_size,
                data.len(),
            );

            return Err(Error::NotEnoughData);
        }

        let (exec_env_data, gear_calls_data) = data.split_at(exec_env_data_requirement.min_size);
        exec_env_data_requirement
            .try_fulfill(exec_env_data)
            .and_then(|eef| {
                gear_calls_data_requirement
                    .try_fulfill(gear_calls_data)
                    .map(|gcf| (eef, gcf))
            })
    }
}

pub(crate) struct DataRequirement<T> {
    pub(crate) min_size: usize,
    _phantom: PhantomData<T>,
}

// todo use macro_rules!
impl DataRequirement<GenerationEnvironment<'_>> {
    fn new() -> Self {
        Self {
            min_size: GenerationEnvironment::random_data_requirement(),
            _phantom: PhantomData,
        }
    }
}

impl DataRequirement<GearCallsGenerator<'_>> {
    fn new() -> Self {
        Self {
            min_size: GearCallsGenerator::random_data_requirement(),
            _phantom: PhantomData,
        }
    }
}

impl<T> DataRequirement<T> {
    pub(crate) fn try_fulfill<'a>(
        &self,
        data: &'a [u8],
    ) -> Result<FulfilledDataRequirement<'a, T>> {
        if data.len() < self.min_size {
            log::trace!(
                "Insufficient data for {:?}: expected - {}, got - {}.",
                any::type_name::<T>(),
                self.min_size,
                data.len()
            );

            return Err(Error::NotEnoughData);
        }

        Ok(FulfilledDataRequirement {
            data: &data[..self.min_size],
            _phantom: PhantomData,
        })
    }
}

pub(crate) struct FulfilledDataRequirement<'a, T> {
    pub(crate) data: &'a [u8],
    _phantom: PhantomData<T>,
}
