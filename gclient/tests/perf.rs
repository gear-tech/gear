// TODO: issue #2628
// use gclient::{GearApi, Result};
// use gear_core::ids::ProgramId;
// use parity_scale_codec::Encode;
// use std::collections::HashMap;
// use tokio::time::{self, Duration};

// const GEAR_PATH: &str = "../target/release/gear";
// const WASM_PATH: &str = "../target/wasm32-unknown-unknown/release";

// async fn upload_programs(api: &GearApi) -> Result<HashMap<&str, ProgramId>> {
//     let init_payloads = vec![
//         ("demo_async_tester", vec![]),
//         ("demo_btree", vec![]),
//         ("demo_calc_hash_in_one_block", vec![]),
//         ("demo_distributor", vec![]),
//         ("demo_exit_handle_sender", vec![]),
//         ("demo_gasless_wasting", vec![]),
//         ("demo_mul_by_const", 42_u64.encode()),
//         (
//             "demo_node",
//             demo_node::Initialization { status: 42 }.encode(),
//         ),
//         ("demo_program_factory", vec![]),
//         (
//             "demo_proxy",
//             demo_proxy::InputArgs {
//                 destination: [0u8; 32],
//             }
//             .encode(),
//         ),
//         (
//             "demo_proxy_relay",
//             demo_proxy_relay::RelayCall::Rereply.encode(),
//         ),
//         (
//             "demo_proxy_with_gas",
//             demo_proxy_with_gas::InputArgs {
//                 destination: gstd::ActorId::zero(),
//                 delay: 10,
//             }
//             .encode(),
//         ),
//     ];

//     println!("Program count: {}", init_payloads.len());

//     let max_gas_limit = api.block_gas_limit()?;
//     let mut progs = HashMap::new();

//     for (name, init_payload) in init_payloads {
//         let (_, id, _) = api
//             .upload_program_bytes_by_path(
//                 &format!("{WASM_PATH}/{name}.opt.wasm"),
//                 "salt",
//                 init_payload,
//                 max_gas_limit,
//                 0,
//             )
//             .await
//             .unwrap_or_else(|e| panic!("Unable to upload program '{name}': {e}"));
//         progs.insert(name, id);
//     }

//     // Upload code for `demo_program_factory`
//     api.upload_code_by_path("../examples/program-factory/child_contract.wasm")
//         .await?;

//     Ok(progs)
// }

// async fn send_messages(api: &GearApi, progs: &HashMap<&str, ProgramId>) -> Result<()> {
//     let handle_payloads = vec![
//         (
//             "demo_async_tester",
//             demo_async_tester::Kind::SendCommit.encode(),
//         ),
//         ("demo_btree", demo_btree::Request::Insert(42, 84).encode()),
//         (
//             "demo_calc_hash_in_one_block",
//             demo_calc_hash_in_one_block::Package {
//                 expected: 6400,
//                 package: demo_calc_hash::Package {
//                     result: [0u8; 32],
//                     counter: 0,
//                 },
//             }
//             .encode(),
//         ),
//         (
//             "demo_distributor",
//             demo_distributor::Request::Report.encode(),
//         ),
//         (
//             "demo_exit_handle_sender",
//             demo_exit_handle_sender::Input::SendMessage {
//                 destination: [0u8; 32].into(),
//                 payload: vec![],
//                 value: 0,
//             }
//             .encode(),
//         ),
//         (
//             "demo_gasless_wasting",
//             demo_gasless_wasting::InputArgs {
//                 prog_to_wait: [0u8; 32].into(),
//                 prog_to_waste: [0u8; 32].into(),
//             }
//             .encode(),
//         ),
//         ("demo_mul_by_const", 42_u64.encode()),
//         ("demo_node", demo_node::Request::Add(42).encode()),
//         (
//             "demo_program_factory",
//             demo_program_factory::CreateProgram::Default.encode(),
//         ),
//         ("demo_proxy", vec![0; 32_768]),
//         ("demo_proxy_relay", vec![0; 32_768]),
//         ("demo_proxy_with_gas", 1_000_000_000_u64.encode()),
//     ];

//     let mut block_gas_limit = api.block_gas_limit()?;
//     let mut messages = Vec::with_capacity(handle_payloads.len());
//     for (prog_name, payload) in handle_payloads {
//         let prog_id = progs[prog_name];
//         let gas_info = api
//             .calculate_handle_gas(None, prog_id, payload.clone(), 0, true)
//             .await?;
//         block_gas_limit = block_gas_limit.saturating_sub(gas_info.burned);
//         if block_gas_limit == 0 {
//             break;
//         }
//         messages.push((prog_id, payload, gas_info.min_limit, 0));
//     }

//     println!("Message count: {}", messages.len());

//     // TODO: unstable test #2322
//     // assert_eq!(block_gas_limit, 0);

//     if let Some(Err(e)) = api
//         .send_message_bytes_batch(messages)
//         .await?
//         .0
//         .into_iter()
//         .find(|r| r.is_err())
//     {
//         return Err(e);
//     }

//     Ok(())
// }

// #[tokio::test]
// async fn full_block_of_messages() -> Result<()> {
//     env_logger::init();

//     let api = GearApi::dev_from_path(GEAR_PATH).await?;

//     let progs = upload_programs(&api).await?;

//     // Wait for the next block
//     time::sleep(Duration::from_secs(3)).await;

//     send_messages(&api, &progs).await
// }


// crate gear_protocol_testing

trait GearProtocol: pallet_gear::Config + pallet_gear_debug::Config {
    fn last_program_id() -> ProgramId;
    fn message_result(message_id: MessageId) -> ExecutionResult;
    fn send_message(from: AccountId, to: ProgramId, .. ) -> Result<MessageId>;
    // .. //
}
