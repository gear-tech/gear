use arbitrary::Unstructured;
use gear_program::{api::generated::api::gear::calls::UploadProgram, result::Result};
use gear_wasm_gen::GearConfig;
use rand::{rngs::SmallRng, RngCore, SeedableRng};
use structopt::StructOpt;
use std::fs::File;
use std::io::Write;

#[derive(Debug, StructOpt)]
#[structopt(name = "node-hacker")]
pub struct Params {
    /// rpc node addr
    #[structopt(long, default_value = "ws://localhost:9944")]
    pub endpoint: String,

    /// user name
    #[structopt(long, default_value = "//Alice")]
    pub user: String,

    /// seed for random seeds generation
    #[structopt(long, short, default_value = "0")]
    pub seed: u64,

    /// dump wasm into "out.wasm" for seed and stop work
    #[structopt(long)]
    pub dump_seed: Option<u64>,

    /// generate program for seed and test it in inf loop
    #[structopt(long)]
    pub only_seed: Option<u64>,
}

fn gen_code_for_seed(seed: u64) -> Vec<u8> {
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut buf = vec![0; 100_000];
    rng.fill_bytes(&mut buf);
    let mut u = Unstructured::new(&buf);
    gear_wasm_gen::gen_gear_program_code(&mut u, GearConfig::default())
}

#[tokio::main]
async fn main() -> Result<()> {
    let params = Params::from_args();

    if let Some(seed) = params.dump_seed {
        let code = gen_code_for_seed(seed);
        let mut file = File::create("out.wasm").expect("Cannot create out.wasm file");
        file.write(&code).expect("Cannot write bytes into file");
        return Ok(());
    }

    gear_program::keystore::login(&params.user, None).unwrap();
    let signer = gear_program::api::Api::new(Some(&params.endpoint))
        .await
        .unwrap()
        .try_signer(None)
        .unwrap();

    let mut salt: u32 = 0;
    let mut seed_gen = SmallRng::seed_from_u64(params.seed);
    loop {
        println!("==============================================");

        let (seed, salt) = if let Some(seed) = params.only_seed {
            salt += 1;
            (seed, salt)
        } else {
            (seed_gen.next_u64(), 0)
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

        let _res = signer.submit_program(params).await.map_err(|err| {
            println!("ERROR: {}", err);
            err
        }).map(|res| {
            println!("Successfully receive response");
            res
        });
    }
}
