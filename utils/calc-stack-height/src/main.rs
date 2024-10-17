// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use anyhow::anyhow;
use gear_core::code::{Code, TryNewCodeConfig};
use gear_wasm_instrument::{SystemBreakCode, STACK_HEIGHT_EXPORT_NAME};
use std::{env, fs};
use wasmer::{
    Exports, Extern, Function, FunctionEnv, Imports, Instance, Memory, MemoryType, Module,
    RuntimeError, Singlepass, Store,
};
use wasmer_types::{FunctionType, TrapCode, Type};

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let schedule = vara_runtime::Schedule::get();
    let inf_recursion = fs::read("examples/wat/spec/inf_recursion.wat")?;
    let inf_recursion = wabt::Wat2Wasm::new().convert(inf_recursion)?;

    let code = Code::try_new_mock_with_rules(
        inf_recursion.as_ref().to_vec(),
        |module| schedule.rules(module),
        TryNewCodeConfig {
            version: schedule.instruction_weights.version,
            stack_height: Some(u32::MAX),
            export_stack_height: true,
            ..Default::default()
        },
    )
    .map_err(|e| anyhow!("{e}"))?;

    let compiler = Singlepass::default();
    let mut store = Store::new(compiler);
    let module = Module::new(&store, code.code())?;

    let mut imports = Imports::new();
    let mut exports = Exports::new();

    let memory = Memory::new(&mut store, MemoryType::new(0, None, false))?;
    exports.insert("memory", Extern::Memory(memory));

    // Here we need to repeat the code from
    // `gear_sandbox_host::sandbox::wasmer_backend::dispatch_function_v2`, as we
    // want to be as close as possible to how the executor uses the stack in the
    // node.

    let env = FunctionEnv::new(&mut store, ());
    let ty = FunctionType::new(vec![Type::I32], vec![]);
    let func =
        Function::new_with_env(
            &mut store,
            &env,
            &ty,
            |_, args| match SystemBreakCode::try_from(args[0].unwrap_i32()) {
                Ok(SystemBreakCode::StackLimitExceeded) => {
                    Err(RuntimeError::new("stack limit exceeded"))
                }
                _ => Ok(vec![]),
            },
        );

    exports.insert("gr_system_break", func);

    imports.register_namespace("env", exports);

    let instance = Instance::new(&mut store, &module, &imports)?;
    let init = instance.exports.get_function("init")?;
    let err = init.call(&mut store, &[]).unwrap_err();
    assert_eq!(err.to_trap(), Some(TrapCode::StackOverflow));

    let stack_height = instance
        .exports
        .get_global(STACK_HEIGHT_EXPORT_NAME)?
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
            inf_recursion.as_ref().to_vec(),
            schedule.instruction_weights.version,
            |module| schedule.rules(module),
            Some(mid),
            schedule.limits.data_segments_amount.into(),
            schedule.limits.table_number.into(),
        )
        .map_err(|e| anyhow!("{e}"))?;

        let module = Module::new(&store, code.code())?;
        let instance = Instance::new(&mut store, &module, &imports)?;
        let init = instance.exports.get_function("init")?;
        let err = init.call(&mut store, &[]).unwrap_err();

        match err.to_trap() {
            None => {
                low = mid + 1;

                stack_height = mid;

                log::info!("Unreachable at {} height", mid);
            }
            Some(TrapCode::StackOverflow) => {
                high = mid - 1;

                log::info!("Overflow at {} height", mid);
            }
            code => panic!("unexpected trap code: {:?}", code),
        }
    }

    println!(
        "Stack height is {} for {}-{}",
        stack_height,
        env::consts::OS,
        env::consts::ARCH
    );

    if let Some(schedule_stack_height) = schedule.limits.stack_height {
        anyhow::ensure!(
            schedule_stack_height <= stack_height,
            "Stack height in runtime schedule must be decreased"
        );
    }

    Ok(())
}
