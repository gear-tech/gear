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

use gear_core::{
    gas::GasLeft,
    memory::{HostPointer, MemoryInterval},
    str::LimitedStr,
};
use gear_lazy_pages_common::{GlobalsAccessConfig, Status};
use parity_scale_codec::{Decode, Encode};
use sp_runtime_interface::{
    pass_by::{Codec, PassBy},
    runtime_interface,
};
use sp_std::{result::Result, vec::Vec};
#[cfg(feature = "std")]
use {
    ark_bls12_381::{G1Projective as G1, G2Affine, G2Projective as G2},
    ark_ec::{
        bls12::Bls12Config,
        hashing::{HashToCurve, curve_maps::wb, map_to_curve_hasher::MapToCurveBasedHasher},
    },
    ark_ff::fields::field_hashers::DefaultFieldHasher,
    ark_scale::ArkScale,
    builtins_common::bls12_381::Bls12_381Ops,
    gear_lazy_pages::LazyPagesStorage,
    gear_lazy_pages_common::ProcessAccessError,
    sp_std::convert::TryFrom,
};

pub use gear_sandbox_interface::sandbox;
#[cfg(feature = "std")]
pub use gear_sandbox_interface::{
    Instantiate, SandboxBackend, detail as sandbox_detail, init as sandbox_init,
};

const _: () = assert!(size_of::<HostPointer>() >= size_of::<usize>());

// Domain Separation Tag for signatures on G2.
pub const DST_G2: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

#[derive(Debug, Clone, Encode, Decode)]
pub struct LazyPagesProgramContext {
    /// Wasm program memory addr.
    pub wasm_mem_addr: Option<HostPointer>,
    /// Wasm program memory size.
    pub wasm_mem_size: u32,
    /// Wasm program stack end page.
    pub stack_end: Option<u32>,
    /// The field contains prefix to a program's memory pages, i.e.
    /// `program_id` + `memory_infix`.
    pub program_key: Vec<u8>,
    /// Globals config to access globals inside lazy-pages.
    pub globals_config: GlobalsAccessConfig,
    /// Lazy-pages access costs.
    pub costs: Vec<u64>,
}

impl PassBy for LazyPagesProgramContext {
    type PassBy = Codec<LazyPagesProgramContext>;
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct LazyPagesInitContext {
    pub page_sizes: Vec<u32>,
    pub global_names: Vec<LimitedStr<'static>>,
    pub pages_storage_prefix: Vec<u8>,
}

impl From<gear_lazy_pages_common::LazyPagesInitContext> for LazyPagesInitContext {
    fn from(ctx: gear_lazy_pages_common::LazyPagesInitContext) -> Self {
        let gear_lazy_pages_common::LazyPagesInitContext {
            page_sizes,
            global_names,
            pages_storage_prefix,
        } = ctx;

        Self {
            page_sizes,
            global_names,
            pages_storage_prefix,
        }
    }
}

impl From<LazyPagesInitContext> for gear_lazy_pages_common::LazyPagesInitContext {
    fn from(ctx: LazyPagesInitContext) -> Self {
        let LazyPagesInitContext {
            page_sizes,
            global_names,
            pages_storage_prefix,
        } = ctx;

        Self {
            page_sizes,
            global_names,
            pages_storage_prefix,
        }
    }
}

impl PassBy for LazyPagesInitContext {
    type PassBy = Codec<LazyPagesInitContext>;
}

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
        let res = lazy_pages_detail::pre_process_memory_accesses(reads, writes, &mut gas_counter);
        gas_bytes.copy_from_slice(&gas_counter.to_le_bytes());
        res
    }

    fn lazy_pages_status() -> (Status,) {
        lazy_pages_detail::lazy_pages_status()
    }

    /// Init lazy-pages.
    /// Returns whether initialization was successful.
    fn init_lazy_pages(ctx: LazyPagesInitContext) -> bool {
        lazy_pages_detail::init_lazy_pages(ctx)
    }

    /// Init lazy pages context for current program.
    /// Panic if some goes wrong during initialization.
    fn init_lazy_pages_for_program(ctx: LazyPagesProgramContext) {
        lazy_pages_detail::init_lazy_pages_for_program(ctx)
    }

    /// Mprotect all wasm mem buffer except released pages.
    /// If `protect` argument is true then restrict all accesses to pages,
    /// else allows read and write accesses.
    fn mprotect_lazy_pages(protect: bool) {
        lazy_pages_detail::mprotect_lazy_pages(protect)
    }

    fn change_wasm_memory_addr_and_size(addr: Option<HostPointer>, size: Option<u32>) {
        lazy_pages_detail::change_wasm_memory_addr_and_size(addr, size)
    }

    fn write_accessed_pages() -> Vec<u32> {
        lazy_pages_detail::write_accessed_pages()
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

#[cfg(feature = "std")]
pub mod lazy_pages_detail {
    use super::*;

    pub fn pre_process_memory_accesses(reads: &[u8], writes: &[u8], gas_counter: &mut u64) -> u8 {
        let mem_interval_size = size_of::<MemoryInterval>();
        let reads_len = reads.len();
        let writes_len = writes.len();

        let mut reads_intervals = Vec::with_capacity(reads_len / mem_interval_size);
        deserialize_mem_intervals(reads, &mut reads_intervals);
        let mut writes_intervals = Vec::with_capacity(writes_len / mem_interval_size);
        deserialize_mem_intervals(writes, &mut writes_intervals);

        gear_lazy_pages::pre_process_memory_accesses(
            &reads_intervals,
            &writes_intervals,
            gas_counter,
        )
        .map(|_| 0)
        .unwrap_or_else(|err| err.into())
    }

    pub fn lazy_pages_status() -> (Status,) {
        (gear_lazy_pages::status()
            .unwrap_or_else(|err| unreachable!("Cannot get lazy-pages status: {err}")),)
    }

    /// Init lazy-pages.
    /// Returns whether initialization was successful.
    pub fn init_lazy_pages(ctx: LazyPagesInitContext) -> bool {
        use gear_lazy_pages::LazyPagesVersion;

        gear_lazy_pages::init(LazyPagesVersion::Version1, ctx.into(), SpIoProgramStorage)
            .map_err(|err| log::error!("Cannot initialize lazy-pages: {err}"))
            .is_ok()
    }

    /// Init lazy pages context for current program.
    /// Panic if some goes wrong during initialization.
    pub fn init_lazy_pages_for_program(ctx: LazyPagesProgramContext) {
        let wasm_mem_addr = ctx.wasm_mem_addr.map(|addr| {
            usize::try_from(addr)
                .unwrap_or_else(|err| unreachable!("Cannot cast wasm mem addr to `usize`: {}", err))
        });

        gear_lazy_pages::initialize_for_program(
            wasm_mem_addr,
            ctx.wasm_mem_size,
            ctx.stack_end,
            ctx.program_key,
            Some(ctx.globals_config),
            ctx.costs,
        )
        .map_err(|e| e.to_string())
        .expect("Cannot initialize lazy pages for current program");
    }

    /// Mprotect all wasm mem buffer except released pages.
    /// If `protect` argument is true then restrict all accesses to pages,
    /// else allows read and write accesses.
    pub fn mprotect_lazy_pages(protect: bool) {
        if protect {
            gear_lazy_pages::set_lazy_pages_protection()
        } else {
            gear_lazy_pages::unset_lazy_pages_protection()
        }
        .map_err(|err| err.to_string())
        .expect("Cannot set/unset mprotection for lazy pages");
    }

    pub fn change_wasm_memory_addr_and_size(addr: Option<HostPointer>, size: Option<u32>) {
        // `as usize` is safe, because of const assert above.
        gear_lazy_pages::change_wasm_mem_addr_and_size(addr.map(|addr| addr as usize), size)
            .unwrap_or_else(|err| unreachable!("Cannot set new wasm addr and size: {err}"));
    }

    pub fn write_accessed_pages() -> Vec<u32> {
        gear_lazy_pages::write_accessed_pages()
            .unwrap_or_else(|err| unreachable!("Cannot get write accessed pages: {err}"))
    }

    fn deserialize_mem_intervals(bytes: &[u8], intervals: &mut Vec<MemoryInterval>) {
        let mem_interval_size = size_of::<MemoryInterval>();
        for chunk in bytes.chunks_exact(mem_interval_size) {
            // can't panic because of chunks_exact
            intervals.push(MemoryInterval::try_from_bytes(chunk).unwrap());
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
    /// Computes the multi Miller loop for BLS12-381 pairing operations.
    ///
    /// The Miller loop is the first phase of pairing computation in bilinear cryptography.
    /// This function performs Miller loops for multiple point pairs simultaneously,
    /// computing ∏ᵢ f(Pᵢ, Qᵢ) where f is the Miller function for the BLS12-381 curve.
    ///
    /// # Parameters
    ///
    /// * `g1` - SCALE-encoded `ArkScale<Vec<G1Affine>>` containing G1 points
    /// * `g2` - SCALE-encoded `ArkScale<Vec<G2Affine>>` containing G2 points
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<u8>)` - SCALE-encoded `ArkScale<Fq12>` Miller loop result
    /// * `Err(u32)` - Error code if decoding fails or invalid input
    ///
    /// # Requirements
    ///
    /// - Both arrays must have equal length and non-zero size
    /// - Points must be valid curve points in their respective groups
    /// - For complete pairing, follow with [`final_exponentiation`]
    fn multi_miller_loop(g1: Vec<u8>, g2: Vec<u8>) -> Result<Vec<u8>, u32> {
        Bls12_381Ops::multi_miller_loop(g1, g2).map_err(|e| e.as_u32())
    }

    /// Performs the final exponentiation step of BLS12-381 pairing computation.
    ///
    /// The final exponentiation is the second and final phase of pairing computation,
    /// applied to the result of the Miller loop. It computes f^((q^12 - 1) / r) where:
    /// - f is the Miller loop result (an element of Fq12)
    /// - q is the base field prime of BLS12-381
    /// - r is the prime order of the G1/G2 groups
    ///
    /// # Parameters
    ///
    /// * `f` - SCALE-encoded `ArkScale<Fq12>` Miller loop result
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<u8>)` - SCALE-encoded `ArkScale<Fq12>` final pairing result in GT
    /// * `Err(u32)` - u32 error code of `BuiltinActorError`.
    fn final_exponentiation(f: Vec<u8>) -> Result<Vec<u8>, u32> {
        Bls12_381Ops::final_exponentiation(f).map_err(|e| e.as_u32())
    }

    /// Aggregate provided G1-points. Useful for cases with hundreds or more items.
    /// Accepts scale-encoded `ArkScale<Vec<G1Projective>>`.
    /// Result is scale-encoded `ArkScale<G1Projective>`.
    fn aggregate_g1(points: &[u8]) -> Result<Vec<u8>, u32> {
        // type ArkScale<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;

        // let ArkScale(points) = ArkScale::<Vec<G1>>::decode(&mut &points[..])
        //     .map_err(|_| u32::from(GearBls12_381Error::Decode))?;

        // let point_first = points
        //     .first()
        //     .ok_or(u32::from(GearBls12_381Error::EmptyPointList))?;

        // let point_aggregated = points
        //     .iter()
        //     .skip(1)
        //     .fold(*point_first, |aggregated, point| aggregated + *point);

        // Ok(ArkScale::<G1>::from(point_aggregated).encode())
        todo!()
    }

    /// Map a message to G2Affine-point using the domain separation tag from `milagro_bls`.
    /// Result is encoded `ArkScale<G2Affine>`.
    fn map_to_g2affine(message: &[u8]) -> Result<Vec<u8>, u32> {
        // type ArkScale<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;
        // type WBMap = wb::WBMap<<ark_bls12_381::Config as Bls12Config>::G2Config>;

        // let mapper =
        //     MapToCurveBasedHasher::<G2, DefaultFieldHasher<sha2::Sha256>, WBMap>::new(DST_G2)
        //         .map_err(|_| u32::from(GearBls12_381Error::MapperCreation))?;

        // let point = mapper
        //     .hash(message)
        //     .map_err(|_| u32::from(GearBls12_381Error::MessageMapping))?;

        // Ok(ArkScale::<G2Affine>::from(point).encode())
        todo!()
    }
}
