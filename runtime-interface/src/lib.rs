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

//! Runtime interface for gear node

#![allow(useless_deprecated, deprecated)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use byteorder::{ByteOrder, LittleEndian};
use codec::{Decode, Encode};
use gear_core::{
    gas::GasLeft,
    memory::{HostPointer, MemoryInterval},
    str::LimitedStr,
};
#[cfg(feature = "std")]
use gear_lazy_pages::LazyPagesStorage;
use gear_lazy_pages_common::{GlobalsAccessConfig, ProcessAccessError, Status};
use sp_runtime_interface::{
    pass_by::{Codec, PassBy},
    runtime_interface,
};
use sp_std::{convert::TryFrom, mem, result::Result, vec::Vec};

mod gear_sandbox;

#[cfg(feature = "std")]
pub use gear_sandbox::init as sandbox_init;
pub use gear_sandbox::sandbox;

const _: () = assert!(core::mem::size_of::<HostPointer>() >= core::mem::size_of::<usize>());

#[derive(Debug, Clone, Encode, Decode)]
#[codec(crate = codec)]
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
#[codec(crate = codec)]
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

fn deserialize_mem_intervals(bytes: &[u8], intervals: &mut Vec<MemoryInterval>) {
    let mem_interval_size = mem::size_of::<MemoryInterval>();
    for chunk in bytes.chunks_exact(mem_interval_size) {
        // can't panic because of chunks_exact
        intervals.push(MemoryInterval::try_from_bytes(chunk).unwrap());
    }
}

#[derive(Debug, Clone, Encode, Decode)]
#[codec(crate = codec)]
pub enum ProcessAccessErrorVer1 {
    OutOfBounds,
    GasLimitExceeded,
    GasAllowanceExceeded,
}

/// Runtime interface for gear node and runtime.
/// Note: name is expanded as gear_ri
#[runtime_interface]
pub trait GearRI {
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

    #[version(2)]
    fn pre_process_memory_accesses(reads: &[u8], writes: &[u8], gas_bytes: &mut [u8; 8]) -> u8 {
        let mem_interval_size = mem::size_of::<MemoryInterval>();
        let reads_len = reads.len();
        let writes_len = writes.len();

        let mut reads_intervals = Vec::with_capacity(reads_len / mem_interval_size);
        deserialize_mem_intervals(reads, &mut reads_intervals);
        let mut writes_intervals = Vec::with_capacity(writes_len / mem_interval_size);
        deserialize_mem_intervals(writes, &mut writes_intervals);

        let mut gas_counter = LittleEndian::read_u64(gas_bytes);

        let res = match gear_lazy_pages::pre_process_memory_accesses(
            &reads_intervals,
            &writes_intervals,
            &mut gas_counter,
        ) {
            Ok(_) => 0,
            Err(err) => err.into(),
        };

        LittleEndian::write_u64(gas_bytes, gas_counter);

        res
    }

    fn lazy_pages_status() -> (Status,) {
        (gear_lazy_pages::status()
            .unwrap_or_else(|err| unreachable!("Cannot get lazy-pages status: {err}")),)
    }

    /// Init lazy-pages.
    /// Returns whether initialization was successful.
    fn init_lazy_pages(ctx: LazyPagesInitContext) -> bool {
        use gear_lazy_pages::LazyPagesVersion;

        gear_lazy_pages::init(LazyPagesVersion::Version1, ctx.into(), SpIoProgramStorage)
            .map_err(|err| log::error!("Cannot initialize lazy-pages: {}", err))
            .is_ok()
    }

    /// Init lazy pages context for current program.
    /// Panic if some goes wrong during initialization.
    fn init_lazy_pages_for_program(ctx: LazyPagesProgramContext) {
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
    fn mprotect_lazy_pages(protect: bool) {
        if protect {
            gear_lazy_pages::set_lazy_pages_protection()
        } else {
            gear_lazy_pages::unset_lazy_pages_protection()
        }
        .map_err(|err| err.to_string())
        .expect("Cannot set/unset mprotection for lazy pages");
    }

    fn change_wasm_memory_addr_and_size(addr: Option<HostPointer>, size: Option<u32>) {
        // `as usize` is safe, because of const assert above.
        gear_lazy_pages::change_wasm_mem_addr_and_size(addr.map(|addr| addr as usize), size)
            .unwrap_or_else(|err| unreachable!("Cannot set new wasm addr and size: {err}"));
    }

    fn write_accessed_pages() -> Vec<u32> {
        gear_lazy_pages::write_accessed_pages()
            .unwrap_or_else(|err| unreachable!("Cannot get write accessed pages: {err}"))
    }

    // Bellow goes deprecated runtime interface functions.
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

#[repr(u32)]
pub enum Plonky2VerifyResult {
    Verified,
    Rejected,
    FailedToDecodeCommonData,
    FailedToDecodeVerifierData,
    FailedToDecodeProof,
    ConfigNotSupported,
}

impl From<Plonky2VerifyResult> for u32 {
    fn from(value: Plonky2VerifyResult) -> Self {
        value as u32
    }
}

/// The struct mirrors the one from plonky2.
#[derive(Debug, Clone, Eq, PartialEq, Decode, Encode)]
#[codec(crate = codec)]
pub struct FriConfig {
    pub rate_bits: u32,
    pub cap_height: u32,
    pub proof_of_work_bits: u32,
    pub num_query_rounds: u32,
}

/// The struct mirrors the one from plonky2.
#[derive(Clone, Debug, Eq, PartialEq, Decode, Encode)]
#[codec(crate = codec)]
pub struct CircuitConfig {
    pub num_wires: u32,
    pub num_routed_wires: u32,
    pub num_constants: u32,
    pub use_base_arithmetic_gate: bool,
    pub security_bits: u32,
    pub num_challenges: u32,
    pub zero_knowledge: bool,
    pub max_quotient_degree_factor: u32,
    pub fri_config: FriConfig,
}

#[runtime_interface]
pub trait SpecificPlonky2 {
    /// Returns encoded (CircuitConfig, public_input_count: u32) on success.
    fn decode(common_circuit_data: Vec<u8>, proof: Vec<u8>) -> Result<Vec<u8>, ()> {
        use plonky2::{
            plonk::{
                self,
                config::{GenericConfig, PoseidonGoldilocksConfig},
            },
            util::serialization::DefaultGateSerializer,
        };

        pub const DIMENSION: usize = 2;
        pub type Config = PoseidonGoldilocksConfig;
        pub type Field = <Config as GenericConfig<DIMENSION>>::F;
        pub type CommonCircuitData = plonk::circuit_data::CommonCircuitData<Field, DIMENSION>;
        pub type ProofWithPublicInputs = plonk::proof::ProofWithPublicInputs<Field, Config, DIMENSION>;

        let Ok(common) = CommonCircuitData::from_bytes(common_circuit_data, &DefaultGateSerializer) else {
            return Err(());
        };

        let Ok(proof_with_pis) = ProofWithPublicInputs::from_bytes(proof, &common) else {
            return Err(());
        };

        let config = &common.config;
        Ok(
            (CircuitConfig {
                num_wires: config.num_wires as u32,
                num_routed_wires: config.num_routed_wires as u32,
                num_constants: config.num_constants as u32,
                use_base_arithmetic_gate: config.use_base_arithmetic_gate,
                security_bits: config.security_bits as u32,
                num_challenges: config.num_challenges as u32,
                zero_knowledge: config.zero_knowledge,
                max_quotient_degree_factor: config.max_quotient_degree_factor as u32,
                fri_config: FriConfig {
                    rate_bits: config.fri_config.rate_bits as u32,
                    cap_height: config.fri_config.cap_height as u32,
                    proof_of_work_bits: config.fri_config.proof_of_work_bits as u32,
                    num_query_rounds: config.fri_config.num_query_rounds as u32
                },
            },
            proof_with_pis.public_inputs.len() as u32,
            ).encode()
        )
    }

    fn verify(
        common_curcuit_data: Vec<u8>,
        verifier_circuit_data: Vec<u8>,
        proof: Vec<u8>,
    ) -> u32 {
        use plonky2::plonk::{
            self,
            config::{GenericConfig, PoseidonGoldilocksConfig},
        };
        use plonky2::util::serialization::DefaultGateSerializer;

        pub const DIMENSION: usize = 2;
        pub type Config = PoseidonGoldilocksConfig;
        pub type Field = <Config as GenericConfig<DIMENSION>>::F;
        // pub type CircuitData = plonk::circuit_data::CircuitData<Field, Config, DIMENSION>;
        pub type CommonCircuitData = plonk::circuit_data::CommonCircuitData<Field, DIMENSION>;
        pub type VerifierOnlyCircuitData = plonk::circuit_data::VerifierOnlyCircuitData<Config, DIMENSION>;
        pub type VerifierCircuitData = plonk::circuit_data::VerifierCircuitData<Field, Config, DIMENSION>;
        // pub type ProverOnlyCircuitData = plonk::circuit_data::ProverOnlyCircuitData<Field, Config, DIMENSION>;
        pub type ProofWithPublicInputs = plonk::proof::ProofWithPublicInputs<Field, Config, DIMENSION>;

        let Ok(common) = CommonCircuitData::from_bytes(common_curcuit_data, &DefaultGateSerializer) else {
            return Plonky2VerifyResult::FailedToDecodeCommonData.into();
        };

        if common.config.fri_config.rate_bits != 3
            || common.config.fri_config.proof_of_work_bits != 16
            || !matches!(common.config.fri_config.reduction_strategy, plonky2::fri::reduction_strategies::FriReductionStrategy::ConstantArityBits(..))
        {
            return Plonky2VerifyResult::ConfigNotSupported.into();
        }

        let Ok(verifier_only) = VerifierOnlyCircuitData::from_bytes(verifier_circuit_data) else {
            return Plonky2VerifyResult::FailedToDecodeVerifierData.into();
        };

        let Ok(proof_with_pis) = ProofWithPublicInputs::from_bytes(proof, &common) else {
            return Plonky2VerifyResult::FailedToDecodeProof.into();
        };

        let verifier_circuit_data = VerifierCircuitData { verifier_only, common };
        match verifier_circuit_data.verify(proof_with_pis) {
            Ok(()) => Plonky2VerifyResult::Verified,
            Err(e) => {
                log::debug!("VerifierCircuitData::verify failed: {e:?}");

                Plonky2VerifyResult::Rejected
            }
        }.into()
    }
}
