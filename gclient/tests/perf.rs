use gclient::{GearApi, Result};
use gear_core::ids::ProgramId;
use parity_scale_codec::Encode;
use std::collections::HashMap;
use tokio::time::{self, Duration};

const GEAR_PATH: &str = "../target/release/gear";
const WASM_PATH: &str = "../target/wasm32-unknown-unknown/release";

async fn upload_programs(api: &GearApi) -> Result<HashMap<&str, ProgramId>> {
    let init_payloads = vec![
        ("demo_block_info", vec![]),
        ("demo_capacitor", 2_000_000.to_string().into()),
        ("demo_collector", vec![]),
        ("demo_decoder", vec![]),
        ("demo_fib", vec![]),
        ("guestbook", vec![]),
        ("demo_minimal", vec![]),
        ("demo_multiping", vec![]),
        ("demo_piggy_bank", vec![]),
        ("demo_ping", vec![]),
        ("demo_ping_gas", vec![]),
        ("demo_program_id", vec![]),
        ("demo_state_rollback", vec![]),
        (
            "demo_sum",
            b"d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d".to_vec(),
        ),
        ("demo_vec", vec![]),
        ("demo_async_tester", vec![]),
        ("demo_btree", vec![]),
        ("demo_calc_hash_in_one_block", vec![]),
        ("demo_distributor", vec![]),
        ("demo_exit_handle", vec![]),
        ("demo_exit_handle_sender", vec![]),
        ("demo_gas_burned", vec![]),
        ("demo_gasless_wasting", vec![]),
        (
            "demo_meta",
            demo_meta::MessageInitIn {
                amount: 42,
                currency: "USD".to_string(),
            }
            .encode(),
        ),
        ("demo_mul_by_const", 42_u64.encode()),
        ("demo_ncompose", (gstd::ActorId::zero(), 42_u16).encode()),
        (
            "demo_new_meta",
            demo_meta_io::MessageInitIn {
                amount: 42,
                currency: "USD".to_string(),
            }
            .encode(),
        ),
        (
            "demo_node",
            demo_node::Initialization { status: 42 }.encode(),
        ),
        ("demo_program_factory", vec![]),
        (
            "demo_proxy",
            demo_proxy::InputArgs {
                destination: [0u8; 32],
            }
            .encode(),
        ),
        (
            "demo_proxy_relay",
            demo_proxy_relay::RelayCall::Rereply.encode(),
        ),
        (
            "demo_proxy_with_gas",
            demo_proxy_with_gas::InputArgs {
                destination: gstd::ActorId::zero(),
                delay: 10,
            }
            .encode(),
        ),
    ];

    println!("Program count: {}", init_payloads.len());

    let max_gas_limit = api.block_gas_limit()?;
    let mut progs = HashMap::new();

    for (name, init_payload) in init_payloads {
        let (_, id, _) = api
            .upload_program_bytes_by_path(
                &format!("{WASM_PATH}/{name}.opt.wasm"),
                "salt",
                init_payload,
                max_gas_limit,
                0,
            )
            .await
            .unwrap_or_else(|e| panic!("Unable to upload program '{name}': {e}"));
        progs.insert(name, id);
    }

    // Upload code for `demo_program_factory`
    api.upload_code_by_path("../examples/binaries/program-factory/child_contract.wasm")
        .await?;

    Ok(progs)
}

async fn send_messages(api: &GearApi, progs: &HashMap<&str, ProgramId>) -> Result<()> {
    let handle_payloads = vec![
        ("demo_block_info", vec![]),
        ("demo_capacitor", 1_000_000.to_string().into()),
        ("demo_collector", b"Lorem ipsum dolor sit amet".to_vec()),
        (
            "demo_decoder",
            b"1 2 3 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20".to_vec(),
        ),
        ("demo_fib", 1000.to_string().into()),
        (
            "guestbook",
            guestbook::Action::AddMessage(guestbook::MessageIn {
                author: "Gear Technologies".to_string(),
                msg: "Lorem ipsum dolor sit amet".to_string(),
            })
            .encode(),
        ),
        ("demo_minimal", vec![]),
        ("demo_multiping", b"PING".to_vec()),
        ("demo_piggy_bank", b"smash".to_vec()),
        ("demo_ping", b"PING".to_vec()),
        ("demo_ping_gas", b"PING_REPLY_COMMIT_WITH_GAS".to_vec()),
        ("demo_program_id", vec![]),
        ("demo_state_rollback", b"leave".to_vec()),
        ("demo_sum", 42.encode()),
        ("demo_vec", 65535.encode()),
        (
            "demo_async_tester",
            demo_async_tester::Kind::SendCommit.encode(),
        ),
        ("demo_btree", demo_btree::Request::Insert(42, 84).encode()),
        (
            "demo_calc_hash_in_one_block",
            demo_calc_hash_in_one_block::Package {
                expected: 6400,
                package: demo_calc_hash::Package {
                    result: [0u8; 32],
                    counter: 0,
                },
            }
            .encode(),
        ),
        (
            "demo_distributor",
            demo_distributor::Request::Report.encode(),
        ),
        ("demo_exit_handle", vec![]),
        (
            "demo_exit_handle_sender",
            demo_exit_handle_sender::Input::SendMessage {
                destination: [0u8; 32].into(),
                payload: vec![],
                value: 0,
            }
            .encode(),
        ),
        ("demo_gas_burned", vec![]),
        (
            "demo_gasless_wasting",
            demo_gasless_wasting::InputArgs {
                prog_to_wait: [0u8; 32].into(),
                prog_to_waste: [0u8; 32].into(),
            }
            .encode(),
        ),
        (
            "demo_meta",
            demo_meta::MessageIn {
                id: demo_meta::Id {
                    decimal: 1,
                    hex: vec![1u8],
                },
            }
            .encode(),
        ),
        ("demo_mul_by_const", 42_u64.encode()),
        ("demo_ncompose", vec![1, 2, 3, 4]),
        (
            "demo_new_meta",
            demo_meta_io::MessageIn {
                id: demo_meta_io::Id {
                    decimal: 1,
                    hex: vec![1u8],
                },
            }
            .encode(),
        ),
        ("demo_node", demo_node::Request::Add(42).encode()),
        (
            "demo_program_factory",
            demo_program_factory::CreateProgram::Default.encode(),
        ),
        ("demo_proxy", vec![0; 32_768]),
        ("demo_proxy_relay", vec![0; 32_768]),
        ("demo_proxy_with_gas", 1_000_000_000_u64.encode()),
    ];

    let mut block_gas_limit = api.block_gas_limit()?;
    let mut messages = Vec::with_capacity(handle_payloads.len());
    for (prog_name, payload) in handle_payloads {
        let prog_id = progs[prog_name];
        let gas_info = api
            .calculate_handle_gas(None, prog_id, payload.clone(), 0, true)
            .await?;
        block_gas_limit = block_gas_limit.saturating_sub(gas_info.burned);
        if block_gas_limit == 0 {
            break;
        }
        messages.push((prog_id, payload, gas_info.min_limit, 0));
    }

    println!("Message count: {}", messages.len());

    // TODO: unstable test #2322
    // assert_eq!(block_gas_limit, 0);

    if let Some(Err(e)) = api
        .send_message_bytes_batch(messages)
        .await?
        .0
        .into_iter()
        .find(|r| r.is_err())
    {
        return Err(e);
    }

    Ok(())
}

#[tokio::test]
async fn full_block_of_messages() -> Result<()> {
    env_logger::init();

    let api = GearApi::dev_from_path(GEAR_PATH).await?;

    let progs = upload_programs(&api).await?;

    // Wait for the next block
    time::sleep(Duration::from_secs(3)).await;

    send_messages(&api, &progs).await
}

mod guestbook {
    use super::*;

    #[allow(dead_code)]
    #[derive(Encode)]
    pub enum Action {
        AddMessage(MessageIn),
        ViewMessages,
    }

    #[derive(Encode)]
    pub struct MessageIn {
        pub author: String,
        pub msg: String,
    }
}
