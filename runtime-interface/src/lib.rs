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

//! Runtime interface for gear node

#![allow(useless_deprecated, deprecated)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "std")]
pub use {
    gear_lazy_pages_host_api::{
        self as lazy_pages_detail, LazyPagesInitContext, LazyPagesProgramContext,
    },
    gear_sandbox_interface::sandbox,
    gear_sandbox_interface::{
        Instantiate, SandboxBackend, detail as sandbox_detail, init as sandbox_init,
    },
};

use gear_core::{
    gas::GasLeft,
    memory::{HostPointer, MemoryInterval},
};
use gear_lazy_pages_common::Status;
use parity_scale_codec::{Decode, Encode};
use sp_runtime_interface::runtime_interface;
use sp_std::{result::Result, vec::Vec};
#[cfg(feature = "std")]
use {
    builtins_common::bls12_381::{Bls12_381Ops, Bls12_381OpsLowLevel},
    gear_lazy_pages::LazyPagesStorage,
    gear_lazy_pages_common::ProcessAccessError,
};

const _: () = assert!(size_of::<HostPointer>() >= size_of::<usize>());

// Domain Separation Tag for signatures on G2.
pub const DST_G2: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

#[cfg(feature = "std")]
#[derive(Debug, Default)]
struct SpIoProgramStorage;

#[cfg(feature = "std")]
impl LazyPagesStorage for SpIoProgramStorage {
    fn page_exists(&self, key: &[u8]) -> bool {
        sp_io::storage::exists(key)
    }

    fn load_page(&mut self, key: &[u8], buffer: &mut [u8]) -> Option<u32> {
        sp_io::storage::read(key, buffer, 0)
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum ProcessAccessErrorVer1 {
    OutOfBounds,
    GasLimitExceeded,
    GasAllowanceExceeded,
}

/// Runtime interface for gear node and runtime.
/// Note: name is expanded as gear_ri
#[runtime_interface]
pub trait GearRI {
    #[version(2)]
    fn pre_process_memory_accesses(reads: &[u8], writes: &[u8], gas_bytes: &mut [u8; 8]) -> u8 {
        let mut gas_counter = u64::from_le_bytes(*gas_bytes);
        let res =
            gear_lazy_pages_host_api::pre_process_memory_accesses(reads, writes, &mut gas_counter);
        gas_bytes.copy_from_slice(&gas_counter.to_le_bytes());
        res
    }

    fn lazy_pages_status() -> (Status,) {
        gear_lazy_pages_host_api::lazy_pages_status()
    }

    /// Init lazy-pages.
    /// Returns whether initialization was successful.
    fn init_lazy_pages(ctx: gear_lazy_pages_host_api::LazyPagesInitContext) -> bool {
        gear_lazy_pages_host_api::init_lazy_pages(ctx, SpIoProgramStorage)
    }

    /// Init lazy pages context for current program.
    /// Panic if some goes wrong during initialization.
    fn init_lazy_pages_for_program(ctx: gear_lazy_pages_host_api::LazyPagesProgramContext) {
        gear_lazy_pages_host_api::init_lazy_pages_for_program(ctx)
    }

    /// Mprotect all wasm mem buffer except released pages.
    /// If `protect` argument is true then restrict all accesses to pages,
    /// else allows read and write accesses.
    fn mprotect_lazy_pages(protect: bool) {
        gear_lazy_pages_host_api::mprotect_lazy_pages(protect)
    }

    fn change_wasm_memory_addr_and_size(addr: Option<HostPointer>, size: Option<u32>) {
        gear_lazy_pages_host_api::change_wasm_memory_addr_and_size(addr, size)
    }

    fn write_accessed_pages() -> Vec<u32> {
        gear_lazy_pages_host_api::write_accessed_pages()
    }

    /* Below goes deprecated runtime interface functions. */
    fn pre_process_memory_accesses(
        reads: &[MemoryInterval],
        writes: &[MemoryInterval],
        gas_left: (GasLeft,),
    ) -> (GasLeft, Result<(), ProcessAccessErrorVer1>) {
        let mut gas_left = gas_left.0;
        let gas_before = gas_left.gas;
        let res = gear_lazy_pages::pre_process_memory_accesses(reads, writes, &mut gas_left.gas);

        // Support charge for allowance otherwise DB will be corrupted.
        gas_left.allowance = gas_left
            .allowance
            .saturating_sub(gas_before.saturating_sub(gas_left.gas));

        match res {
            Ok(_) => {
                if gas_left.allowance > 0 {
                    (gas_left, Ok(()))
                } else {
                    (gas_left, Err(ProcessAccessErrorVer1::GasAllowanceExceeded))
                }
            }
            Err(ProcessAccessError::OutOfBounds) => {
                (gas_left, Err(ProcessAccessErrorVer1::OutOfBounds))
            }
            Err(ProcessAccessError::GasLimitExceeded) => {
                (gas_left, Err(ProcessAccessErrorVer1::GasLimitExceeded))
            }
        }
    }
}

/// For debug using in benchmarks testing.
/// In wasm runtime is impossible to interact with OS functionality,
/// this interface allows to do it partially.
#[runtime_interface]
pub trait GearDebug {
    fn println(msg: &[u8]) {
        println!("{}", sp_std::str::from_utf8(msg).unwrap());
    }

    fn file_write(path: &str, data: Vec<u8>) {
        use std::{fs::File, io::Write};

        let mut file = File::create(path).unwrap();
        file.write_all(&data).unwrap();
    }

    fn file_read(path: &str) -> Vec<u8> {
        use std::{fs::File, io::Read};

        let mut file = File::open(path).unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        data
    }

    fn time_in_nanos() -> u128 {
        use std::time::SystemTime;

        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }
}

#[runtime_interface]
pub trait GearBls12_381 {
    /// Aggregate provided G1-points. Useful for cases with hundreds or more items.
    /// Accepts scale-encoded `ArkScale<Vec<G1Projective>>`.
    /// Result is scale-encoded `ArkScale<G1Projective>`.
    fn aggregate_g1(points: Vec<u8>) -> Result<Vec<u8>, u32> {
        Bls12_381OpsLowLevel::aggregate_g1(points).map_err(|e| e.as_u32())
    }

    /// Map a message to G2Affine-point using the domain separation tag from `milagro_bls`.
    /// Result is encoded `ArkScale<G2Affine>`.
    fn map_to_g2affine(message: Vec<u8>) -> Result<Vec<u8>, u32> {
        Bls12_381OpsLowLevel::map_to_g2affine(message).map_err(|e| e.as_u32())
    }
}
