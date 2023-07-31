//! Benches for wasm module-linking.
#![feature(test)]

extern crate test;

use anyhow::Result;
use gear_wasm_builder::optimize;
use std::{path::PathBuf, process::Command};
use test::bench::Bencher;

const DEMO_MEMOP_RELATIVE_PATH: &str = "memop";
const DEMO_MEMOP_WASM_RELATIVE_PATH: &str =
    "memop/target/wasm32-unknown-unknown/release/memop.wasm";

/// Build demo memop with different features.
fn build_demo(gwasm: bool) -> Result<Vec<u8>> {
    let demo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(DEMO_MEMOP_RELATIVE_PATH)
        .canonicalize()?;
    let mut args = vec!["build", "--release", "--no-default-features"];

    if gwasm {
        args.push("--features");
        args.push("gwasm");
    }

    Command::new("cargo")
        .env("RUSTFLAGS", "-C link-arg=--import-memory")
        .args(&args)
        .current_dir(demo)
        .status()?;

    optimize::optimize_wasm(
        DEMO_MEMOP_WASM_RELATIVE_PATH.into(),
        DEMO_MEMOP_WASM_RELATIVE_PATH.into(),
        "z",
        false,
    )?;
    std::fs::read(DEMO_MEMOP_WASM_RELATIVE_PATH).map_err(Into::into)
}

// #[bench]
// fn bench_galloc_memop(b: &mut Bencher) -> Result<()> {
//     let wasm = build_demo(false)?;
//     b.iter(|| execute(&wasm).expect("Failed to execute"));
//
//     Ok(())
// }

mod wasmtime {
    use crate::{build_demo, Bencher, Result};

    fn execute(wasm: &[u8]) -> Result<()> {
        use ::wasmtime::{Engine, Linker, Memory, MemoryType, Module, Store};

        let engine = Engine::default();
        let mut linker = Linker::<()>::new(&engine);
        let mut store = Store::new(&engine, ());

        // TODO: calculate this memory.
        let memory = Memory::new(&mut store, MemoryType::new(0xffff, None))?;
        linker.define(&mut store, "env", "memory", memory)?;

        let memop = Module::from_binary(&engine, wasm)?;
        let dlmalloc = Module::from_binary(&engine, &gwasm_processor::DLMALLOC)?;

        let dlmalloc = linker.instantiate(&mut store, &dlmalloc)?;
        linker.instance(&mut store, "gwasm-dlmalloc", dlmalloc)?;

        let memop = linker.instantiate(&mut store, &memop)?;
        let run = memop.get_typed_func::<(), i64>(&mut store, "memop")?;
        let res = run.call(&mut store, ())?;

        assert_eq!(res, 1);

        Ok(())
    }

    #[bench]
    fn bench_gwasm_memop(b: &mut Bencher) -> Result<()> {
        let wasm = build_demo(true)?;
        b.iter(|| execute(&wasm).expect("Failed to execute"));

        Ok(())
    }
}

mod wasmer {
    use crate::{build_demo, Bencher, Result};

    fn execute(wasm: &[u8]) -> Result<()> {
        use ::wasmer::{Imports, Instance, Memory, MemoryType, Module, Store, Value};

        let mut store = Store::default();
        let dlmalloc = Module::new(&store.engine(), &gwasm_processor::DLMALLOC)?;
        let memop = Module::new(&store.engine(), &wasm)?;

        let memory = Memory::new(&mut store, MemoryType::new(0xffff, None, false))?;
        let mut imports = Imports::new();
        imports.define("env", "memory", memory);

        let dlmalloc = Instance::new(&mut store, &dlmalloc, &Default::default())?;
        imports.register_namespace("gwasm-dlmalloc", dlmalloc.exports);

        let memop = Instance::new(&mut store, &memop, &imports)?;
        let res = memop.exports.get_function("memop")?.call(&mut store, &[])?;

        assert_eq!(res[0], Value::I64(1));

        Ok(())
    }

    #[bench]
    fn bench_gwasm_memop(b: &mut Bencher) -> Result<()> {
        let wasm = build_demo(true)?;
        b.iter(|| execute(&wasm).expect("Failed to execute"));

        Ok(())
    }
}
