// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use wasmtime::{Cache, CacheConfig, Config, Engine, Module, Strategy};

const BIG_WASM: &[u8] = include_bytes!(concat!(
    env!("GEAR_WORKSPACE_DIR"),
    "/sdk/examples/big-wasm/big.wasm"
));

fn lru_cache(c: &mut Criterion) {
    let code = BIG_WASM;

    let cache_dir = tempfile::tempdir().expect("temp dir is created");
    let mut cache = CacheConfig::new();
    cache.with_directory(cache_dir.path().join("wasmtime-cache"));
    let cache = Cache::new(cache).expect("cache config is valid");

    let mut config = Config::new();
    config.strategy(Strategy::Winch).cache(Some(cache));
    let engine = Engine::new(&config).expect("engine config is valid");

    c.bench_function("disk_cache", |b| {
        b.iter(|| Module::new(&engine, black_box(code)).expect("disk cache hit"))
    });

    c.bench_function("lru_cache", |b| {
        b.iter(|| gear_wasmtime_cache::get(&engine, black_box(code)).expect("LRU cache hit"))
    });
}

criterion_group!(benches, lru_cache);
criterion_main!(benches);
