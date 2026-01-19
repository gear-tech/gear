mod args;
mod batch;
mod utils;
use alloy::{
    eips::BlockId,
    primitives::Address,
    providers::{Provider, ProviderBuilder, RootProvider, fillers::FillProvider},
};
use anyhow::Result;
use args::{Params, parse_cli_params};
use ethexe_common::{Address as LocalAddress, k256::ecdsa::SigningKey};
use ethexe_ethereum::{Ethereum, router::Router};
use ethexe_observer::{EthereumConfig, ObserverService};
use ethexe_signer::{KeyStorage, MemoryKeyStorage, Signer};
use gear_core::{code::CodeAndId, ids::prelude::CodeIdExt};
use gear_wasm_gen::StandardGearWasmConfigsBundle;
use gprimitives::CodeId;
use rand::rngs::SmallRng;
use std::{str::FromStr, sync::Arc, time::Duration};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let params = parse_cli_params();

    match params {
        Params::Dump { seed } => {
            info!("Dump requested with seed: {}", seed);
            // Dump logic would go here
        }
        Params::Load(load_params) => {
            info!("Starting load test on {}", load_params.node);

            /*// 1. Initialize Signer
            // Using memory signer for example, in real usage might need key loading
            let signer = Signer::memory();
            // TODO: In real usage, we need to fund this account or use a funded one.
            // For now, we assume the user/root params might provide a key,
            // but for this skeleton we just generate a random one to satisfby the API check.
            let key = signer.storage_mut().generate_key()?;
            let sender_address = key.to_address();

            info!("Using temporary sender address: {:?}", sender_address);

            // 2. Create Provider
            let provider = Arc::new(api::create_provider(&load_params.node, signer, sender_address).await?);

            // 3. Create Generator
            let seed = load_params.loader_seed.unwrap_or(0);
            let generator = generators::BatchGenerator::new(
                provider.clone(),
                api::Signer::address(&crate::api::Sender::new(Signer::memory(), sender_address).unwrap()), /* This is a bit hacky for the skeleton, fixing in real impl */
                &load_params,
                seed
            );

            // 4. Create and Run Pool
            let (stop_tx, stop_rx) = tokio::sync::broadcast::channel(1);
            let pool = batch_pool::BatchPool::new(provider, load_params, generator);

            // Handle Ctrl+C
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.ok();
                stop_tx.send(()).ok();
            });

            pool.run(stop_rx).await?;*/

            let signing_key = alloy::hex::decode(&load_params.sender_private_key).unwrap();
            /*(let provider: FillProvider<_, RootProvider<Ethereum>> = ProviderBuilder::default()
                .wallet(EthereumWallet::new(LocalSigner::from_signing_key(
                    SigningKey::from_slice(signing_key.as_ref()).expect("Invalid signing key"),
                )))
                .connect(&load_params.node)
                .await?;
            println!("Connected to Anvil");

            let router = Router::*/
            /*
            let address = alloy::hex::decode(&load_params.sender_address).unwrap();*/

            let mut keystore = MemoryKeyStorage::empty();
            let signing_key =
                SigningKey::from_slice(signing_key.as_ref()).expect("Invalid signing key");
            let pubkey = keystore.add_key(signing_key.into()).unwrap();
            let signer = ethexe_signer::Signer::new(keystore);
            let router_addr = Address::from_str(&load_params.router_address).unwrap();

            let ethereum_cfg = EthereumConfig {
                rpc: load_params.node.clone(),
                beacon_rpc: load_params.node.clone(),
                router_address: router_addr.into(),
                block_time: Duration::from_secs(12),
            };

            let observer = ObserverService::new(&ethereum_cfg, 12, ethexe_db::Database::memory())
                .await
                .unwrap();

            let eth =
                Ethereum::new(&load_params.node, router_addr, signer, pubkey.to_address()).await?;

            let code = gear_call_gen::generate_gear_program::<
                SmallRng,
                StandardGearWasmConfigsBundle,
            >(42, StandardGearWasmConfigsBundle::default());
            let code_id = CodeId::generate(code.as_ref());
            let (tx, _) = eth
                .router()
                .request_code_validation_with_sidecar(code.as_ref())
                .await?
                .send()
                .await?;

            println!("Code ID: {code_id}, tx hash: {tx:?}");
            std::thread::sleep(Duration::from_secs(12));
            let latest_block = eth.provider().get_block_number().await.unwrap();
            let ids = eth
                .router()
                .query()
                .codes_states_at([code_id], BlockId::latest())
                .await
                .unwrap();
            println!("{ids:?}");
        }
    }

    Ok(())
}
/*
#[derive(Clone)]
pub struct WaitForUploadCode {
    receiver: ObserverEventReceiver,
    pub code_id: CodeId,
}

#[derive(Debug)]
pub struct UploadCodeInfo {
    pub code_id: CodeId,
    pub valid: bool,
}

impl WaitForUploadCode {
    pub async fn wait_for(self) -> anyhow::Result<UploadCodeInfo> {
        log::info!("📗 Waiting for code upload, code_id {}", self.code_id);

        let valid = self
            .receiver
            .filter_map_block_synced()
            .find_map(|event| match event {
                BlockEvent::Router(RouterEvent::CodeGotValidated { code_id, valid })
                    if code_id == self.code_id =>
                {
                    Some(valid)
                }
                _ => None,
            })
            .await;

        Ok(UploadCodeInfo {
            code_id: self.code_id,
            valid,
        })
    }
}
*/
