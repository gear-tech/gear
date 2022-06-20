// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use crate::{Config, Pallet, Weight};
use codec::{Decode, Encode};
use frame_support::{codec, pallet_prelude::*, storage_alias, traits::Get, Identity};

/// Wrapper for all migrations of this pallet, based on `StorageVersion`.
pub fn migrate<T: Config>() -> Weight {
    let version = StorageVersion::get::<Pallet<T>>();
    let mut weight: Weight = 0;

    if version < 2 {
        weight = weight.saturating_add(v2::migrate::<T>());
        StorageVersion::new(2).put::<Pallet<T>>();
    }

    weight
}

/// V2: `gear_core::Code` is changed to have an exports field.
mod v2 {
    use super::*;
    use sp_std::vec::Vec;

    #[derive(Encode, Decode)]
    struct WasmPageNumber(u32);

    #[derive(Encode, Decode, Clone, Copy)]
    struct CodeId([u8; 32]);

    #[derive(Encode, Decode)]
    struct OldCode {
        code: Vec<u8>,
        raw_code: Vec<u8>,
        static_pages: WasmPageNumber,
        #[codec(compact)]
        instruction_weights_version: u32,
    }

    #[derive(Encode, Decode)]
    struct Code {
        code: Vec<u8>,
        raw_code: Vec<u8>,
        exports: Vec<Vec<u8>>,
        static_pages: WasmPageNumber,
        #[codec(compact)]
        instruction_weights_version: u32,
    }

    #[derive(Encode, Decode)]
    struct OldInstrumentedCode {
        code: Vec<u8>,
        static_pages: WasmPageNumber,
        version: u32,
    }

    #[derive(Encode, Decode)]
    struct InstrumentedCode {
        code: Vec<u8>,
        exports: Vec<Vec<u8>>,
        static_pages: WasmPageNumber,
        version: u32,
    }

    #[storage_alias]
    type CodeStorage<T: Config> = StorageMap<Pallet<T>, Identity, CodeId, InstrumentedCode>;

    #[storage_alias]
    type OriginalCodeStorage<T: Config> = StorageMap<Pallet<T>, Identity, CodeId, Vec<u8>>;

    pub fn migrate<T: Config>() -> Weight {
        let mut weight: Weight = 0;

        <CodeStorage<T>>::translate(|key, old: OldInstrumentedCode| {
            weight = weight.saturating_add(T::DbWeight::get().reads_writes(2, 1));
            if let Some(orig_code) = <OriginalCodeStorage<T>>::get(key) {
                if let Ok(module) = wasm_instrument::parity_wasm::deserialize_buffer::<
                    wasm_instrument::parity_wasm::elements::Module,
                >(&orig_code)
                {
                    let exports = if let Some(export_section) = module.export_section() {
                        export_section
                            .entries()
                            .iter()
                            .map(|v| v.field().as_bytes().to_vec())
                            .collect()
                    } else {
                        Vec::new()
                    };

                    if exports.contains(&b"init".to_vec()) || exports.contains(&b"handle".to_vec())
                    {
                        Some(InstrumentedCode {
                            code: old.code,
                            exports,
                            static_pages: old.static_pages,
                            version: old.version,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        });

        weight
    }
}
