//! Gear node loader.
//!
//! This tool sends semi-random data to the gear node with one main purpose - crash it.
//! The sent data is not completely random as it is usually in fuzz-kind tests. The tool
//! gets properly structured data acceptable by the gear node and randomizes it's "fields".
//! That's why generated data is called semi-random.

use std::{fs::File, io::Write, sync::Arc};

use arbitrary::Unstructured;
use gear_program::{
    api::{generated::api::gear::calls::UploadProgram, Api},
    result::Result,
};
use gear_wasm_gen::GearConfig;
use parking_lot::Mutex;
use rand::{rngs::SmallRng, RngCore, SeedableRng};

use args::{parse_cli_params, Params};
use reporter::Reporter;
use task::TaskPool;

mod args;
mod reporter;
mod task;

#[tokio::main]
async fn main() -> Result<()> {
    let params = parse_cli_params();

    if let Some(seed) = params.dump_seed {
        let code = gen_code_for_seed(seed);
        let mut file = File::create("out.wasm").expect("Cannot create out.wasm file");
        file.write_all(&code).expect("Cannot write bytes into file");
        return Ok(());
    }

    load_node(params).await;

    Ok(())
}

async fn load_node(params: Params) {
    gear_program::keystore::login(&params.user, None).unwrap();

    let params = Arc::new(params);
    let gear_api = gear_program::api::Api::new(Some(&params.endpoint))
        .await
        .unwrap();
    let salt = Arc::new(Mutex::new(0));
    let seed_gen = Arc::new(Mutex::new(SmallRng::seed_from_u64(params.seed)));

    let mut task_pool = TaskPool::new(&params);
    loop {
        let reporters = task_pool
            .run(|| {
                load_node_task(
                    gear_api.clone(),
                    Arc::clone(&salt),
                    Arc::clone(&seed_gen),
                    Arc::clone(&params),
                )
            })
            .await
            .expect("tmp");
        reporters.into_iter().for_each(Reporter::report);
    }
}

async fn load_node_task(
    gear_api: Api,
    salt: Arc<Mutex<u32>>,
    seed_gen: Arc<Mutex<SmallRng>>,
    params: Arc<Params>,
) -> Reporter {
    let signer = gear_api.try_signer(None).unwrap();
    let (seed, salt) = if let Some(seed) = params.only_seed {
        *salt.lock() += 1;
        (seed, *salt.lock())
    } else {
        (seed_gen.lock().next_u64(), 0)
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
