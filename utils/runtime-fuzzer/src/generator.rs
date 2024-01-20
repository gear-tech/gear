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

use arbitrary::Unstructured;
use gear_core::ids::ProgramId;
use std::mem;

use crate::data::*;

// Max code size - 25 KiB.
const MAX_CODE_SIZE: usize = 25 * 1024;

/// Maximum payload size for the fuzzer - 1 KiB.
///
/// TODO: #3442
const MAX_PAYLOAD_SIZE: usize = 1024;
const _: () = assert!(MAX_PAYLOAD_SIZE <= gear_core::message::MAX_PAYLOAD_SIZE);

/// Maximum salt size for the fuzzer - 512 bytes.
///
/// There's no need in large salts as we have only 35 extrinsics
/// for one run. Also small salt will make overall size of the
/// corpus smaller.
const MAX_SALT_SIZE: usize = 512;
const _: () = assert!(MAX_SALT_SIZE <= gear_core::message::MAX_PAYLOAD_SIZE);

const ID_SIZE: usize = mem::size_of::<ProgramId>();
const GAS_AND_VALUE_SIZE: usize = mem::size_of::<(u64, u128)>();

/// Used to make sure that generators will not exceed `Unstructured` size as it's used not only
/// to generate things like wasm code or message payload but also to generate some auxiliary
/// data, for example index in some vec.
const AUXILIARY_SIZE: usize = 512;

pub(crate) struct GenerationEnvironment<'a> {
    unstructured: Unstructured<'a>,
}

impl<'a> GenerationEnvironment<'a> {
    pub(crate) fn new(data_requirement: FulfilledDataRequirement<'a, Self>) -> Self {
        Self {
            unstructured: Unstructured::new(data_requirement.data),
        }
    }
}

impl GenerationEnvironment<'_> {
    pub(crate) const fn random_data_requirement() -> usize {
        const VALUE_SIZE: usize = mem::size_of::<u128>();

        VALUE_SIZE
            * (GearCallsGenerator::UPLOAD_PROGRAM_CALLS_COUNT
                + GearCallsGenerator::SEND_MESSAGE_CALLS_COUNT)
            + AUXILIARY_SIZE
    }
}

pub(crate) struct GearCallsGenerator<'a> {
    unstructured: Unstructured<'a>,
}

impl<'a> GearCallsGenerator<'a> {
    pub(crate) fn new(data_requirement: FulfilledDataRequirement<'a, Self>) -> Self {
        Self {
            unstructured: Unstructured::new(data_requirement.data),
        }
    }
}

impl GearCallsGenerator<'_> {
    // *WARNING*:
    //
    // Increasing these constants requires resetting minimal
    // size of fuzzer input buffer in corresponding scripts.
    const UPLOAD_PROGRAM_CALLS_COUNT: usize = 10;
    const SEND_MESSAGE_CALLS_COUNT: usize = 15;

    pub(crate) const fn random_data_requirement() -> usize {
        Self::upload_program_data_requirement() * Self::UPLOAD_PROGRAM_CALLS_COUNT
            + Self::send_message_data_requirement() * Self::SEND_MESSAGE_CALLS_COUNT
    }

    const fn upload_program_data_requirement() -> usize {
        MAX_CODE_SIZE + MAX_SALT_SIZE + MAX_PAYLOAD_SIZE + GAS_AND_VALUE_SIZE + AUXILIARY_SIZE
    }

    const fn send_message_data_requirement() -> usize {
        ID_SIZE + MAX_PAYLOAD_SIZE + GAS_AND_VALUE_SIZE + AUXILIARY_SIZE
    }
}
