//! Gear node loader.
//!
//! This tool sends semi-random data to the gear node with one main purpose - crash it.
//! The sent data is not completely random as it is usually in fuzz-kind tests. The tool
//! gets properly structured data acceptable by the gear node and randomizes it's "fields".
//! That's why generated data is called semi-random.

use std::{fs::File, io::Write, sync::Arc};

use arbitrary::Unstructured;
use gear_program::api::{generated::api::gear::calls::UploadProgram, Api};
use gear_wasm_gen::GearConfig;
use parking_lot::Mutex;
use rand::{rngs::SmallRng, RngCore, SeedableRng};

use args::{parse_cli_params, LoadParams, Params, SeedVariant};
use reporter::Reporter;
use task::TaskPool;

mod args;
mod reporter;
mod task;

/// Main entry-point
#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{e:}");
    }
}

async fn run() -> Result<(), String> {
    let params = parse_cli_params();

    match params {
        Params::Dump { seed } => dump_with_seed(seed),
        Params::Load(load_params) => load_node(load_params).await,
    }
}

// todo use anyhow?
fn dump_with_seed(seed: u64) -> Result<(), String> {
    let code = gen_code_for_seed(seed);
    let mut file = File::create("out.wasm").map_err(|e| e.to_string())?;
    file.write_all(&code).map_err(|e| e.to_string())?;
    Ok(())
}

async fn load_node(params: LoadParams) -> Result<(), String> {
    gear_program::keystore::login(&params.user, None).unwrap();

    let params = Arc::new(params);
    let gear_api = gear_program::api::Api::new(Some(&params.endpoint))
        .await
        .unwrap();
    let salt = Arc::new(Mutex::new(0));

    let mut task_pool = TaskPool::try_new(params.threads)?;
    loop {
        let reporters = task_pool
            .run(|| load_node_task(gear_api.clone(), Arc::clone(&salt), Arc::clone(&params)))
            .await
            .expect("tmp");
        reporters.into_iter().for_each(Reporter::report);
    }

    Ok(())
}

async fn load_node_task(gear_api: Api, salt: Arc<Mutex<u32>>, params: Arc<LoadParams>) -> Reporter {
    let signer = gear_api.try_signer(None).unwrap();
    let (seed, salt) = match params.seed {
        SeedVariant::Start(seed) => (SmallRng::seed_from_u64(seed).next_u64(), 0),
        SeedVariant::Constant(num) => {
            *salt.lock() += 1;
            (num, *salt.lock())
        }
    };

    let mut reporter = Reporter::new(seed);
    let salt = format!("{:02}", salt);
    reporter.record(format!("Run with salt = {}", salt));

    let code = gen_code_for_seed(seed);
    reporter.record(format!("Gen code size = {}", code.len()));

    let params = UploadProgram {
        code: code.clone(),
        salt: hex::decode(salt.as_str()).unwrap(),
        init_payload: hex::decode("00").unwrap(),
        gas_limit: 250_000_000_000,
        value: 0,
    };

    if let Err(e) = signer.submit_program(params).await {
        reporter.record(format!("ERROR: {}", e));
    } else {
        reporter.record("Successfully receive response");
    }

    reporter
}

fn gen_code_for_seed(seed: u64) -> Vec<u8> {
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut buf = vec![0; 100_000];
    rng.fill_bytes(&mut buf);
    let mut u = Unstructured::new(&buf);
    gear_wasm_gen::gen_gear_program_code(&mut u, GearConfig::default())
}
