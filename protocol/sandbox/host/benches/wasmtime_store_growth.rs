// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use criterion::{Criterion, criterion_group, criterion_main};
use gear_sandbox_env::{EnvironmentDefinition, Instantiate};
use gear_sandbox_host::{
    error::Result,
    sandbox::{GuestEnvironment, SandboxBackend, SandboxComponents, SupervisorContext},
};
use parity_scale_codec::Encode;
use sp_wasm_interface_common::{Pointer, WordSize};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

const EMPTY_WASM: &[u8] = b"\0asm\x01\0\0\0";
const MAX_OPERATIONS_PER_STORE: u64 =
    (wasmtime::DEFAULT_INSTANCE_LIMIT as u64 + wasmtime::DEFAULT_MEMORY_LIMIT as u64) / 2;

#[derive(Clone, Copy)]
enum ResetPolicy {
    Never,
    Every(usize),
}

impl ResetPolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Never => "long_lived_store",
            Self::Every(_) => "clear_periodically",
        }
    }
}

struct NoopSupervisor;

impl SupervisorContext for NoopSupervisor {
    fn invoke(
        &mut self,
        _invoke_args_ptr: Pointer<u8>,
        _invoke_args_len: WordSize,
        _func_idx: gear_sandbox_host::sandbox::SupervisorFuncIndex,
    ) -> Result<i64> {
        Ok(0)
    }

    fn read_memory_into(
        &self,
        _address: Pointer<u8>,
        dest: &mut [u8],
    ) -> std::result::Result<(), String> {
        dest.fill(0);
        Ok(())
    }

    fn write_memory(
        &mut self,
        _address: Pointer<u8>,
        _data: &[u8],
    ) -> std::result::Result<(), String> {
        Ok(())
    }

    fn allocate_memory(&mut self, _size: WordSize) -> std::result::Result<Pointer<u8>, String> {
        Ok(Pointer::null())
    }

    fn deallocate_memory(&mut self, _ptr: Pointer<u8>) -> std::result::Result<(), String> {
        Ok(())
    }
}

fn bench_wasmtime_store_growth(c: &mut Criterion) {
    let mut group = c.benchmark_group("wasmtime_store_growth");

    for reset_policy in [ResetPolicy::Never, ResetPolicy::Every(50)] {
        group.bench_function(reset_policy.as_str(), |b| {
            b.iter_custom(|iters| {
                let mut state = BenchState::new();
                let mut elapsed = Duration::ZERO;
                for i in 0..iters {
                    if i.is_multiple_of(MAX_OPERATIONS_PER_STORE) {
                        state = BenchState::new();
                    }

                    let instant = Instant::now();
                    state.run_iteration(i, reset_policy);
                    elapsed += instant.elapsed();
                }
                elapsed
            })
        });
    }

    group.finish();
}

struct BenchState {
    store: SandboxComponents<()>,
    env_def: Vec<u8>,
    supervisor: NoopSupervisor,
}

impl BenchState {
    fn new() -> Self {
        Self {
            store: SandboxComponents::new(SandboxBackend::Wasmtime),
            env_def: EnvironmentDefinition {
                entries: Vec::new(),
            }
            .encode(),
            supervisor: NoopSupervisor,
        }
    }

    fn run_iteration(&mut self, iteration: u64, reset_policy: ResetPolicy) {
        let memory_idx = self
            .store
            .new_memory(1, 1)
            .expect("failed to create sandbox memory");
        black_box(memory_idx);

        let guest_env = GuestEnvironment::decode(&self.store, &self.env_def)
            .unwrap_or_else(|_| panic!("failed to decode env"));
        let instance = self
            .store
            .instantiate(
                Instantiate::Version1,
                EMPTY_WASM,
                guest_env,
                &mut self.supervisor,
            )
            .unwrap_or_else(|_| panic!("failed to instantiate empty wasm"));
        let instance_idx = instance.register(&mut self.store, ());
        black_box(instance_idx);

        if let ResetPolicy::Every(clear_every) = reset_policy
            && iteration.is_multiple_of(clear_every as u64)
        {
            self.store.clear();
        }
    }
}

criterion_group!(benches, bench_wasmtime_store_growth);
criterion_main!(benches);
