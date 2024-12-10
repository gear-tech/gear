// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

use super::{BuiltinActorError, Config, WeightInfo, LOG_TARGET};
use alloc::{vec, vec::Vec};
use parity_scale_codec::{Compact, Decode, Input};

pub(crate) fn decode_vec<T: Config, I: Input>(
    gas_limit: &mut u64,
    input: &mut I,
) -> Result<Vec<u8>, BuiltinActorError> {
    let Ok(len) = Compact::<u32>::decode(input).map(u32::from) else {
        log::debug!(
            target: LOG_TARGET,
            "Failed to scale-decode vector length"
        );
        return Err(BuiltinActorError::DecodingError);
    };

    let to_spend = <T as Config>::WeightInfo::decode_bytes(len).ref_time();
    if *gas_limit < to_spend {
        return Err(BuiltinActorError::InsufficientGas);
    }

    *gas_limit = gas_limit.saturating_sub(to_spend);

    let mut items = vec![0u8; len as usize];
    let bytes_slice = items.as_mut_slice();
    input.read(bytes_slice).map(|_| items).map_err(|_| {
        log::debug!(
            target: LOG_TARGET,
            "Failed to scale-decode vector data",
        );

        BuiltinActorError::DecodingError
    })
}
