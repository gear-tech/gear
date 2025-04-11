use anyhow::{anyhow, Context};
use futures::{StreamExt, TryStreamExt};
use gcli::cmd::config::Network;
use gear_core::{
    code::{Code, CodeError},
    gas_metering::Schedule,
};
use gprimitives::CodeId;
use gsdk::{metadata::storage::GearProgramStorage, Api};
use std::future;
use tokio::task;
use tracing::Level;
use tracing_subscriber::EnvFilter;

struct InstrumentationResult {
    code_id: CodeId,
    res: Result<Code, CodeError>,
}

impl InstrumentationResult {
    fn ok(&self) -> Option<CodeId> {
        if let Ok(_code) = &self.res {
            Some(self.code_id)
        } else {
            None
        }
    }

    fn into_err(self) -> Option<(CodeId, CodeError)> {
        if let Err(err) = self.res {
            Some((self.code_id, err))
        } else {
            None
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(Level::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let api = Api::new(Some(Network::Testnet.as_ref()))
        .await
        .context("Failed to create client")?;

    let latest_block = api
        .blocks()
        .at_latest()
        .await
        .context("Failed to get the latest block")?;
    let latest_block = latest_block.hash();

    let storage_key = Api::storage_root(GearProgramStorage::OriginalCodeStorage).to_root_bytes();
    let storage_key_len = storage_key.len();
    let storage = api.storage().at(latest_block);

    let keys = storage
        .fetch_raw_keys(storage_key)
        .await
        .context("Failed to obtain stream of keys")?;
    let failed_codes = keys
        .map(|key| {
            let api = api.clone();
            async move {
                let key = key.context("Failed to get key")?;
                let code_id = CodeId::try_from(&key[storage_key_len..])
                    .map_err(|e| anyhow!("Failed to parse key: {e}"))?;

                let fetch_res = tokio::spawn(async move {
                    api.original_code_storage_at(code_id, latest_block).await
                })
                .await
                .context("Function inside `tokio::spawn()` panicked")?;
                let original_code = fetch_res
                    .with_context(|| format!("Failed to fetch original code {code_id}"))?;

                let parse_res = task::spawn_blocking(move || {
                    let schedule = Schedule::default();
                    Code::try_new(
                        original_code,
                        schedule.instruction_weights.version,
                        |module| schedule.rules(module),
                        schedule.limits.stack_height,
                        schedule.limits.data_segments_amount.into(),
                    )
                })
                .await
                .context("`gear-wasm-instrument` panicked")?;

                anyhow::Ok(InstrumentationResult {
                    code_id,
                    res: parse_res,
                })
            }
        })
        .buffer_unordered(16)
        .inspect_ok(|res| {
            if let Some(code_id) = res.ok() {
                tracing::info!("Code {code_id} succeed")
            }
        })
        .try_filter_map(|res| future::ready(Ok(res.into_err())))
        .try_collect::<Vec<_>>()
        .await?;

    Ok(())
}
