use crate::{ProcessorContext, ProcessorExternalities};
use alloc::vec;
use gear_core::{
    code::Code,
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    gas_metering::CustomConstantCostRules,
    ids::{prelude::*, CodeId, ProgramId},
    memory::Memory,
    message::{
        ContextSettings, DispatchKind, IncomingDispatch, IncomingMessage, MessageContext, Payload,
    },
};
use gear_core_backend::env::Environment;
use gear_lazy_pages::LazyPagesVersion;
use gear_lazy_pages_common::LazyPagesInitContext;
use gear_lazy_pages_native_interface::LazyPagesNative;
use parity_scale_codec::Encode;

type Ext = crate::Ext<LazyPagesNative>;

#[test]
fn execute_environment_multiple_times() {
    use demo_fungible_token::{FTAction, InitConfig, WASM_BINARY};
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

    let payload =
        Payload::try_from(InitConfig::test_sequence().encode()).expect("Failed to encode");

    let ext = make_ext(program_id, payload);

    let env = Environment::new(
        ext,
        code.code(),
        vec![DispatchKind::Init, DispatchKind::Handle]
            .into_iter()
            .collect(),
        512.into(),
    )
    .expect("Failed to create environment");

    let execution_result = env
        .execute(DispatchKind::Init, |ctx, mem, globals_config| {
            Ext::lazy_pages_init_for_program(
                ctx,
                mem,
                program_id,
                Default::default(),
                Some(
                    mem.size(ctx)
                        .to_page_number()
                        .expect("Memory size is 4GB, so cannot be stack end"),
                ),
                globals_config,
                Default::default(),
            );
        })
        .expect("Failed to execute WASM module");

    let env = match execution_result {
        Ok(success_execution) => {
            // Mint 1_000_000 tokens to main user
            let payload =
                Payload::try_from(FTAction::Mint(1_000_000).encode()).expect("Failed to encode");

            let ext = make_ext(program_id, payload);

            let env = success_execution.set_ext(ext).expect("Failed to set ext");

            env
        }
        Err(_failed_execution) => {
            panic!("Failed to execute WASM module");
        }
    };

    let execution_result = env
        .execute(DispatchKind::Handle, |_, _, _| {})
        .expect("Failed to execute WASM module");

    let env = match execution_result {
        Ok(success_execution) => {
            // Try to burn more tokens than available
            let payload =
                Payload::try_from(FTAction::Burn(2_000_000).encode()).expect("Failed to encode");

            let ext = make_ext(program_id, payload);

            let env = success_execution.set_ext(ext).expect("Failed to set ext");

            env
        }
        Err(_failed_execution) => {
            panic!("Failed to execute WASM module");
        }
    };

    let execution_result = env
        .execute(DispatchKind::Handle, |_, _, _| {})
        .expect("Failed to execute WASM module");

    let env = match execution_result {
        Ok(_success_execution) => {
            panic!("Should fail to burn more tokens than available");
        }
        Err(failed_execution) => {
            // Mint 3_000_000 tokens to main user
            let payload =
                Payload::try_from(FTAction::Mint(3_000_000).encode()).expect("Failed to encode");

            let ext = make_ext(program_id, payload);

            failed_execution
                .clear_memory(ext, 512.into())
                .expect("Failed to clear memory")
        }
    };

    let execution_result = env
        .execute(DispatchKind::Handle, |ctx, mem, globals_config| {
            Ext::lazy_pages_init_for_program(
                ctx,
                mem,
                program_id,
                Default::default(),
                Some(
                    mem.size(ctx)
                        .to_page_number()
                        .expect("Memory size is 4GB, so cannot be stack end"),
                ),
                globals_config,
                Default::default(),
            );
        })
        .expect("Failed to execute WASM module");

    let env = match execution_result {
        Ok(success_execution) => {
            // Burn 1_000_000 tokens
            let payload =
                Payload::try_from(FTAction::Burn(1_000_000).encode()).expect("Failed to encode");

            let ext = make_ext(program_id, payload);

            let env = success_execution.set_ext(ext).expect("Failed to set ext");

            env
        }
        Err(_failed_execution) => {
            panic!("Failed to execute WASM module");
        }
    };

    let execution_result = env
        .execute(DispatchKind::Handle, |_, _, _| {})
        .expect("Failed to execute WASM module");

    let env = match execution_result {
        Ok(success_execution) => {
            let payload =
                Payload::try_from(FTAction::TotalSupply.encode()).expect("Failed to encode");

            let ext = make_ext(program_id, payload);

            let env = success_execution.set_ext(ext).expect("Failed to set ext");

            env
        }
        Err(_failed_execution) => {
            panic!("Failed to execute WASM module");
        }
    };

    let execution_result = env
        .execute(DispatchKind::Handle, |_, _, _| {})
        .expect("Failed to execute WASM module");

    execution_result.expect("Failed to execute WASM module");
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

fn message_sender() -> ProgramId {
    let bytes = [1, 2, 3, 4].repeat(8);
    ProgramId::try_from(bytes.as_ref()).unwrap()
}
