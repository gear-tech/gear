use gear_core::code::{Code, TryNewCodeConfig};
use std::{env, error::Error, fs};
use wasmer::{
    Exports, Extern, Function, ImportObject, Instance, Memory, MemoryType, Module, Store,
};
use wasmer_types::TrapCode;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

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
    .map_err(|e| e.to_string())?;

    let compiler = wasmer::Singlepass::default();
    let store = Store::new(&wasmer::Universal::new(compiler).engine());
    let module = Module::new(&store, code.code())?;

    let mut imports = ImportObject::new();
    let mut env = Exports::new();

    let memory = Memory::new(&store, MemoryType::new(0, None, false))?;
    env.insert("memory", Extern::Memory(memory));

    let func = Function::new_native(&store, || {});
    env.insert("gr_out_of_gas", func);

    imports.register("env", env);

    let instance = Instance::new(&module, &imports)?;
    let init = instance.exports.get_function("init")?;
    let err = init.call(&[]).unwrap_err();
    assert_eq!(err.to_trap(), Some(TrapCode::StackOverflow));

    let stack_height = instance
        .exports
        .get_global("__gear_stack_height")?
        .get()
        .i32()
        .expect("Unexpected global type") as u32;
    log::info!("Stack has overflowed at {} height", stack_height);

    log::info!("Binary search for maximum possible stack height");

    let mut low = 0;
    let mut high = stack_height - 1;

    let mut stack_height = 0;

    loop {
        let mid = (low + high) / 2;

        let code = Code::try_new(
            inf_recursion.as_ref().to_vec(),
            schedule.instruction_weights.version,
            |module| schedule.rules(module),
            Some(mid),
        )
        .map_err(|e| e.to_string())?;

        let module = Module::new(&store, code.code())?;
        let instance = Instance::new(&module, &imports)?;
        let init = instance.exports.get_function("init")?;
        let err = init.call(&[]).unwrap_err();

        let stop = low == high;

        match err.to_trap() {
            Some(TrapCode::UnreachableCodeReached) => {
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

        if stop {
            break;
        }
    }

    println!(
        "Stack height is {} for {}-{}",
        stack_height,
        env::consts::OS,
        env::consts::ARCH
    );

    if let Some(schedule_stack_height) = schedule.limits.stack_height {
        if schedule_stack_height > stack_height {
            return Err("Stack height in runtime schedule must be decreased".into());
        }
    }

    Ok(())
}
