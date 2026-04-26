// Pre-defined scenarios. Each function loads a wasm, deploys it (with any
// required mock counter-parties), drives a representative message flow, and
// returns aggregated gas burn.

use gtest::{Program, System, WasmProgram};
use parity_scale_codec::{Decode, Encode};
use std::{fs, path::Path};

const USER_ID: u64 = 42;
const PROGRAM_ID: u64 = 100;
const REPLIER_ID: u64 = 200;

const BASE_BALANCE: u128 = 100_000_000_000_000;

pub struct ScenarioResult {
    pub name: &'static str,
    pub wasm: std::path::PathBuf,
    pub messages: usize,
    pub total_gas: u64,
    pub per_message: Vec<u64>,
}

// ----- shared helpers ---------------------------------------------------

/// gear-core `ActorId` SCALE-encodes as 32 raw bytes; `From<u64>` places the
/// little-endian u64 in the first 8 bytes.
fn actor_id_bytes_from_u64(id: u64) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(&id.to_le_bytes());
    buf
}

fn drain_blocks(sys: &System) -> (usize, u64, Vec<u64>) {
    let mut messages = 0;
    let mut total = 0u64;
    let mut per_msg = Vec::new();
    loop {
        let res = sys.run_next_block();
        for (_id, gas) in &res.gas_burned {
            total = total.saturating_add(*gas);
            per_msg.push(*gas);
            messages += 1;
        }
        if res.total_processed == 0 {
            break;
        }
    }
    (messages, total, per_msg)
}

#[derive(Debug, Clone)]
struct ConstReplier {
    payload: &'static [u8],
}

impl WasmProgram for ConstReplier {
    fn init(&mut self, _payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str> {
        Ok(None)
    }
    fn handle(&mut self, _payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str> {
        Ok(Some(self.payload.to_vec()))
    }
    fn clone_boxed(&self) -> Box<dyn WasmProgram> {
        Box::new(self.clone())
    }
    fn state(&mut self) -> Result<Vec<u8>, &'static str> {
        Ok(vec![])
    }
}

// ----- demo-async Command shape (kept in sync with examples/async/src/lib.rs)

#[derive(Encode, Decode)]
enum AsyncCommand {
    Common,
    Mutex,
}

// ----- scenarios ---------------------------------------------------------

pub fn async_common(wasm: &Path) -> ScenarioResult {
    run_async(wasm, "async-common", AsyncCommand::Common)
}

pub fn async_mutex(wasm: &Path) -> ScenarioResult {
    run_async(wasm, "async-mutex", AsyncCommand::Mutex)
}

fn run_async(wasm: &Path, name: &'static str, cmd: AsyncCommand) -> ScenarioResult {
    let sys = System::new();
    sys.init_logger();
    sys.mint_to(USER_ID, BASE_BALANCE);

    // Mock destination that always replies "PONG" (matches demo-async expectation).
    let _replier = Program::mock_with_id(
        &sys,
        REPLIER_ID,
        ConstReplier { payload: b"PONG" },
    );

    let prog = Program::from_binary_with_id(&sys, PROGRAM_ID, fs::read(wasm).unwrap());
    let init_payload = actor_id_bytes_from_u64(REPLIER_ID).to_vec();
    let init_msg = prog.send_bytes(USER_ID, init_payload);
    let init_res = sys.run_next_block();
    assert!(
        init_res.succeed.contains(&init_msg),
        "init failed: {:?}",
        init_res.failed
    );

    let _handle_msg = prog.send_bytes(USER_ID, cmd.encode());
    let (messages, total_gas, per_message) = drain_blocks(&sys);

    ScenarioResult {
        name,
        wasm: wasm.to_path_buf(),
        messages,
        total_gas,
        per_message,
    }
}

pub fn sync_ping(wasm: &Path) -> ScenarioResult {
    let sys = System::new();
    sys.init_logger();
    sys.mint_to(USER_ID, BASE_BALANCE);

    let prog = Program::from_binary_with_id(&sys, PROGRAM_ID, fs::read(wasm).unwrap());

    // demo-ping init expects b"PING" and conditionally replies; deploy with PING.
    let init_msg = prog.send_bytes(USER_ID, b"PING".to_vec());
    let init_res = sys.run_next_block();
    assert!(
        init_res.succeed.contains(&init_msg),
        "init failed: {:?}",
        init_res.failed
    );

    let _handle_msg = prog.send_bytes(USER_ID, b"PING".to_vec());
    let (messages, total_gas, per_message) = drain_blocks(&sys);

    ScenarioResult {
        name: "sync-ping",
        wasm: wasm.to_path_buf(),
        messages,
        total_gas,
        per_message,
    }
}
