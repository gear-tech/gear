// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use wasmtime::{Cache, CacheConfig, Config, Engine, Module, Strategy};

const BIG_WASM: &[u8] = include_bytes!(concat!(
    env!("GEAR_WORKSPACE_DIR"),
    "/sdk/examples/big-wasm/big.wasm"
));

fn create_engine() -> Engine {
    let cache_dir = tempfile::tempdir().expect("temp dir is created").keep();
    let mut cache = CacheConfig::new();
    cache.with_directory(cache_dir.join("wasmtime-cache"));
    let cache = Cache::new(cache).expect("cache config is valid");

    let mut config = Config::new();
    config.strategy(Strategy::Winch).cache(Some(cache));
    Engine::new(&config).expect("engine config is valid")
}

fn lru_cache(c: &mut Criterion) {
    let code = BIG_WASM;

    let engine = create_engine();

    let mut group = c.benchmark_group("new_module");
    group.throughput(Throughput::Elements(1));

    group.bench_function("disk_cache", |b| {
        b.iter(|| Module::new(&engine, black_box(code)).unwrap())
    });

    group.bench_function("lru_cache", |b| {
        b.iter(|| gear_wasmtime_cache::get(&engine, black_box(code)).unwrap())
    });

    group.bench_function("engine_changed", |b| {
        b.iter_batched(
            create_engine,
            |engine| gear_wasmtime_cache::get(&engine, black_box(code)).unwrap(),
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, lru_cache);
criterion_main!(benches);
