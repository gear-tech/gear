use anyhow::{anyhow, Context};
use futures::{stream, StreamExt, TryStreamExt};
use gcli::cmd::config::Network;
use gear_core::{
    code::{Code, CodeError},
    gas_metering::Schedule,
};
use gprimitives::{CodeId, H256};
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
    async fn fetch_and_instrument(
        api: Api,
        latest_block: H256,
        code_id: CodeId,
    ) -> anyhow::Result<Self> {
        let original_code = api
            .original_code_storage_at(code_id, latest_block)
            .await
            .with_context(|| format!("Failed to fetch original code {code_id}"))?;

        let res = task::spawn_blocking(move || {
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
        .context("`gear_core::Code` panicked")?;

        Ok(Self { code_id, res })
    }

    fn err(self) -> Option<(CodeId, CodeError)> {
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

    tracing::info!("Fetching all code IDs");
    let keys = storage
        .fetch_raw_keys(storage_key)
        .await
        .context("Failed to obtain stream of keys")?
        .map(|key| key.context("Failed to get key"))
        .and_then(|key| {
            future::ready(
                CodeId::try_from(&key[storage_key_len..])
                    .map_err(|e| anyhow!("Failed to parse key: {e}")),
            )
        })
        .try_collect::<Vec<CodeId>>()
        .await?;
    let keys_len = keys.len();

    tracing::info!("Processing {keys_len} codes");
    let failed_codes = stream::iter(keys)
        .map(|code_id| {
            let api = api.clone();
            InstrumentationResult::fetch_and_instrument(api, latest_block, code_id)
        })
        .buffer_unordered(16)
        .chunks(100)
        .scan(0, |counter, chunk| {
            *counter += chunk.len();
            tracing::info!("[{counter}/{keys_len}] Instrumenting codes...");
            future::ready(Some(stream::iter(chunk)))
        })
        .flatten()
        .try_filter_map(|res| future::ready(Ok(res.err())))
        .try_collect::<Vec<_>>()
        .await?;

    for (code_id, err) in failed_codes {
        tracing::error!("{code_id} failed: {err}");
    }

    Ok(())
}
