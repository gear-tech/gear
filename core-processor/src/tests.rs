use crate::{ext::ExtInfo, ProcessorContext, ProcessorExternalities};
use alloc::{vec, vec::Vec};
use gear_core::{
    code::Code,
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    gas_metering::CustomConstantCostRules,
    ids::{prelude::*, CodeId, ProgramId},
    memory::{Memory, MemoryDump, MemoryError},
    message::{
        ContextSettings, DispatchKind, IncomingDispatch, IncomingMessage, MessageContext, Payload,
    },
};
use gear_core_backend::env::{
    BackendReport, Environment, EnvironmentExecutionResult, FailedExecution, ReadyToExecute,
    SuccessExecution,
};
use gear_lazy_pages::LazyPagesVersion;
use gear_lazy_pages_common::LazyPagesInitContext;
use gear_lazy_pages_native_interface::LazyPagesNative;
use parity_scale_codec::Encode;

type Ext = crate::Ext<LazyPagesNative>;

#[derive(Copy, Clone)]
enum ExecutionExpectation {
    Success,
    Failure,
}

enum MemoryAction {
    Init,
    Skip,
}

struct TestConfig {
    pub name: &'static str,
    pub payload: Payload,
    pub dispatch_kind: DispatchKind,
    pub memory_action: MemoryAction,
    pub expectation: ExecutionExpectation,
}

#[test]
fn execute_environment_multiple_times_with_memory_replacing() {
    use demo_fungible_token::{FTAction, InitConfig};

    let configs = vec![
        TestConfig {
            name: "Init",
            payload: Payload::try_from(InitConfig::test_sequence().encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Init,
            memory_action: MemoryAction::Init,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Mint 1_000_000 tokens to main user
            name: "Mint 1_000_000",
            payload: Payload::try_from(FTAction::Mint(1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            memory_action: MemoryAction::Skip,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Try to burn more tokens than available
            name: "Burn 2_000_000",
            payload: Payload::try_from(FTAction::Burn(2_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            memory_action: MemoryAction::Skip,
            expectation: ExecutionExpectation::Failure,
        },
        TestConfig {
            // Mint 3_000_000 tokens to main user
            name: "Mint 3_000_000",
            payload: Payload::try_from(FTAction::Mint(3_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            memory_action: MemoryAction::Init,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Burn 1_000_000 tokens
            name: "Burn 1_000_000",
            payload: Payload::try_from(FTAction::Burn(1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            memory_action: MemoryAction::Skip,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            name: "TotalSupply",
            payload: Payload::try_from(FTAction::TotalSupply.encode()).expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            memory_action: MemoryAction::Skip,
            expectation: ExecutionExpectation::Success,
        },
    ];

    let mut memory_dump = MemoryDump::new();

    let _ext_info = chain_execute(configs, &mut memory_dump);
}

#[test]
fn execute_environment_multiple_times_and_compare_results() {
    use demo_fungible_token::{FTAction, InitConfig};

    let configs_failed_execution_memory_replace = vec![
        TestConfig {
            name: "Init",
            payload: Payload::try_from(InitConfig::test_sequence().encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Init,
            memory_action: MemoryAction::Init,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Mint 1_000_000 tokens to main user
            name: "Mint 1_000_000",
            payload: Payload::try_from(FTAction::Mint(1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            memory_action: MemoryAction::Skip,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Try to burn more tokens than available
            name: "Burn 2_000_000",
            payload: Payload::try_from(FTAction::Burn(2_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            memory_action: MemoryAction::Skip,
            expectation: ExecutionExpectation::Failure,
        },
        TestConfig {
            // Burn 1_000_000 tokens
            name: "Burn 1_000_000",
            payload: Payload::try_from(FTAction::Burn(1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            memory_action: MemoryAction::Skip,
            expectation: ExecutionExpectation::Success,
        },
    ];

    let configs_normal_execution = vec![
        TestConfig {
            name: "Init",
            payload: Payload::try_from(InitConfig::test_sequence().encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Init,
            memory_action: MemoryAction::Init,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Mint 1_000_000 tokens to main user
            name: "Mint 1_000_000",
            payload: Payload::try_from(FTAction::Mint(1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            memory_action: MemoryAction::Skip,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Burn 1_000_000 tokens
            name: "Burn 1_000_000",
            payload: Payload::try_from(FTAction::Burn(1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            memory_action: MemoryAction::Skip,
            expectation: ExecutionExpectation::Success,
        },
    ];

    let configs_normal_execution_2 = vec![
        TestConfig {
            name: "Init",
            payload: Payload::try_from(InitConfig::test_sequence().encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Init,
            memory_action: MemoryAction::Init,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Mint 1_000_000 tokens to main user
            name: "Mint 1_000_000",
            payload: Payload::try_from(FTAction::Mint(1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            memory_action: MemoryAction::Skip,
            expectation: ExecutionExpectation::Success,
        },
    ];

    let mut memory_dump = MemoryDump::new();

    let ext_info_failed_with_memory_replace =
        chain_execute(configs_failed_execution_memory_replace, &mut memory_dump);
    let ext_info_normal = chain_execute(configs_normal_execution, &mut memory_dump);
    let ext_info_normal_2 = chain_execute(configs_normal_execution_2, &mut memory_dump);

    assert_eq!(
        ext_info_failed_with_memory_replace,
        ext_info_normal,
        "Execution results are different, failed execution: {ext_info_failed_with_memory_replace:?}, normal execution: {ext_info_normal:?}"
    );

    assert_ne!(
        ext_info_normal,
        ext_info_normal_2,
        "Execution results are the same, normal execution: {ext_info_normal:?}, normal execution 2: {ext_info_normal_2:?}"
    );

    assert_eq!(
        ext_info_failed_with_memory_replace,
        ext_info_normal,
        "Execution results are different, failed execution: {ext_info_failed_with_memory_replace:?}, normal execution: {ext_info_normal:?}"
    );
}

fn chain_execute(test_configs: Vec<TestConfig>, memory_dump: &mut MemoryDump) -> ExtInfo {
    use demo_fungible_token::WASM_BINARY;
    const PROGRAM_STORAGE_PREFIX: [u8; 32] = *b"execute_wasm_multiple_times_test";

    gear_lazy_pages::init(
        LazyPagesVersion::Version1,
        LazyPagesInitContext::new(PROGRAM_STORAGE_PREFIX),
        (),
    )
    .expect("Failed to init lazy-pages");

    let code = WASM_BINARY.to_vec();

    let code = Code::try_new(
        code,
        1,
        |_| CustomConstantCostRules::new(0, 0, 0),
        None,
        None,
    )
    .expect("Failed to create Code");

    let code_id = CodeId::generate(code.original_code());
    let program_id = ProgramId::generate_from_user(code_id, b"");

    let mut env;
    let mut execution_result: Option<EnvironmentExecutionResult<Ext>> = None;

    let mut previous_expectation = ExecutionExpectation::Success;
    let mut previous_config_name = "";

    for (i, test_config) in test_configs.into_iter().enumerate() {
        if i == 0 {
            let ext = make_ext(program_id, test_config.payload);

            env = Environment::new(
                ext,
                code.code(),
                vec![DispatchKind::Init, DispatchKind::Handle]
                    .into_iter()
                    .collect(),
                512.into(),
            )
            .unwrap_or_else(|_| {
                panic!(
                    "Failed to create environment, config name: {}",
                    test_config.name
                )
            });
        } else {
            env = inspect_and_set_payload(
                execution_result.take().unwrap(),
                program_id,
                test_config.payload,
                previous_expectation,
                memory_dump,
                previous_config_name,
            );
        }

        // Store expectation for next iteration
        previous_expectation = test_config.expectation;
        previous_config_name = test_config.name;

        execution_result = Some(execute_for_result(
            env,
            test_config.dispatch_kind,
            program_id,
            test_config.memory_action,
            test_config.name,
        ));
    }

    let env = match execution_result {
        Some(Ok(success_execution)) => success_execution,
        Some(Err(_failed_execution)) => {
            panic!(
                "Execution result is failed, config name: {}",
                previous_config_name
            )
        }
        None => {
            panic!(
                "Execution result is None, config name: {}",
                previous_config_name
            )
        }
    };

    let BackendReport {
        mut store,
        mut memory,
        ext,
        ..
    } = env.report();

    // released pages initial data will be added to `pages_initial_data` after execution.
    Ext::lazy_pages_post_execution_actions(&mut store, &mut memory);

    ext.into_ext_info(&mut store, &memory)
        .expect("Failed to get ext info")
}

fn execute_for_result<'a>(
    env: Environment<'a, Ext, ReadyToExecute>,
    dispatch_kind: DispatchKind,
    program_id: ProgramId,
    memory_action: MemoryAction,
    config_name: &str,
) -> EnvironmentExecutionResult<'a, Ext> {
    match memory_action {
        MemoryAction::Init => env
            .execute(dispatch_kind, |ctx, mem, globals_config| {
                Ext::lazy_pages_init_for_program(
                    ctx,
                    mem,
                    program_id,
                    Default::default(),
                    Some(mem.size(ctx).to_page_number().unwrap_or_else(|| {
                        panic!(
                            "Memory size is 4GB, so cannot be stack end, config_name: {}",
                            config_name
                        )
                    })),
                    globals_config,
                    Default::default(),
                );
            })
            .unwrap_or_else(|_| {
                panic!(
                    "Failed to execute WASM module, config_name: {}",
                    config_name
                );
            }),
        MemoryAction::Skip => env
            .execute(dispatch_kind, |_, _, _| {})
            .unwrap_or_else(|_| {
                panic!(
                    "Failed to execute WASM module, config_name: {}",
                    config_name
                );
            }),
    }
}

fn inspect_and_set_payload<'a>(
    execution_result: EnvironmentExecutionResult<'a, Ext>,
    program_id: ProgramId,
    payload: Payload,
    expectation: ExecutionExpectation,
    memory_dump: &mut MemoryDump,
    config_name: &str,
) -> Environment<'a, Ext, ReadyToExecute> {
    match execution_result {
        Ok(success_execution) => match expectation {
            ExecutionExpectation::Success => {
                let new_memory_dump = dump_memory(&success_execution).unwrap_or_else(|_| {
                    panic!("Failed to dump memory, config name: {}", config_name)
                });
                memory_dump
                    .try_replace(new_memory_dump)
                    .unwrap_or_else(|_| {
                        panic!(
                            "Failed to replace memory dump, config name: {}",
                            config_name
                        )
                    });

                let ext = make_ext(program_id, payload);

                success_execution
                    .set_ext(ext)
                    .unwrap_or_else(|_| panic!("Failed to set ext, config name: {}", config_name))
            }
            ExecutionExpectation::Failure => {
                panic!(
                    "Should succeed to execute WASM module, config name: {}",
                    config_name
                );
            }
        },
        Err(failed_execution) => match expectation {
            ExecutionExpectation::Success => {
                panic!(
                    "Should fail to execute WASM module, config name: {}",
                    config_name
                );
            }
            ExecutionExpectation::Failure => {
                let ext = make_ext(program_id, payload);

                let success_execution =
                    set_memory(failed_execution, memory_dump).unwrap_or_else(|_| {
                        panic!("Failed to set memory, config name: {}", config_name)
                    });

                success_execution
                    .set_ext(ext)
                    .unwrap_or_else(|_| panic!("Failed to set ext, config name: {}", config_name))
            }
        },
    }
}

fn make_ext(program_id: ProgramId, payload: Payload) -> Ext {
    let incoming_message = IncomingMessage::new(
        0.into(),
        message_sender(),
        payload,
        Default::default(),
        Default::default(),
        Default::default(),
    );

    let message_context = MessageContext::new(
        IncomingDispatch::new(DispatchKind::Init, incoming_message, None),
        program_id,
        ContextSettings::with_outgoing_limits(1024, u32::MAX),
    );

    let processor_context = ProcessorContext {
        message_context,
        program_id,
        value_counter: ValueCounter::new(250_000_000_000),
        gas_counter: GasCounter::new(250_000_000_000),
        gas_allowance_counter: GasAllowanceCounter::new(250_000_000_000),
        ..ProcessorContext::new_mock()
    };

    Ext::new(processor_context)
}

fn dump_memory(env: &Environment<Ext, SuccessExecution>) -> Result<MemoryDump, MemoryError> {
    env.dump_memory(Ext::dump_memory)
}

fn set_memory<'a>(
    env: Environment<'a, Ext, FailedExecution>,
    dump: &MemoryDump,
) -> Result<Environment<'a, Ext, SuccessExecution>, MemoryError> {
    env.set_memory(|ctx, memory| Ext::set_memory(ctx, memory, dump))
}

fn message_sender() -> ProgramId {
    let bytes = [1, 2, 3, 4].repeat(8);
    ProgramId::try_from(bytes.as_ref()).unwrap()
}
