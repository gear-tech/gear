use crate::{ProcessorContext, ProcessorExternalities, ext::ExtInfo};
use alloc::{vec, vec::Vec};
use gear_core::{
    buffer::Payload,
    code::Code,
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    gas_metering::CustomConstantCostRules,
    ids::{ActorId, CodeId, prelude::*},
    memory::AllocationsContext,
    message::{ContextSettings, DispatchKind, IncomingDispatch, IncomingMessage, MessageContext},
};
use gear_core_backend::{
    MemoryStorer,
    env::{BackendReport, Environment, ExecutedEnvironment, ReadyToExecute},
};
use gear_lazy_pages::{LazyPagesStorage, LazyPagesVersion};
use gear_lazy_pages_common::LazyPagesInitContext;
use gear_lazy_pages_native_interface::LazyPagesNative;
use parity_scale_codec::Encode;

type Ext = crate::Ext<LazyPagesNative>;

#[derive(Copy, Clone)]
enum ExecutionExpectation {
    Success,
    Failure,
}

struct TestConfig {
    pub name: &'static str,
    pub payload: Payload,
    pub dispatch_kind: DispatchKind,
    pub expectation: ExecutionExpectation,
}

#[derive(Debug)]
struct EmptyStorage;

impl LazyPagesStorage for EmptyStorage {
    fn page_exists(&self, _key: &[u8]) -> bool {
        false
    }

    fn load_page(&mut self, _key: &[u8], _buffer: &mut [u8]) -> Option<u32> {
        unreachable!();
    }
}

const MEMORY_SIZE: u16 = 100;
const MAX_MEMORY: u16 = 65000;

#[test]
fn execute_environment_multiple_times_with_memory_replacing() {
    use demo_fungible_token::{FTAction, InitConfig};

    let configs = vec![
        TestConfig {
            name: "Init",
            payload: Payload::try_from(InitConfig::test_sequence().encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Init,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Mint 1_000_000 tokens to main user
            name: "Mint 1_000_000",
            payload: Payload::try_from(FTAction::Mint(1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Try to burn more tokens than available
            name: "Burn 2_000_000",
            payload: Payload::try_from(FTAction::Burn(2_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Failure,
        },
        TestConfig {
            // Mint 3_000_000 tokens to main user
            name: "Mint 3_000_000",
            payload: Payload::try_from(FTAction::Mint(3_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Burn 1_000_000 tokens
            name: "Burn 1_000_000",
            payload: Payload::try_from(FTAction::Burn(1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            name: "TotalSupply",
            payload: Payload::try_from(FTAction::TotalSupply.encode()).expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
    ];

    let mut memory_dumper = Ext::memory_dumper();

    let _ext_info = chain_execute(configs, &mut memory_dumper);
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
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            name: "Test set",
            payload: Payload::try_from(FTAction::TestSet(0..500, 1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            name: "Test set",
            payload: Payload::try_from(FTAction::TestSet(0..500, 2_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Mint 1_000_000 tokens to main user
            name: "Mint 1_000_000",
            payload: Payload::try_from(FTAction::Mint(1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Try to burn more tokens than available
            name: "Burn 2_000_000",
            payload: Payload::try_from(FTAction::Burn(2_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Failure,
        },
        TestConfig {
            // Burn 1_000_000 tokens
            name: "Burn 1_000_000",
            payload: Payload::try_from(FTAction::Burn(1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
    ];

    let configs_normal_execution = vec![
        TestConfig {
            name: "Init",
            payload: Payload::try_from(InitConfig::test_sequence().encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Init,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            name: "Test set",
            payload: Payload::try_from(FTAction::TestSet(0..500, 1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            name: "Test set",
            payload: Payload::try_from(FTAction::TestSet(0..500, 2_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Mint 1_000_000 tokens to main user
            name: "Mint 1_000_000",
            payload: Payload::try_from(FTAction::Mint(1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Burn 1_000_000 tokens
            name: "Burn 1_000_000",
            payload: Payload::try_from(FTAction::Burn(1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
    ];

    let configs_normal_execution_2 = vec![
        TestConfig {
            name: "Init",
            payload: Payload::try_from(InitConfig::test_sequence().encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Init,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            name: "Test set",
            payload: Payload::try_from(FTAction::TestSet(0..500, 1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            name: "Test set",
            payload: Payload::try_from(FTAction::TestSet(0..500, 2_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
        TestConfig {
            // Mint 1_000_000 tokens to main user
            name: "Mint 1_000_000",
            payload: Payload::try_from(FTAction::Mint(1_000_000).encode())
                .expect("Failed to encode"),
            dispatch_kind: DispatchKind::Handle,
            expectation: ExecutionExpectation::Success,
        },
    ];

    let mut memory_dumper = Ext::memory_dumper();

    let ext_info_failed_with_memory_replace =
        chain_execute(configs_failed_execution_memory_replace, &mut memory_dumper);
    let ext_info_normal = chain_execute(configs_normal_execution, &mut memory_dumper);
    let ext_info_normal_2 = chain_execute(configs_normal_execution_2, &mut memory_dumper);

    assert_eq!(
        ext_info_failed_with_memory_replace, ext_info_normal,
        "Execution results are different, failed execution: {ext_info_failed_with_memory_replace:?}, normal execution: {ext_info_normal:?}"
    );

    assert_ne!(
        ext_info_normal, ext_info_normal_2,
        "Execution results are the same, normal execution: {ext_info_normal:?}, normal execution 2: {ext_info_normal_2:?}"
    );

    assert_eq!(
        ext_info_failed_with_memory_replace, ext_info_normal,
        "Execution results are different, failed execution: {ext_info_failed_with_memory_replace:?}, normal execution: {ext_info_normal:?}"
    );
}

fn chain_execute(test_configs: Vec<TestConfig>, memory_dumper: &mut impl MemoryStorer) -> ExtInfo {
    use demo_fungible_token::WASM_BINARY;
    const PROGRAM_STORAGE_PREFIX: [u8; 32] = *b"execute_wasm_multiple_times_test";

    gear_lazy_pages::init(
        LazyPagesVersion::Version1,
        LazyPagesInitContext::new(PROGRAM_STORAGE_PREFIX),
        EmptyStorage,
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
    let program_id = ActorId::generate_from_user(code_id, b"");

    let mut env;
    let mut execution_result: Option<ExecutedEnvironment<Ext>> = None;

    let mut previous_expectation = ExecutionExpectation::Success;
    let mut previous_config_name = "";

    for (i, test_config) in test_configs.into_iter().enumerate() {
        if i == 0 {
            let ext = make_ext(
                test_config.dispatch_kind,
                program_id,
                &code,
                test_config.payload,
            );

            env = Environment::new(
                ext,
                code.instrumented_code().bytes(),
                code.metadata().exports().clone(),
                MEMORY_SIZE.into(),
                |ctx, mem, globals_config| {
                    Ext::lazy_pages_init_for_program(
                        ctx,
                        mem,
                        program_id,
                        Default::default(),
                        code.metadata().stack_end(),
                        globals_config,
                        Default::default(),
                    );
                },
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
                test_config.dispatch_kind,
                program_id,
                &code,
                test_config.payload,
                previous_expectation,
                memory_dumper,
                previous_config_name,
            );
        }

        // Store expectation for next iteration
        previous_expectation = test_config.expectation;
        previous_config_name = test_config.name;

        execution_result = Some(
            env.execute(test_config.dispatch_kind, Some(memory_dumper))
                .unwrap_or_else(|e| {
                    panic!(
                        "Failed to execute WASM module, config_name: {}, with error: {}",
                        test_config.name, e
                    );
                }),
        );
    }

    let env = match execution_result {
        Some(ExecutedEnvironment::SuccessExecution(success_execution)) => success_execution,
        Some(ExecutedEnvironment::FailedExecution(_failed_execution)) => {
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

#[allow(clippy::too_many_arguments)]
fn inspect_and_set_payload<'a>(
    execution_result: ExecutedEnvironment<'a, Ext>,
    dispatch_kind: DispatchKind,
    actor_id: ActorId,
    code: &Code,
    payload: Payload,
    expectation: ExecutionExpectation,
    memory_dumper: &mut impl MemoryStorer,
    config_name: &str,
) -> Environment<'a, Ext, ReadyToExecute> {
    match execution_result {
        ExecutedEnvironment::SuccessExecution(success_execution) => match expectation {
            ExecutionExpectation::Success => {
                let ext = make_ext(dispatch_kind, actor_id, code, payload);

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
        ExecutedEnvironment::FailedExecution(failed_execution) => match expectation {
            ExecutionExpectation::Success => {
                panic!(
                    "Should succeed to execute WASM module, config name: {}",
                    config_name
                );
            }
            ExecutionExpectation::Failure => {
                let ext = make_ext(dispatch_kind, actor_id, code, payload);

                let success_execution =
                    failed_execution.revert(memory_dumper).unwrap_or_else(|_| {
                        panic!("Failed to set memory, config name: {}", config_name)
                    });

                success_execution
                    .set_ext(ext)
                    .unwrap_or_else(|_| panic!("Failed to set ext, config name: {}", config_name))
            }
        },
    }
}

fn make_ext(dispatch_kind: DispatchKind, actor_id: ActorId, code: &Code, payload: Payload) -> Ext {
    let incoming_message = IncomingMessage::new(
        0.into(),
        message_sender(),
        payload,
        Default::default(),
        Default::default(),
        Default::default(),
    );

    let message_context = MessageContext::new(
        IncomingDispatch::new(dispatch_kind, incoming_message, None),
        actor_id,
        ContextSettings::with_outgoing_limits(1024, u32::MAX),
    );

    let processor_context = ProcessorContext {
        message_context,
        program_id: actor_id,
        value_counter: ValueCounter::new(250_000_000_000),
        gas_counter: GasCounter::new(250_000_000_000),
        gas_allowance_counter: GasAllowanceCounter::new(250_000_000_000),
        allocations_context: AllocationsContext::try_new(
            MEMORY_SIZE.into(),
            Default::default(),
            code.metadata().static_pages(),
            code.metadata().stack_end(),
            MAX_MEMORY.into(),
        )
        .expect("Failed to create AllocationsContext"),
        ..ProcessorContext::new_mock()
    };

    Ext::new(processor_context)
}

fn message_sender() -> ActorId {
    let bytes = [1, 2, 3, 4].repeat(8);
    ActorId::try_from(bytes.as_ref()).unwrap()
}
