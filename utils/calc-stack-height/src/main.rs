// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use anyhow::{Context, ensure};
use gear_core::{
    code::{Code, TryNewCodeConfig},
    gas_metering::Schedule,
};
use gear_wasm_instrument::{STACK_HEIGHT_EXPORT_NAME, SystemBreakCode};
use std::{env, fs};
use tracing_subscriber::EnvFilter;
use wasmtime::{Engine, Linker, Memory, MemoryType, Module, Store, Trap, ValType};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive("info".parse()?)
                .from_env_lossy(),
        )
        .init();

    let schedule = Schedule::default();
    let inf_recursion = fs::read_to_string("examples/wat/spec/inf_recursion.wat")
        .context("Failed to read `inf_recursion.wat`")?;
    let inf_recursion = wat::parse_str(inf_recursion).context("Failed to convert WAT to WASM")?;

    let code = Code::try_new_mock_with_rules(
        inf_recursion.clone(),
        |module| schedule.rules(module),
        TryNewCodeConfig {
            version: schedule.instruction_weights.version,
            stack_height: Some(u32::MAX),
            export_stack_height: true,
            ..Default::default()
        },
    )
    .context("Code error")?;

    let mut config = wasmtime::Config::default();
    config
        .strategy(wasmtime::Strategy::Winch)
        .macos_use_mach_ports(false);
    let engine = Engine::new(&config).context("Failed to create engine")?;
    let mut store = Store::new(&engine, ());
    let module = Module::new(store.engine(), code.instrumented_code().bytes())
        .context("Failed to create initial module")?;

    let mut linker = Linker::new(store.engine());

    let memory =
        Memory::new(&mut store, MemoryType::new(0, None)).context("Failed to create memory")?;
    linker
        .define(&store, "env", "memory", memory)
        .context("Failed to define memory")?;

    // Here we need to repeat the code from
    // `gear_sandbox_host::sandbox::wasmtime_backend::dispatch_function_v2`, as we
    // want to be as close as possible to how the executor uses the stack in the
    // node.

    let func_ty = wasmtime::FuncType::new(store.engine(), [ValType::I32], []);
    let func = wasmtime::Func::new(
        &mut store,
        func_ty.clone(),
        move |_caller, params, _results| -> Result<(), wasmtime::Error> {
            match SystemBreakCode::try_from(params[0].unwrap_i32()) {
                Ok(SystemBreakCode::StackLimitExceeded) => {
                    Err(anyhow::anyhow!("stack limit exceeded"))
                }
                _ => Ok(()),
            }
        },
    );

    linker
        .define(&mut store, "env", "gr_system_break", func)
        .context("Failed to define gr_system_break")?;

    let instance = linker
        .instantiate(&mut store, &module)
        .context("Failed to instantiate initial module")?;
    let init = instance
        .get_func(&mut store, "init")
        .context("Failed to get initial `init` function export")?;
    let err = init.call(&mut store, &[], &mut []).unwrap_err();
    assert_eq!(
        err.root_cause().downcast_ref::<Trap>().copied(),
        Some(Trap::StackOverflow)
    );

    let stack_height = instance
        .get_global(&mut store, STACK_HEIGHT_EXPORT_NAME)
        .context("Failed to get global")?
        .get(&mut store)
        .i32()
        .expect("Unexpected global type") as u32;
    log::info!("Stack has overflowed at {stack_height} height");

    log::info!("Binary search for maximum possible stack height");

    let mut low = 0;
    let mut high = stack_height - 1;

    let mut stack_height = 0;

    while low <= high {
        let mid = (low + high) / 2;

        let code = Code::try_new(
            inf_recursion.clone(),
            schedule.instruction_weights.version,
            |module| schedule.rules(module),
            Some(mid),
            schedule.limits.data_segments_amount.into(),
        )
        .context("Code error")?;

        let module = Module::new(store.engine(), code.instrumented_code().bytes())
            .context("Failed to create module")?;
        let instance = linker
            .instantiate(&mut store, &module)
            .context("Failed to instantiate module")?;
        let init = instance
            .get_func(&mut store, "init")
            .context("Failed to get `init` function export")?;
        let err = init.call(&mut store, &[], &mut []).unwrap_err();

        match err.root_cause().downcast_ref::<Trap>() {
            None => {
                low = mid + 1;

                stack_height = mid;

                log::info!("Unreachable at {mid} height");
            }
            Some(Trap::StackOverflow) => {
                high = mid - 1;

                log::info!("Overflow at {mid} height");
            }
            code => panic!("unexpected trap code: {code:?}"),
        }
    }

    println!(
        "Stack height is {} for {}-{}",
        stack_height,
        env::consts::OS,
        env::consts::ARCH
    );

    if let Some(schedule_stack_height) = schedule.limits.stack_height {
        ensure!(
            schedule_stack_height <= stack_height,
            "Stack height in runtime schedule must be decreased"
        );
    }

    Ok(())
}
