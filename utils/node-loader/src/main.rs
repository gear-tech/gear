//! Gear node loader.
//!
//! This tool sends semi-random data to the gear node with one main purpose - crash it.
//! The sent data is not completely random as it is usually in fuzz-kind tests. The tool
//! gets properly structured data acceptable by the gear node and randomizes it's "fields".
//! That's why generated data is called semi-random.

use std::{fs::File, io::Write, sync::Arc};

use arbitrary::Unstructured;
use futures::future::join_all;
use gear_program::{
    api::{generated::api::gear::calls::UploadProgram, Api},
    result::Result,
};
use gear_wasm_gen::GearConfig;
use parking_lot::Mutex;
use rand::{rngs::SmallRng, RngCore, SeedableRng};

use args::{parse_cli_params, Params};

mod args;

const MAX_TASKS: usize = 100;

#[tokio::main]
async fn main() -> Result<()> {
    let params = parse_cli_params();

    if let Some(seed) = params.dump_seed {
        let code = gen_code_for_seed(seed);
        let mut file = File::create("out.wasm").expect("Cannot create out.wasm file");
        file.write(&code).expect("Cannot write bytes into file");
        return Ok(());
    }

    load_node(params).await;

    Ok(())
}

async fn load_node(params: Params) {
    gear_program::keystore::login(&params.user, None).unwrap();

    let gear_api = gear_program::api::Api::new(Some(&params.endpoint))
        .await
        .unwrap();
    let salt = Arc::new(Mutex::new(0));
    let seed_gen = Arc::new(Mutex::new(SmallRng::seed_from_u64(params.seed)));

    let mut tasks = Vec::with_capacity(MAX_TASKS);
    loop {
        tasks.clear();
        while tasks.len() != MAX_TASKS {
            let task = load_node_task(
                gear_api.clone(),
                Arc::clone(&salt),
                Arc::clone(&seed_gen),
                &params,
            );
            tasks.push(Box::pin(task));
        }
        join_all(&mut tasks).await;
    }
}

async fn load_node_task(
    gear_api: Api,
    salt: Arc<Mutex<u32>>,
    seed_gen: Arc<Mutex<SmallRng>>,
    params: &Params,
) {
    let signer = gear_api.try_signer(None).unwrap();

    println!("==============================================");

    let (seed, salt) = if let Some(seed) = params.only_seed {
        *salt.lock() += 1;
        (seed, *salt.lock())
    } else {
        (seed_gen.lock().next_u64(), 0)
    };
    let salt = format!("{:02}", salt);
    println!("Run with seed = {}, salt = {}", seed, salt);

    let code = gen_code_for_seed(seed);
    println!("Gen code size = {}", code.len());

    let params = UploadProgram {
        code: code.clone(),
        salt: hex::decode(salt.as_str()).unwrap(),
        init_payload: hex::decode("00").unwrap(),
        gas_limit: 250_000_000_000,
        value: 0,
    };

    let _res = signer
        .submit_program(params)
        .await
        .map_err(|err| {
            println!("ERROR: {}", err);
            err
        })
        .map(|res| {
            println!("Successfully receive response");
            res
        });
}

fn gen_code_for_seed(seed: u64) -> Vec<u8> {
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut buf = vec![0; 100_000];
    rng.fill_bytes(&mut buf);
    let mut u = Unstructured::new(&buf);
    gear_wasm_gen::gen_gear_program_code(&mut u, GearConfig::default())
}
