use gear_core::code::{Code, TryNewCodeConfig};
use gear_wasm_instrument::{SystemBreakCode, STACK_HEIGHT_EXPORT_NAME};
use sandbox_wasmer::{
    Exports, Extern, Function, HostEnvInitError, ImportObject, Instance, Memory, MemoryType,
    Module, RuntimeError, Singlepass, Store, Universal, WasmerEnv,
};
use sandbox_wasmer_types::{FunctionType, TrapCode, Type};
use std::{env, fs};

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
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    let compiler = Singlepass::default();
    let store = Store::new(&Universal::new(compiler).engine());
    let module = Module::new(&store, code.code())?;

    let mut imports = ImportObject::new();
    let mut env = Exports::new();

    let memory = Memory::new(&store, MemoryType::new(0, None, false))?;
    env.insert("memory", Extern::Memory(memory));

    // Here we need to repeat the code from
    // `gear_sandbox_host::sandbox::wasmer_backend::dispatch_function_v2`, as we
    // want to be as close as possible to how the executor uses the stack in the
    // node.

    #[derive(Default, Clone)]
    struct Env;

    impl WasmerEnv for Env {
        fn init_with_instance(&mut self, _: &Instance) -> Result<(), HostEnvInitError> {
            Ok(())
        }
    }

    let ty = FunctionType::new(vec![Type::I32], vec![]);
    let func = Function::new_with_env(&store, &ty, Env, |_, args| match SystemBreakCode::try_from(
        args[0].unwrap_i32(),
    ) {
        Ok(SystemBreakCode::StackLimitExceeded) => Err(RuntimeError::new("stack limit exceeded")),
        _ => Ok(vec![]),
    });

    env.insert("gr_system_break", func);

    imports.register("env", env);

    let instance = Instance::new(&module, &imports)?;
    let init = instance.exports.get_function("init")?;
    let err = init.call(&[]).unwrap_err();
    assert_eq!(err.to_trap(), Some(TrapCode::StackOverflow));

    let stack_height = instance
        .exports
        .get_global(STACK_HEIGHT_EXPORT_NAME)?
        .get()
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
        .map_err(|e| anyhow::anyhow!("{e}"))?;

        let module = Module::new(&store, code.code())?;
        let instance = Instance::new(&module, &imports)?;
        let init = instance.exports.get_function("init")?;
        let err = init.call(&[]).unwrap_err();

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
