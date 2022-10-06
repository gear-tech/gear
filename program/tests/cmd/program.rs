//! Integration tests for command `program`
use crate::common::{self, env, logs, traits::Convert, Result};
use parity_scale_codec::Encode;

const META_STATE_WITH_NONE_INPUT: &str = "0x08010000000000000004012c536f6d655375726e616d6520536f6d654e616d6502000000000000000402244f746865724e616d65304f746865725375726e616d65";

#[derive(Encode)]
struct MessageInitIn {
    amount: u8,
    currency: String,
}

#[tokio::test]
async fn test_command_state_works() -> Result<()> {
    common::login_as_alice().expect("login failed");

    // setup node
    let mut node = common::Node::dev()?;
    node.wait(logs::gear_node::IMPORTING_BLOCKS)?;

    // get demo meta
    let opt = env::wasm_bin("demo_meta.opt.wasm");
    let meta = env::wasm_bin("demo_meta.meta.wasm");

    // Deploy demo_meta
    let deploy = common::gear(&[
        "-e",
        &node.ws(),
        "upload-program",
        &opt,
        "",
        &hex::encode(
            MessageInitIn {
                amount: 42,
                currency: "GEAR".into(),
            }
            .encode(),
        ),
        "20000000000",
    ])?;

    assert!(deploy
        .stderr
        .convert()
        .contains(logs::gear_program::EX_UPLOAD_PROGRAM));

    // Get program id
    let pid = common::program_id(demo_meta::WASM_BINARY, &[]);

    // Query state of demo_meta
    let state = common::gear(&[
        "-e",
        &node.ws(),
        "program",
        &hex::encode(pid),
        "state",
        &meta,
        "--msg",
        "0x00", // None
    ])?;

    assert!(state.stdout.convert().contains(META_STATE_WITH_NONE_INPUT));
    Ok(())
}
